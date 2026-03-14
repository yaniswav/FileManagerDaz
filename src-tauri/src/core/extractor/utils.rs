//! Shared utilities for extraction

use crate::error::AppResult;
use serde::Serialize;
use std::fs;
use std::ops::Add;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

// ============================================================================
// ContentStats - Factored content statistics
// ============================================================================

/// Content statistics (files, directories, size)
///
/// Replaces the repetitive (file_count, dir_count, total_size) triplets.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct ContentStats {
    /// Number of files
    pub files: usize,
    /// Number of directories
    pub dirs: usize,
    /// Total size in bytes
    pub size_bytes: u64,
}

impl ContentStats {
    /// Creates empty stats
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates stats with given values
    #[allow(dead_code)]
    pub fn with_values(files: usize, dirs: usize, size_bytes: u64) -> Self {
        Self {
            files,
            dirs,
            size_bytes,
        }
    }

    /// Merges two stats (adds values)
    pub fn merge(&self, other: &ContentStats) -> Self {
        Self {
            files: self.files + other.files,
            dirs: self.dirs + other.dirs,
            size_bytes: self.size_bytes + other.size_bytes,
        }
    }

    /// Adds a file to stats
    pub fn add_file(&mut self, size: u64) {
        self.files += 1;
        self.size_bytes += size;
    }

    /// Adds a directory to stats
    pub fn add_dir(&mut self) {
        self.dirs += 1;
    }

    /// Conversion to legacy triplet (for compatibility)
    #[allow(dead_code)]
    pub fn as_tuple(&self) -> (usize, usize, u64) {
        (self.files, self.dirs, self.size_bytes)
    }
}

impl Add for ContentStats {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        self.merge(&other)
    }
}

// ============================================================================
// Utility functions
// ============================================================================

/// Counts files, directories and total size of a directory
pub fn count_directory_contents(dir: &Path) -> AppResult<ContentStats> {
    let mut stats = ContentStats::new();

    for entry in WalkDir::new(dir).min_depth(1) {
        let entry = entry?;
        if entry.file_type().is_file() {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            stats.add_file(size);
        } else if entry.file_type().is_dir() {
            stats.add_dir();
        }
    }

    Ok(stats)
}

/// Gets top-level entries of a folder
pub fn get_root_entries(dir: &Path) -> AppResult<Vec<String>> {
    let mut entries = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if let Some(name) = entry.file_name().to_str() {
            entries.push(name.to_string());
        }
    }

    entries.sort();
    Ok(entries)
}

/// Merges two directories recursively (source -> destination)
///
/// Optimization: tries fs::rename first (instant on same volume),
/// then falls back to fs::copy if rename fails (cross-volume).
pub fn merge_directories(src: &Path, dest: &Path) -> AppResult<()> {
    fs::create_dir_all(dest)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let entry_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        if entry_path.is_dir() {
            merge_directories(&entry_path, &dest_path)?;
        } else {
            // Optimization: try rename first (instant on same volume)
            // If destination already exists, remove it first
            if dest_path.exists() {
                let _ = fs::remove_file(&dest_path);
            }

            // Try rename (free on same volume)
            if fs::rename(&entry_path, &dest_path).is_err() {
                // Fallback to copy (cross-volume or other error)
                fs::copy(&entry_path, &dest_path)?;
                // Remove source after successful copy to complete the "move"
                let _ = fs::remove_file(&entry_path);
            }
        }
    }

    Ok(())
}

/// Formats a byte size as a readable string
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Finds all archives in a folder (recursive up to depth 10)
///
/// Skips secondary parts of multi-part archives to avoid double-processing.
pub fn find_archives_in_dir(dir: &Path) -> AppResult<Vec<PathBuf>> {
    use super::ArchiveFormat;
    use super::multipart;

    let mut archives = Vec::new();

    for entry in WalkDir::new(dir).min_depth(1).max_depth(10) {
        let entry = entry?;
        if entry.file_type().is_file() {
            if ArchiveFormat::from_extension(entry.path()).is_some() {
                // Skip secondary parts of multi-part archives
                if multipart::is_secondary_part(entry.path()) {
                    continue;
                }
                archives.push(entry.path().to_path_buf());
            }
        }
    }

    Ok(archives)
}

// ============================================================================
// DAZ structure normalization
// ============================================================================

/// Standard DAZ folders at the root of a library
const DAZ_STANDARD_FOLDERS: &[&str] = &[
    "data",
    "People",
    "Runtime",
    "Documentation",
    "Environments",
    "General",
    "Light Presets",
    "Props",
    "Render Presets",
    "Render Settings",
    "Scripts",
    "Shader Presets",
    "Templates",
    "aniBlocks",
    // Common variants
    "Content",
    "content",
];

