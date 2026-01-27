//! RAR extraction via external `unrar.exe` binary
//!
//! Note: Native RAR extraction in Rust is complex because libs require
//! the proprietary UnRAR DLL. We therefore use an external binary.

use crate::config::SETTINGS;
use crate::error::{AppError, AppResult};
use std::fs;
use std::path::Path;
use std::process::Command;
use tracing::{error, info};

use super::utils::{count_directory_contents, ContentStats};

/// Extracts a RAR archive via unrar.exe
///
/// # Arguments
/// * `archive_path` - Path to the RAR archive
/// * `dest_dir` - Destination directory
///
/// # Returns
/// `ContentStats` with file count, folder count and total size
///
/// # Errors
/// - `AppError::ExternalToolNotFound` if unrar.exe is not configured
/// - `AppError::RarError` if extraction fails
pub fn extract_rar(archive_path: &Path, dest_dir: &Path) -> AppResult<ContentStats> {
    info!("Extracting RAR: {:?}", archive_path);

    let settings = SETTINGS
        .read()
        .map_err(|e| AppError::Config(format!("Cannot read settings: {}", e)))?;

    let unrar_path = settings.unrar_path.clone().ok_or_else(|| {
        error!("UnRAR not found - RAR extraction unavailable");
        AppError::ExternalToolNotFound(
            "UnRAR.exe not found. Install WinRAR or place UnRAR.exe in the PATH.".to_string(),
        )
    })?;

    drop(settings); // Release lock before long operations

    fs::create_dir_all(dest_dir)?;

    // Execute unrar x archive.rar destination/
    let output = Command::new(&unrar_path)
        .arg("x") // Extract with full paths
        .arg("-o+") // Overwrite existing files
        .arg("-y") // Answer yes to everything
        .arg(archive_path)
        .arg(dest_dir)
        .output()
        .map_err(|e| {
            error!("Failed to execute UnRAR: {}", e);
            AppError::RarError(format!("Failed to execute UnRAR: {}", e))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("UnRAR failed: {}", stderr);
        return Err(AppError::RarError(format!(
            "UnRAR failed (code {}): {}",
            output.status.code().unwrap_or(-1),
            stderr
        )));
    }

    // Count extracted content
    let stats = count_directory_contents(dest_dir)?;

    info!(
        "RAR extraction complete: {} files, {} dirs, {} bytes",
        stats.files, stats.dirs, stats.size_bytes
    );

    Ok(stats)
}

#[cfg(test)]
mod tests {
    // RAR tests require unrar.exe and a real RAR archive
    #[test]
    fn test_module_compiles() {
        assert!(true);
    }
}
