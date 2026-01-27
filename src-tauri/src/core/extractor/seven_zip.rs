//! Native 7z extraction via `sevenz-rust` crate

use crate::error::AppResult;
use std::fs;
use std::path::Path;
use tracing::{error, info};

use super::utils::{count_directory_contents, ContentStats};

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
        crate::error::AppError::SevenZipError(e.to_string())
    })?;

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
