//! Robust batch processing with progress tracking
//!
//! Handles large-scale import operations with error recovery,
//! progress reporting, and graceful degradation.

use crate::config::settings::AppSettings;
use crate::core::extractor::checkpoint::{cleanup_extracted_folders, Checkpoint};
use crate::core::extractor::process_source_recursive;
use crate::core::extractor::resilience::{
    validate_archive, BatchProcessor, ResilienceConfig, TimeoutGuard,
};
use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::cell::Cell;
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

/// Progress callback shared between the batch processor and any caller-side
/// observer; held in an Arc<Mutex<...>> so it can be cloned into Rayon tasks.
type ProgressCallback = Arc<Mutex<dyn FnMut(BatchProgress) + Send>>;

/// Batch processor with progress callback
pub struct RobustBatchProcessor {
    config: ResilienceConfig,
    settings: AppSettings,
    progress_callback: Option<ProgressCallback>,
    checkpoint_dir: Option<PathBuf>,
    session_id: Option<String>,
    enable_cleanup: bool,
}

impl RobustBatchProcessor {
    /// Create a new batch processor
    pub fn new(config: ResilienceConfig, settings: AppSettings) -> Self {
        Self {
            config,
            settings,
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
        // Counters share state between the per-attempt closure (which emits
        // progress events) and the per-item completion callback (which knows
        // the definitive Ok/Err of an item after retries). Cell keeps both
        // closures `FnMut` without fighting the borrow checker.
        let completed = Cell::new(already_processed);
        let succeeded = Cell::new(checkpoint.processed.len());
        let failed = Cell::new(checkpoint.failed.len());

        let processor = BatchProcessor::new(remaining_paths, self.config.clone());

        let (successes, failures) = processor.process_all_with_progress(
            |path| {
                // Update progress for the item that is about to start.
                let now_completed = completed.get() + 1;
                completed.set(now_completed);

                // ETA based on items that have actually finished so far. At callback
                // time `completed` was just bumped for the *current* item (which is
                // about to start), so `completed - 1` items have finished and the
                // elapsed time reflects their processing.
                let eta_seconds = {
                    let finished = now_completed.saturating_sub(1);
                    if finished > 0 && total > finished {
                        let elapsed = start_time.elapsed().as_secs_f64();
                        let avg_per_item = elapsed / finished as f64;
                        let remaining = (total - finished) as f64;
                        // Floor at 1s while work remains, otherwise the UI flips
                        // to "0s" on very fast items even though more are queued.
                        Some(((avg_per_item * remaining).round() as u64).max(1))
                    } else {
                        None
                    }
                };

                if let Some(ref callback) = self.progress_callback {
                    if let Ok(mut cb) = callback.lock() {
                        cb(BatchProgress {
                            total,
                            completed: now_completed,
                            succeeded: succeeded.get(),
                            failed: failed.get(),
                            current_item: Some(path.to_string_lossy().to_string()),
                            eta_seconds,
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
                let result = process_source_recursive(path, 5, &self.settings)?;

                // Check timeout after processing
                guard.check_timeout()?;

                let duration_ms = start.elapsed().as_millis() as u64;
                info!(
                    "Completed: {:?} ({} files, {} ms)",
                    path, result.total_files, duration_ms
                );

                Ok(BatchItemResult {
                    source_path: path.to_string_lossy().to_string(),
                    destination: result.destination.to_string_lossy().to_string(),
                    files_count: result.total_files,
                    total_size: result.total_size,
                    duration_ms,
                })
            },
            |path, result| {
                // Called once per item after RetryStrategy is done. This is
                // where `succeeded` / `failed` get their definitive bump —
                // doing it inside the closure above would double-count any
                // retry attempt that failed before the final outcome.
                //
                // Both branches persist the checkpoint streaming-style so a
                // crash mid-batch keeps the partial progress recoverable.
                match result {
                    Ok(_) => {
                        succeeded.set(succeeded.get() + 1);
                        checkpoint.mark_processed(path);
                    }
                    Err(e) => {
                        failed.set(failed.get() + 1);
                        checkpoint.mark_failed(path, e.to_string());
                    }
                }
                if let Some(ref dir) = self.checkpoint_dir {
                    if let Err(save_err) = checkpoint.save(dir) {
                        warn!("Failed to save checkpoint: {}", save_err);
                    }
                }
            },
        );

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
                        AppError::UnsupportedFormat(_) => "INVALID_FORMAT",
                        AppError::Config(_) => "CONFIG_ERROR",
                        _ => "UNKNOWN_ERROR",
                    };

                    // Checkpoint persistence happens streaming in `on_item_done`
                    // above — no need to mark_failed again here.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_progress_tracking() {
        let progress = Arc::new(Mutex::new(Vec::new()));
        let progress_clone = progress.clone();

        let config = ResilienceConfig::default();
        let settings = AppSettings::default();
        let _processor = RobustBatchProcessor::new(config, settings).with_progress(move |p| {
            progress_clone
                .lock()
                .expect("progress mutex poisoned in test")
                .push(p);
        });

        // Progress callback is set
        assert!(_processor.progress_callback.is_some());
    }

    /// Regression test: the `failed` counter inside `BatchProgress` events
    /// used to be loaded from the checkpoint at start and never incremented
    /// at runtime — the UI stayed stuck on whatever value the checkpoint
    /// had (typically 0) even when items failed mid-batch.
    #[test]
    fn failed_counter_increments_on_runtime_failures() {
        use tempfile::TempDir;

        let progress: Arc<Mutex<Vec<BatchProgress>>> = Arc::new(Mutex::new(Vec::new()));
        let progress_clone = progress.clone();

        // max_retries = 1 → each item fails immediately, no retry storm;
        // skip_corrupted = true → the batch keeps going after a failure so
        // we get to observe more than one progress event.
        let config = ResilienceConfig {
            max_retries: 1,
            ..ResilienceConfig::default()
        };
        let settings = AppSettings::default();
        let processor = RobustBatchProcessor::new(config, settings)
            .with_progress(move |p| {
                progress_clone
                    .lock()
                    .expect("progress mutex poisoned in test")
                    .push(p);
            })
            .with_cleanup(false);

        // Two non-existent paths: `validate_archive` returns
        // `AppError::NotFound` before any extraction runs.
        let tmp = TempDir::new().unwrap();
        let bad1 = tmp.path().join("does_not_exist_1.zip");
        let bad2 = tmp.path().join("does_not_exist_2.zip");

        let result = processor.process_batch(vec![bad1, bad2]);
        assert!(
            result.is_ok(),
            "process_batch should return Ok (with failures in the result) when skip_corrupted is true; got: {:?}",
            result.err()
        );

        let outcome = result.unwrap();
        assert_eq!(outcome.failures.len(), 2, "both items should have failed");
        assert_eq!(outcome.successes.len(), 0);

        let events = progress.lock().unwrap();
        let max_failed = events.iter().map(|p| p.failed).max().unwrap_or(0);
        assert!(
            max_failed >= 1,
            "expected at least one BatchProgress with failed >= 1; got events: {:?}",
            events
                .iter()
                .map(|p| (p.completed, p.succeeded, p.failed))
                .collect::<Vec<_>>()
        );
    }

    /// Regression test: `checkpoint.mark_failed` used to be called only in
    /// the post-batch failures loop, so a crash mid-batch lost the partial
    /// failure record. The fix moves the mark into `on_item_done`, which
    /// also `checkpoint.save()`s after each item. This test asserts that
    /// the on-disk checkpoint already contains the failures by the time
    /// `process_batch` returns — proving the streaming-style persistence
    /// path (each failure is durable as soon as it is observed).
    #[test]
    fn failed_items_are_streamed_to_checkpoint() {
        use crate::core::extractor::checkpoint::Checkpoint;
        use tempfile::TempDir;

        let checkpoint_dir = TempDir::new().unwrap();
        let session_id = "test-session".to_string();

        let config = ResilienceConfig {
            max_retries: 1,
            ..ResilienceConfig::default()
        };
        let settings = AppSettings::default();
        let processor = RobustBatchProcessor::new(config, settings)
            .with_checkpoint(checkpoint_dir.path().to_path_buf(), session_id.clone())
            .with_cleanup(false);

        let work_dir = TempDir::new().unwrap();
        let bad1 = work_dir.path().join("nope-1.zip");
        let bad2 = work_dir.path().join("nope-2.zip");

        let outcome = processor.process_batch(vec![bad1, bad2]).unwrap();
        assert_eq!(outcome.failures.len(), 2);

        // Reload the checkpoint from disk to confirm the failures were
        // persisted streaming-style (i.e. during `on_item_done`, not only
        // in a post-loop that a crash could skip).
        let persisted = Checkpoint::load(checkpoint_dir.path(), &session_id).unwrap();
        assert_eq!(
            persisted.failed.len(),
            2,
            "both failures must be persisted to the checkpoint file"
        );
    }
}
