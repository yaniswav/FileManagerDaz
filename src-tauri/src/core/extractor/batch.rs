//! Robust batch processing with progress tracking
//!
//! Handles large-scale import operations with error recovery,
//! progress reporting, and graceful degradation.

use crate::config::SETTINGS;
use crate::core::extractor::checkpoint::{cleanup_extracted_folders, Checkpoint};
use crate::core::extractor::process_source_recursive;
use crate::core::extractor::resilience::{
    validate_archive, BatchProcessor, ResilienceConfig, TimeoutGuard,
};
use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

/// Progress update for batch operations
#[derive(Debug, Clone, Serialize)]
pub struct BatchProgress {
    /// Total number of items to process
    pub total: usize,
    /// Number of items completed
    pub completed: usize,
    /// Number of successful items
    pub succeeded: usize,
    /// Number of failed items
    pub failed: usize,
    /// Currently processing item (if any)
    pub current_item: Option<String>,
    /// Estimated time remaining in seconds (if available)
    pub eta_seconds: Option<u64>,
}

/// Result of a batch operation
#[derive(Debug, Clone, Serialize)]
pub struct BatchOperationResult {
    /// Successful extractions with their results
    pub successes: Vec<BatchItemResult>,
    /// Failed items with error details
    pub failures: Vec<BatchItemFailure>,
    /// Overall statistics
    pub stats: BatchStats,
}

/// Individual item result
#[derive(Debug, Clone, Serialize)]
pub struct BatchItemResult {
    pub source_path: String,
    pub destination: String,
    pub files_count: usize,
    pub total_size: u64,
    pub duration_ms: u64,
}

/// Individual item failure
#[derive(Debug, Clone, Serialize)]
pub struct BatchItemFailure {
    pub source_path: String,
    pub error: String,
    pub error_code: String,
    pub skipped: bool,
}

/// Batch operation statistics
#[derive(Debug, Clone, Serialize)]
pub struct BatchStats {
    pub total_items: usize,
    pub successful: usize,
    pub failed: usize,
    pub total_files: usize,
    pub total_size_bytes: u64,
    pub duration_seconds: u64,
}

/// Batch processor with progress callback
pub struct RobustBatchProcessor {
    config: ResilienceConfig,
    progress_callback: Option<Arc<Mutex<dyn FnMut(BatchProgress) + Send>>>,
    checkpoint_dir: Option<PathBuf>,
    session_id: Option<String>,
    enable_cleanup: bool,
}

impl RobustBatchProcessor {
    /// Create a new batch processor
    pub fn new(config: ResilienceConfig) -> Self {
        Self {
            config,
            progress_callback: None,
            checkpoint_dir: None,
            session_id: None,
            enable_cleanup: true,
        }
    }

    /// Enable checkpoint/resume functionality
    pub fn with_checkpoint(mut self, checkpoint_dir: PathBuf, session_id: String) -> Self {
        self.checkpoint_dir = Some(checkpoint_dir);
        self.session_id = Some(session_id);
        self
    }

    /// Control temp directory cleanup
    pub fn with_cleanup(mut self, enable: bool) -> Self {
        self.enable_cleanup = enable;
        self
    }

    /// Set progress callback
    pub fn with_progress<F>(mut self, callback: F) -> Self
    where
        F: FnMut(BatchProgress) + Send + 'static,
    {
        self.progress_callback = Some(Arc::new(Mutex::new(callback)));
        self
    }

