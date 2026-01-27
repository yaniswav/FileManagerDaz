//! Checkpoint system for crash recovery
//!
//! Tracks processed items to enable resuming interrupted batch operations.

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};

/// Checkpoint data for batch operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Session ID
    pub session_id: String,
    /// Total items to process
    pub total_items: usize,
    /// Successfully processed items (by canonical path)
    pub processed: HashSet<String>,
    /// Failed items with error details
    pub failed: Vec<FailedItem>,
    /// Timestamp of last update
    pub last_update: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedItem {
    pub path: String,
    pub error: String,
    pub timestamp: u64,
}

impl Checkpoint {
    /// Create a new checkpoint
    pub fn new(session_id: String, total_items: usize) -> Self {
        Self {
            session_id,
            total_items,
            processed: HashSet::new(),
            failed: Vec::new(),
            last_update: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// Mark an item as processed
    pub fn mark_processed(&mut self, path: &Path) {
        let canonical = path.to_string_lossy().to_string();
        self.processed.insert(canonical);
        self.update_timestamp();
    }

    /// Mark an item as failed
    pub fn mark_failed(&mut self, path: &Path, error: String) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.failed.push(FailedItem {
            path: path.to_string_lossy().to_string(),
            error,
            timestamp,
        });
        self.update_timestamp();
    }

    /// Check if an item was already processed
    pub fn is_processed(&self, path: &Path) -> bool {
        let canonical = path.to_string_lossy().to_string();
        self.processed.contains(&canonical)
    }

    /// Get remaining items from a list
    pub fn get_remaining(&self, all_paths: &[PathBuf]) -> Vec<PathBuf> {
        all_paths
            .iter()
            .filter(|p| !self.is_processed(p))
            .cloned()
            .collect()
    }

    /// Calculate progress percentage
    pub fn progress_percent(&self) -> f64 {
        if self.total_items == 0 {
            return 100.0;
        }
        (self.processed.len() as f64 / self.total_items as f64) * 100.0
    }

    /// Update timestamp
    fn update_timestamp(&mut self) {
        self.last_update = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }

    /// Save checkpoint to disk
    pub fn save(&self, checkpoint_dir: &Path) -> AppResult<()> {
        fs::create_dir_all(checkpoint_dir).map_err(|e| {
            AppError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create checkpoint dir: {}", e),
            ))
        })?;

        let checkpoint_file = checkpoint_dir.join(format!("{}.json", self.session_id));
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| AppError::Internal(format!("Failed to serialize checkpoint: {}", e)))?;

        fs::write(&checkpoint_file, json).map_err(|e| {
            AppError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to write checkpoint: {}", e),
            ))
        })?;

        info!(
            "Checkpoint saved: {} items processed ({:.1}%)",
            self.processed.len(),
            self.progress_percent()
        );

        Ok(())
    }

    /// Load checkpoint from disk
    pub fn load(checkpoint_dir: &Path, session_id: &str) -> AppResult<Self> {
        let checkpoint_file = checkpoint_dir.join(format!("{}.json", session_id));

        if !checkpoint_file.exists() {
            return Err(AppError::NotFound(checkpoint_file));
        }

        let json = fs::read_to_string(&checkpoint_file).map_err(|e| {
            AppError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to read checkpoint: {}", e),
            ))
        })?;

        let checkpoint: Checkpoint = serde_json::from_str(&json)
            .map_err(|e| AppError::Internal(format!("Failed to deserialize checkpoint: {}", e)))?;

        info!(
            "Checkpoint loaded: {} items already processed ({:.1}%)",
            checkpoint.processed.len(),
            checkpoint.progress_percent()
        );

        Ok(checkpoint)
    }

    /// Find latest checkpoint in directory
    pub fn find_latest(checkpoint_dir: &Path) -> AppResult<Option<Self>> {
        if !checkpoint_dir.exists() {
            return Ok(None);
        }

        let entries = fs::read_dir(checkpoint_dir).map_err(|e| {
            AppError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to read checkpoint dir: {}", e),
            ))
        })?;

        let mut checkpoints = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                match fs::read_to_string(&path) {
                    Ok(json) => match serde_json::from_str::<Checkpoint>(&json) {
                        Ok(checkpoint) => checkpoints.push(checkpoint),
                        Err(e) => warn!("Failed to parse checkpoint {:?}: {}", path, e),
                    },
                    Err(e) => warn!("Failed to read checkpoint {:?}: {}", path, e),
                }
            }
        }

        if checkpoints.is_empty() {
            return Ok(None);
        }

        // Return checkpoint with most recent update
        checkpoints.sort_by_key(|c| c.last_update);
        Ok(Some(checkpoints.pop().unwrap()))
    }

    /// Delete checkpoint file
    pub fn delete(&self, checkpoint_dir: &Path) -> AppResult<()> {
        let checkpoint_file = checkpoint_dir.join(format!("{}.json", self.session_id));

        if checkpoint_file.exists() {
            fs::remove_file(&checkpoint_file).map_err(|e| {
                AppError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to delete checkpoint: {}", e),
                ))
            })?;
            info!("Checkpoint deleted: {}", self.session_id);
        }

        Ok(())
    }
}