/// Checks if a folder contains standard DAZ folders
fn has_daz_standard_folders(dir: &Path) -> bool {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                if let Some(name) = entry.file_name().to_str() {
                    if DAZ_STANDARD_FOLDERS.contains(&name) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Normalizes the structure of an extracted folder to remove unnecessary wrappers
///
/// Handles the following cases:
/// - Single "Content" folder containing data/, People/, Runtime/ etc.
/// - Single folder with product name containing data/, People/, Runtime/ etc.
/// - Multiple nesting levels (e.g. ProductName/Content/data/)
///
/// # Returns
/// `true` if normalization was performed
pub fn normalize_daz_structure(dir: &Path) -> AppResult<bool> {
    use tracing::{debug, warn};

    let mut normalized = false;
    let mut iterations = 0;
    const MAX_ITERATIONS: usize = 5;

    // Repeat until we can no longer normalize
    while iterations < MAX_ITERATIONS {
        iterations += 1;

        // Count entries at root
        let entries: Vec<_> = fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();

        // Separate folders and files
        let (folders, files): (Vec<_>, Vec<_>) = entries
            .into_iter()
            .partition(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false));

        // If already has DAZ standard folders at root, nothing to do
        if has_daz_standard_folders(dir) {
            // Unless we have a "Content" folder to unwrap
            if let Some(content_entry) = folders.iter().find(|e| {
                e.file_name()
                    .to_str()
                    .map(|n| n.eq_ignore_ascii_case("content"))
                    .unwrap_or(false)
            }) {
                let content_path = content_entry.path();
                if has_daz_standard_folders(&content_path) {
                    debug!(
                        "Found nested Content folder, unwrapping: {:?}",
                        content_path
                    );
                    unwrap_folder_contents(&content_path, dir)?;
                    if let Err(e) = fs::remove_dir_all(&content_path) {
                        warn!("Could not remove Content wrapper: {}", e);
                    }
                    normalized = true;
                    continue; // Re-check after this normalization
                }
            }
            break;
        }

        // Remove standalone files (README, license, promo images) at root
        let mut removed_files = false;
        for file_entry in &files {
            let file_path = file_entry.path();
            if let Some(file_name) = file_path.file_name().and_then(|n| n.to_str()) {
                let name_lower = file_name.to_lowercase();
                if name_lower.contains("readme")
                    || name_lower.contains("license")
                    || name_lower.contains("promo")
                    || name_lower.ends_with(".txt")
                    || name_lower.ends_with(".pdf")
                    || name_lower.ends_with(".html")
                    || name_lower.ends_with(".png")
                    || name_lower.ends_with(".jpg")
                    || name_lower.ends_with(".jpeg")
                {
                    debug!("Removing standalone file: {:?}", file_name);
                    if let Err(e) = fs::remove_file(&file_path) {
                        warn!("Could not remove file {:?}: {}", file_name, e);
                    } else {
                        removed_files = true;
                    }
                }
            }
        }

        if removed_files {
            normalized = true;
            continue; // Re-check after cleanup
        }

        // If single folder and 0-1 file (readme, license, etc.), check if we can unwrap
        if folders.len() == 1 {
            let single_folder = &folders[0];
            let folder_path = single_folder.path();

            // Check if this folder contains DAZ standard folders
            if has_daz_standard_folders(&folder_path) {
                debug!(
                    "Found wrapper folder with DAZ content, unwrapping: {:?}",
                    folder_path
                );

                // Move the wrapper contents to the parent
                unwrap_folder_contents(&folder_path, dir)?;

                // Remove the empty wrapper
                if let Err(e) = fs::remove_dir_all(&folder_path) {
                    warn!("Could not remove wrapper folder: {}", e);
                }

                normalized = true;
                continue; // Re-check after this normalization
            }
        }

        // No normalization possible
        break;
    }

    Ok(normalized)
}

/// Moves the contents of a folder to another (unwrap)
fn unwrap_folder_contents(src: &Path, dest: &Path) -> AppResult<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let entry_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        if entry_path.is_dir() {
            // If the folder already exists, merge
            if dest_path.exists() {
                merge_directories(&entry_path, &dest_path)?;
            } else {
                // Otherwise, rename/move
                if fs::rename(&entry_path, &dest_path).is_err() {
                    // Cross-volume: copy then delete
                    merge_directories(&entry_path, &dest_path)?;
                }
            }
        } else {
            // File: move/copy
            if dest_path.exists() {
                let _ = fs::remove_file(&dest_path);
            }
            if fs::rename(&entry_path, &dest_path).is_err() {
                fs::copy(&entry_path, &dest_path)?;
            }
        }
    }

    Ok(())
}
