//! Native ZIP extraction via `zip` crate
//!
//! Uses a buffer to optimize I/O performance.

use super::utils::ContentStats;
use crate::error::AppResult;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter};
use std::path::Path;
use tracing::{debug, info, warn};

/// Buffer size for I/O operations (64 KB)
const BUFFER_SIZE: usize = 64 * 1024;

/// Extracts a ZIP archive to the destination folder
///
/// # Arguments
/// * `archive_path` - Path to the ZIP archive
/// * `dest_dir` - Destination directory
///
/// # Returns
/// ContentStats with extraction statistics
pub fn extract_zip(archive_path: &Path, dest_dir: &Path) -> AppResult<ContentStats> {
    info!("Extracting ZIP: {:?}", archive_path);

    fs::create_dir_all(dest_dir)?;

    let mut stats = ContentStats::new();

    // Scope to ensure file is closed before returning
    {
        // Open with a buffer for better performance
        let file = File::open(archive_path)?;
        let reader = BufReader::with_capacity(BUFFER_SIZE, file);
        let mut archive = zip::ZipArchive::new(reader)?;

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;

            // Security: use enclosed_name to avoid path traversal attacks
            let entry_path = match entry.enclosed_name() {
                Some(p) => p.to_owned(),
                None => {
                    warn!("Skipping entry with invalid name at index {}", i);
                    continue;
                }
            };

            let full_path = dest_dir.join(&entry_path);

            if entry.is_dir() {
                fs::create_dir_all(&full_path)?;
                stats.add_dir();
            } else {
                // Create parent folders if needed
                if let Some(parent) = full_path.parent() {
                    if !parent.exists() {
                        fs::create_dir_all(parent)?;
                    }
                }

                // Write with a buffer for better performance
                let outfile = File::create(&full_path)?;
                let mut writer = BufWriter::with_capacity(BUFFER_SIZE, outfile);
                let size = io::copy(&mut entry, &mut writer)?;

                stats.add_file(size);

                debug!("Extracted: {:?} ({} bytes)", entry_path, size);
            }
        }
    } // Archive dropped here, file handle closed

    // Small delay to ensure Windows releases the file handle
    // (important for subsequent operations like trash/delete)
    std::thread::sleep(std::time::Duration::from_millis(100));

    info!(
        "ZIP extraction complete: {} files, {} dirs, {} bytes",
        stats.files, stats.dirs, stats.size_bytes
    );

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn create_test_zip(dir: &Path) -> std::path::PathBuf {
        let zip_path = dir.join("test_archive.zip");
        let file = File::create(&zip_path).unwrap();
        let mut zip = ZipWriter::new(file);

        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

        zip.start_file("data/test.txt", options).unwrap();
        zip.write_all(b"Hello from DAZ content!").unwrap();

        zip.start_file("Runtime/textures/texture.png", options)
            .unwrap();
        zip.write_all(b"fake png data").unwrap();

        zip.add_directory("People/", options).unwrap();

        zip.finish().unwrap();
        zip_path
    }

    #[test]
    fn test_extract_zip() {
        let temp_dir = TempDir::new().unwrap();
        let zip_path = create_test_zip(temp_dir.path());
        let dest_dir = temp_dir.path().join("extracted");

        let stats = extract_zip(&zip_path, &dest_dir).unwrap();

        assert_eq!(stats.files, 2);
        assert!(stats.dirs >= 1); // At least People/
        assert!(stats.size_bytes > 0);
        assert!(dest_dir.join("data/test.txt").exists());
        assert!(dest_dir.join("Runtime/textures/texture.png").exists());
    }

    #[test]
    fn test_extract_zip_creates_dest_dir() {
        let temp_dir = TempDir::new().unwrap();
        let zip_path = create_test_zip(temp_dir.path());
        let dest_dir = temp_dir.path().join("nested/deep/dest");

        let result = extract_zip(&zip_path, &dest_dir);
        assert!(result.is_ok());
        assert!(dest_dir.exists());
    }
}