/// Detect already extracted archives from temp directory
///
/// Scans temp directory for *_extracted folders and returns their source paths.
/// This allows resuming after a crash even without a checkpoint.
pub fn detect_already_extracted(temp_dir: &Path, source_paths: &[PathBuf]) -> Vec<PathBuf> {
    if !temp_dir.exists() {
        return Vec::new();
    }

    info!("Detecting already extracted archives in {:?}", temp_dir);

    let Ok(entries) = fs::read_dir(temp_dir) else {
        warn!("Failed to read temp directory");
        return Vec::new();
    };

    let mut extracted_names = std::collections::HashSet::new();

    for entry in entries.flatten() {
        let path = entry.path();

        // Find folders ending with _extracted
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.ends_with("_extracted") && path.is_dir() {
                // Remove _extracted suffix to get original name
                let original_name = name.trim_end_matches("_extracted");
                extracted_names.insert(original_name.to_string());
            }
        }
    }

    if extracted_names.is_empty() {
        return Vec::new();
    }

    info!("Found {} already extracted folders", extracted_names.len());

    // Match source paths with extracted names
    let mut already_extracted = Vec::new();

    for source_path in source_paths {
        if let Some(file_stem) = source_path.file_stem().and_then(|s| s.to_str()) {
            if extracted_names.contains(file_stem) {
                already_extracted.push(source_path.clone());
            }
        }
    }

    info!(
        "Matched {} source paths to extracted folders",
        already_extracted.len()
    );
    already_extracted
}

/// Cleanup extracted folders in temp directory
pub fn cleanup_extracted_folders(temp_dir: &Path) -> AppResult<usize> {
    if !temp_dir.exists() {
        return Ok(0);
    }

    info!("Cleaning up extracted folders in {:?}", temp_dir);

    let entries = fs::read_dir(temp_dir).map_err(|e| {
        AppError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to read temp dir: {}", e),
        ))
    })?;

    let mut cleaned = 0;

    for entry in entries.flatten() {
        let path = entry.path();

        // Check if it's an extracted folder (ends with _extracted)
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.ends_with("_extracted") && path.is_dir() {
                match fs::remove_dir_all(&path) {
                    Ok(_) => {
                        cleaned += 1;
                        if cleaned % 100 == 0 {
                            info!("Cleaned {} extracted folders...", cleaned);
                        }
                    }
                    Err(e) => {
                        error!("Failed to remove {:?}: {}", path, e);
                    }
                }
            }
        }
    }

    info!("Cleanup complete: {} extracted folders removed", cleaned);
    Ok(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_checkpoint_lifecycle() {
        let temp_dir = env::temp_dir().join("test_checkpoint");
        fs::create_dir_all(&temp_dir).unwrap();

        let session_id = "test_session".to_string();
        let mut checkpoint = Checkpoint::new(session_id.clone(), 10);

        let test_path = PathBuf::from("/test/path");
        checkpoint.mark_processed(&test_path);
        assert!(checkpoint.is_processed(&test_path));
        assert_eq!(checkpoint.progress_percent(), 10.0);

        checkpoint.save(&temp_dir).unwrap();
        let loaded = Checkpoint::load(&temp_dir, &session_id).unwrap();
        assert_eq!(loaded.processed.len(), 1);
        assert!(loaded.is_processed(&test_path));

        loaded.delete(&temp_dir).unwrap();
        assert!(!temp_dir.join(format!("{}.json", session_id)).exists());

        fs::remove_dir_all(&temp_dir).ok();
    }
}
