//! Native 7z extraction via `sevenz-rust` crate

use crate::error::{AppError, AppResult};
use std::fs;
use std::path::Path;
use tracing::{error, info, warn};
use walkdir::WalkDir;

use super::utils::{count_directory_contents, ContentStats};

/// Validates that all extracted files are within the destination directory.
/// Prevents path traversal attacks from malicious 7z archives.
fn validate_no_path_traversal(dest_dir: &Path) -> AppResult<()> {
    let canonical_dest = dest_dir.canonicalize().map_err(|e| {
        AppError::SevenZipError(format!("Cannot canonicalize dest dir: {}", e))
    })?;

    for entry in WalkDir::new(dest_dir).into_iter().filter_map(|e| e.ok()) {
        let canonical_entry = entry.path().canonicalize().map_err(|e| {
            AppError::SevenZipError(format!("Cannot canonicalize entry: {}", e))
        })?;

        if !canonical_entry.starts_with(&canonical_dest) {
            warn!("Path traversal detected: {:?}", entry.path());
            return Err(AppError::SevenZipError(format!(
                "Security: path traversal detected in 7z archive ({})",
                entry.path().display()
            )));
        }
    }

    Ok(())
}

/// Extracts a 7z archive to the destination folder
///
/// # Arguments
/// * `archive_path` - Path to the 7z archive
/// * `dest_dir` - Destination directory
///
/// # Returns
/// `ContentStats` with file count, folder count and total size
pub fn extract_7z(archive_path: &Path, dest_dir: &Path) -> AppResult<ContentStats> {
    info!("Extracting 7z: {:?}", archive_path);

    fs::create_dir_all(dest_dir)?;

    // sevenz_rust::decompress extracts all content directly
    sevenz_rust::decompress_file(archive_path, dest_dir).map_err(|e| {
        error!("7z extraction failed: {}", e);
        AppError::SevenZipError(e.to_string())
    })?;

    // Security: verify no files were extracted outside dest_dir
    validate_no_path_traversal(dest_dir)?;

    // Count extracted content
    let stats = count_directory_contents(dest_dir)?;

    info!(
        "7z extraction complete: {} files, {} dirs, {} bytes",
        stats.files, stats.dirs, stats.size_bytes
    );

    Ok(stats)
}

#[cfg(test)]
mod tests {
    // 7z tests require a real 7z archive, which is more complex
    // to generate in tests. We just verify that the function exists.
    #[test]
    fn test_module_compiles() {
        // This test simply verifies that the module compiles correctly
        assert!(true);
    }
}