    /// Process multiple archives/folders
    pub fn process_batch(self, paths: Vec<PathBuf>) -> AppResult<BatchOperationResult> {
        // Detect already extracted archives BEFORE cleanup
        let already_extracted = if let Some(temp_dir) = self.get_temp_dir() {
            use crate::core::extractor::checkpoint::detect_already_extracted;
            let extracted = detect_already_extracted(&temp_dir, &paths);
            if !extracted.is_empty() {
                info!(
                    "🔍 Detected {} archives already extracted from previous run",
                    extracted.len()
                );
            }
            extracted
        } else {
            Vec::new()
        };

        // Cleanup extracted folders if enabled
        if self.enable_cleanup {
            if let Some(temp_dir) = self.get_temp_dir() {
                if let Err(e) = cleanup_extracted_folders(&temp_dir) {
                    warn!("Cleanup failed: {}", e);
                }
            }
        }

        // Load or create checkpoint
        let mut checkpoint = match &self.checkpoint_dir {
            Some(dir) if self.session_id.is_some() => {
                let session_id = self.session_id.as_ref().unwrap();
                match Checkpoint::load(dir, session_id) {
                    Ok(cp) => {
                        info!(
                            "Resuming from checkpoint: {} items already processed",
                            cp.processed.len()
                        );
                        cp
                    }
                    Err(_) => {
                        info!("Starting new checkpoint session: {}", session_id);
                        let mut new_checkpoint = Checkpoint::new(session_id.clone(), paths.len());

                        // Pre-mark already extracted archives as processed
                        for extracted_path in &already_extracted {
                            new_checkpoint.mark_processed(extracted_path);
                        }

                        if !already_extracted.is_empty() {
                            info!(
                                "✅ Pre-marked {} already extracted archives as processed",
                                already_extracted.len()
                            );
                        }

                        new_checkpoint
                    }
                }
            }
            _ => {
                let mut no_checkpoint = Checkpoint::new("no_checkpoint".to_string(), paths.len());

                // Even without persistent checkpoint, mark already extracted
                for extracted_path in &already_extracted {
                    no_checkpoint.mark_processed(extracted_path);
                }

                no_checkpoint
            }
        };

        // Filter out already processed items
        let remaining_paths = checkpoint.get_remaining(&paths);
        let total = paths.len();
        let already_processed = total - remaining_paths.len();

        if already_processed > 0 {
            info!("Skipping {} already processed items", already_processed);
        }

        info!(
            "Starting robust batch processing of {} remaining items (total: {})",
            remaining_paths.len(),
            total
        );

        let start_time = std::time::Instant::now();
        let mut completed = already_processed;
        let mut succeeded = checkpoint.processed.len();
        let failed = checkpoint.failed.len();

        let processor = BatchProcessor::new(remaining_paths, self.config.clone());

        let (successes, failures) = processor.process_all(|path| {
            // Update progress
            completed += 1;
            if let Some(ref callback) = self.progress_callback {
                if let Ok(mut cb) = callback.lock() {
                    cb(BatchProgress {
                        total,
                        completed,
                        succeeded,
                        failed,
                        current_item: Some(path.to_string_lossy().to_string()),
                        eta_seconds: None, // TODO: calculate based on average time
                    });
                }
            }

            // Validate before processing
            validate_archive(path, &self.config)?;

            // Create timeout guard
            let guard = TimeoutGuard::new(
                format!("Extract {}", path.display()),
                self.config.extraction_timeout,
            );

            // Process with recursive extraction
            info!("Processing: {:?}", path);
            let start = std::time::Instant::now();
            let result = process_source_recursive(path, 5)?;

            // Check timeout after processing
            guard.check_timeout()?;

            let duration_ms = start.elapsed().as_millis() as u64;
            info!(
                "Completed: {:?} ({} files, {} ms)",
                path, result.total_files, duration_ms
            );

            succeeded += 1;

            // Update checkpoint
            checkpoint.mark_processed(path);
            if let Some(ref dir) = self.checkpoint_dir {
                if let Err(e) = checkpoint.save(dir) {
                    warn!("Failed to save checkpoint: {}", e);
                }
            }

            Ok(BatchItemResult {
                source_path: path.to_string_lossy().to_string(),
                destination: result.destination.to_string_lossy().to_string(),
                files_count: result.total_files,
                total_size: result.total_size,
                duration_ms,
            })
        });

        let duration_seconds = start_time.elapsed().as_secs();

        // Calculate stats before consuming vectors
        let total_files: usize = successes.iter().map(|(_, r)| r.files_count).sum();
        let total_size: u64 = successes.iter().map(|(_, r)| r.total_size).sum();
        let success_count = successes.len();
        let failure_count = failures.len();

        let result = BatchOperationResult {
            successes: successes.into_iter().map(|(_, r)| r).collect(),
            failures: failures
                .into_iter()
                .map(|(path, error)| {
                    let error_msg = error.to_string();
                    let error_code = match error {
                        AppError::NotFound(_) => "FILE_NOT_FOUND",
                        AppError::ZipError(_)
                        | AppError::RarError(_)
                        | AppError::SevenZipError(_) => "EXTRACTION_ERROR",
                        AppError::UnsupportedFormat(_) | AppError::InvalidArchive(_) => {
                            "INVALID_FORMAT"
                        }
                        AppError::Config(_) => "CONFIG_ERROR",
                        _ => "UNKNOWN_ERROR",
                    };

                    // Update checkpoint for failures
                    checkpoint.mark_failed(&path, error_msg.clone());
                    if let Some(ref dir) = self.checkpoint_dir {
                        if let Err(e) = checkpoint.save(dir) {
                            warn!("Failed to save checkpoint: {}", e);
                        }
                    }

                    BatchItemFailure {
                        source_path: path.to_string_lossy().to_string(),
                        error: error_msg,
                        error_code: error_code.to_string(),
                        skipped: self.config.skip_corrupted,
                    }
                })
                .collect(),
            stats: BatchStats {
                total_items: total,
                successful: success_count,
                failed: failure_count,
                total_files,
                total_size_bytes: total_size,
                duration_seconds,
            },
        };

        info!(
            "Batch processing complete: {}/{} successful, {} failed in {} seconds",
            result.stats.successful,
            result.stats.total_items,
            result.stats.failed,
            result.stats.duration_seconds
        );

        // Delete checkpoint if all completed successfully
        if result.stats.failed == 0 {
            if let Some(ref dir) = self.checkpoint_dir {
                if let Err(e) = checkpoint.delete(dir) {
                    warn!("Failed to delete checkpoint: {}", e);
                }
            }
        }

        Ok(result)
    }

    /// Get temp directory for extractions
    fn get_temp_dir(&self) -> Option<PathBuf> {
        std::env::temp_dir().join("FileManagerDaz").into()
    }
}

/// Convenience function for batch processing with default settings
pub fn process_batch_with_defaults(paths: Vec<PathBuf>) -> AppResult<BatchOperationResult> {
    let settings = SETTINGS.read().unwrap();
    let config = settings.to_resilience_config();

    RobustBatchProcessor::new(config).process_batch(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_progress_tracking() {
        let progress = Arc::new(Mutex::new(Vec::new()));
        let progress_clone = progress.clone();

        let config = ResilienceConfig::default();
        let _processor = RobustBatchProcessor::new(config).with_progress(move |p| {
            progress_clone.lock().unwrap().push(p);
        });

        // Progress callback is set
        assert!(_processor.progress_callback.is_some());
    }
}
