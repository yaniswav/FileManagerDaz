//! Utility for moving files to Windows Recycle Bin
//!
//! Uses Windows Shell API (IFileOperation) via the `trash` crate.

use crate::error::{AppError, AppResult};
use std::path::Path;
use tracing::{info, warn};

/// Moves a file to Windows Recycle Bin
///
/// # Arguments
/// * `path` - Path of the file to put in trash
///
/// # Returns
/// * `Ok(true)` if the file was moved successfully
/// * `Err(...)` if an error occurred
///
/// # Note
/// On Windows, uses the system Recycle Bin.
/// The file can be restored by the user.
pub fn move_to_trash(path: &Path) -> AppResult<bool> {
    if !path.exists() {
        warn!("Cannot trash non-existent file: {:?}", path);
        return Err(AppError::NotFound(path.to_path_buf()));
    }

    if !path.is_file() {
        warn!("Cannot trash non-file path: {:?}", path);
        return Err(AppError::InvalidPath(format!(
            "Only files can be moved to trash: {}",
            path.display()
        )));
    }

    info!("Moving to trash: {:?}", path);

    // Retry logic for file locks (antivirus, Windows Defender, etc.)
    let max_retries = 3;
    let mut last_error = None;

    for attempt in 1..=max_retries {
        match trash::delete(path) {
            Ok(_) => {
                if attempt > 1 {
                    info!(
                        "Successfully moved to trash after {} attempts: {:?}",
                        attempt, path
                    );
                } else {
                    info!("Successfully moved to trash: {:?}", path);
                }
                return Ok(true);
            }
            Err(e) => {
                let error_msg = e.to_string();
                let is_lock_error = error_msg.contains("being used by another process")
                    || error_msg.contains("sharing violation")
                    || error_msg.contains("locked")
                    || error_msg.contains("in use");

                if attempt < max_retries {
                    let delay_ms = if is_lock_error {
                        // Longer delay for locked files
                        500 * attempt as u64
                    } else {
                        200 * attempt as u64
                    };

                    warn!(
                        "Failed to move to trash (attempt {}/{}): {}. Retrying in {}ms...",
                        attempt, max_retries, error_msg, delay_ms
                    );

                    std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                } else {
                    last_error = Some(e);
                }
            }
        }
    }

    // All retries failed
    let err = last_error.unwrap();
    warn!(
        "Failed to move to trash after {} attempts: {}",
        max_retries, err
    );
    Err(AppError::Io(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("Failed to move to trash: {}", err),
    )))
}

/// Checks if a path is a supported archive
pub fn is_archive_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .map(|ext| matches!(ext.as_str(), "zip" | "rar" | "7z"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_archive_file() {
        assert!(is_archive_file(Path::new("test.zip")));
        assert!(is_archive_file(Path::new("test.ZIP")));
        assert!(is_archive_file(Path::new("test.rar")));
        assert!(is_archive_file(Path::new("test.7z")));
        assert!(!is_archive_file(Path::new("test.txt")));
        assert!(!is_archive_file(Path::new("test.duf")));
        assert!(!is_archive_file(Path::new("folder")));
    }
}
