//! DAZ library maintenance module
//!
//! - Duplicate detection (files with same hash or name)
//! - Orphan file detection (not in a referenced product)
//! - Guided cleanup with backup

use crate::config::SETTINGS;
use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use tracing::info;
use walkdir::WalkDir;

// ============================================================================
// Public types
// ============================================================================

/// Detected maintenance issue
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum MaintenanceIssue {
    /// Duplicate file (same hash)
    Duplicate {
        path: String,
        duplicate_of: String,
        size: u64,
        hash: String,
    },
    /// File with similar name (potential duplicate)
    SimilarName {
        path: String,
        similar_to: String,
        similarity: f64,
    },
    /// Orphan file (not in a referenced product)
    #[allow(dead_code)]
    Orphan {
        path: String,
        size: u64,
        file_type: String,
    },
    /// Empty folder
    EmptyFolder { path: String },
    /// Temporary / cache file
    TempFile { path: String, size: u64 },
}

/// Maintenance analysis summary
#[derive(Debug, Clone, Serialize)]
pub struct MaintenanceSummary {
    /// Total number of files scanned
    pub total_files_scanned: usize,
    /// Total size scanned
    pub total_size_scanned: u64,
    /// Issues found
    pub issues: Vec<MaintenanceIssue>,
    /// Recoverable space (if duplicates/orphans are deleted)
    pub recoverable_space: u64,
    /// Scan duration (ms)
    pub scan_duration_ms: u64,
}

/// Scan options
#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Detect duplicates by hash
    pub detect_duplicates: bool,
    /// Detect similar names
    pub detect_similar_names: bool,
    /// Detect orphans
    #[allow(dead_code)]
    pub detect_orphans: bool,
    /// Detect empty folders
    pub detect_empty_folders: bool,
    /// Detect temporary files
    pub detect_temp_files: bool,
    /// Extensions to ignore
    pub ignore_extensions: Vec<String>,
    /// Minimum size for hash (ignore small files)
    pub min_size_for_hash: u64,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            detect_duplicates: true,
            detect_similar_names: false,
            detect_orphans: true,
            detect_empty_folders: true,
            detect_temp_files: true,
            ignore_extensions: vec![
                ".db".to_string(),
                ".db-journal".to_string(),
                ".log".to_string(),
            ],
            min_size_for_hash: 1024, // 1 KB minimum
        }
    }
}

/// Cleanup action result
#[derive(Debug, Clone, Serialize)]
pub struct CleanupResult {
    pub success: bool,
    pub files_deleted: usize,
    pub folders_deleted: usize,
    pub space_freed: u64,
    pub errors: Vec<String>,
    pub backup_path: Option<String>,
}

// ============================================================================
// DAZ file extensions
// ============================================================================

#[allow(dead_code)]
const DAZ_EXTENSIONS: &[&str] = &[
    ".duf", ".dsf", ".dse", ".daz", ".dsa", ".ds", ".dsx", ".dbz", ".png", ".jpg", ".jpeg", ".tif",
    ".tiff", ".exr", ".hdr", ".obj", ".fbx", ".dae",
];

const TEMP_PATTERNS: &[&str] = &[
    "thumbs.db",
    "desktop.ini",
    ".ds_store",
    "~$",
    ".tmp",
    ".bak",
    ".cache",
];

// ============================================================================
// Maintenance scanner
// ============================================================================

/// Scans a library to detect maintenance issues
pub fn scan_library(library_path: &Path, options: &ScanOptions) -> AppResult<MaintenanceSummary> {
    let start = std::time::Instant::now();

    info!(
        "Scanning library for maintenance issues: {:?}",
        library_path
    );

    if !library_path.exists() {
        return Err(AppError::NotFound(library_path.to_path_buf()));
    }

    let mut issues = Vec::new();
    let mut total_files = 0usize;
    let mut total_size = 0u64;
    let mut recoverable = 0u64;

    // Hash -> (first path found, size)
    let mut hash_map: HashMap<String, (String, u64)> = HashMap::new();
    // Normalized filename -> full path
    let mut name_map: HashMap<String, String> = HashMap::new();
    // Folders with content (to detect empty ones)
    let mut non_empty_folders: std::collections::HashSet<PathBuf> =
        std::collections::HashSet::new();

    // Traverse library
    for entry in WalkDir::new(library_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // Mark parent as non-empty
        if let Some(parent) = path.parent() {
            non_empty_folders.insert(parent.to_path_buf());
        }

        // Ignore directories for most checks
        if path.is_dir() {
            continue;
        }

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_lowercase();

        // Ignore certain extensions
        if options
            .ignore_extensions
            .iter()
            .any(|ext| file_name.ends_with(ext))
        {
            continue;
        }

        // File size
        let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        total_files += 1;
        total_size += size;

        // Temporary file detection
        if options.detect_temp_files {
            if is_temp_file(&file_name) {
                issues.push(MaintenanceIssue::TempFile {
                    path: path.to_string_lossy().to_string(),
                    size,
                });
                recoverable += size;
                continue; // No need for other checks
            }
        }

        // Duplicate detection by hash
        if options.detect_duplicates && size >= options.min_size_for_hash {
            if let Ok(hash) = compute_fast_hash(path) {
                let path_str = path.to_string_lossy().to_string();
                if let Some((original_path, _original_size)) = hash_map.get(&hash) {
                    issues.push(MaintenanceIssue::Duplicate {
                        path: path_str,
                        duplicate_of: original_path.clone(),
                        size,
                        hash: hash.clone(),
                    });
                    recoverable += size;
                } else {
                    hash_map.insert(hash, (path_str, size));
                }
            }
        }

        // Similar name detection
        if options.detect_similar_names {
            let normalized = normalize_filename(&file_name);
            let path_str = path.to_string_lossy().to_string();

            if let Some(similar_path) = name_map.get(&normalized) {
                if similar_path != &path_str {
                    issues.push(MaintenanceIssue::SimilarName {
                        path: path_str.clone(),
                        similar_to: similar_path.clone(),
                        similarity: 0.9, // Simplification
                    });
                }
            } else {
                name_map.insert(normalized, path_str);
            }
        }
    }

    // Empty folder detection
    if options.detect_empty_folders {
        for entry in WalkDir::new(library_path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_dir() && path != library_path && !non_empty_folders.contains(path) {
                // Verify the folder is truly empty
                if fs::read_dir(path)
                    .map(|mut d| d.next().is_none())
                    .unwrap_or(false)
                {
                    issues.push(MaintenanceIssue::EmptyFolder {
                        path: path.to_string_lossy().to_string(),
                    });
                }
            }
        }
    }

    let duration = start.elapsed().as_millis() as u64;

    info!(
        "Maintenance scan complete: {} files, {} issues, {} recoverable",
        total_files,
        issues.len(),
        format_size(recoverable)
    );

    Ok(MaintenanceSummary {
        total_files_scanned: total_files,
        total_size_scanned: total_size,
        issues,
        recoverable_space: recoverable,
        scan_duration_ms: duration,
    })
}

/// Quick scan of all libraries
pub fn scan_all_libraries(options: &ScanOptions) -> AppResult<MaintenanceSummary> {
    let settings = SETTINGS
        .read()
        .map_err(|_| AppError::Config("Lock error".into()))?;
    let libraries = settings.daz_libraries.clone();
    drop(settings);

    let mut combined = MaintenanceSummary {
        total_files_scanned: 0,
        total_size_scanned: 0,
        issues: Vec::new(),
        recoverable_space: 0,
        scan_duration_ms: 0,
    };

    for lib in &libraries {
        if let Ok(summary) = scan_library(Path::new(lib), options) {
            combined.total_files_scanned += summary.total_files_scanned;
            combined.total_size_scanned += summary.total_size_scanned;
            combined.issues.extend(summary.issues);
            combined.recoverable_space += summary.recoverable_space;
            combined.scan_duration_ms += summary.scan_duration_ms;
        }
    }

    Ok(combined)
}

// ============================================================================
// Cleanup actions
// ============================================================================

/// Deletes selected files with optional backup
pub fn cleanup_files(
    files: &[String],
    backup: bool,
    backup_dir: Option<&Path>,
) -> AppResult<CleanupResult> {
    let mut result = CleanupResult {
        success: true,
        files_deleted: 0,
        folders_deleted: 0,
        space_freed: 0,
        errors: Vec::new(),
        backup_path: None,
    };

    // Create the backup folder if needed
    let backup_base = if backup {
        let dir = backup_dir.map(|p| p.to_path_buf()).unwrap_or_else(|| {
            let settings = SETTINGS.read().unwrap();
            PathBuf::from(&settings.temp_dir).join("maintenance_backup")
        });

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let backup_path = dir.join(format!("backup_{}", timestamp));
        fs::create_dir_all(&backup_path)?;
        result.backup_path = Some(backup_path.to_string_lossy().to_string());
        Some(backup_path)
    } else {
        None
    };

    for file_path in files {
        let path = Path::new(file_path);

        if !path.exists() {
            continue;
        }

        // Taille avant suppression
        let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);

        // Backup if requested
        if let Some(ref backup_base) = backup_base {
            let relative = path.file_name().unwrap_or_default();
            let backup_path = backup_base.join(relative);

            if let Err(e) = fs::copy(path, &backup_path) {
                result
                    .errors
                    .push(format!("Backup failed for {}: {}", file_path, e));
                continue;
            }
        }

        // Suppression
        match if path.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        } {
            Ok(_) => {
                if path.is_dir() {
                    result.folders_deleted += 1;
                } else {
                    result.files_deleted += 1;
                    result.space_freed += size;
                }
            }
            Err(e) => {
                result
                    .errors
                    .push(format!("Delete failed for {}: {}", file_path, e));
                result.success = false;
            }
        }
    }

    Ok(result)
}

/// Removes all empty folders from a library
pub fn cleanup_empty_folders(library_path: &Path) -> AppResult<CleanupResult> {
    let mut result = CleanupResult {
        success: true,
        files_deleted: 0,
        folders_deleted: 0,
        space_freed: 0,
        errors: Vec::new(),
        backup_path: None,
    };

    // Depth-first traversal (to delete subfolders before parents)
    let mut folders: Vec<PathBuf> = WalkDir::new(library_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir() && e.path() != library_path)
        .map(|e| e.path().to_path_buf())
        .collect();

    // Sort by decreasing depth
    folders.sort_by(|a, b| b.components().count().cmp(&a.components().count()));

    for folder in folders {
        // Check if the folder is empty
        if fs::read_dir(&folder)
            .map(|mut d| d.next().is_none())
            .unwrap_or(false)
        {
            match fs::remove_dir(&folder) {
                Ok(_) => {
                    result.folders_deleted += 1;
                }
                Err(e) => {
                    result
                        .errors
                        .push(format!("Failed to delete {}: {}", folder.display(), e));
                }
            }
        }
    }

    Ok(result)
}

/// Complete library cleanup: removes unwanted files and empty folders
///
/// This function removes:
/// - Promo/marketing images at root level
/// - README, LICENSE, and documentation files at root level
/// - Temporary files (Thumbs.db, .DS_Store, etc.)
/// - Empty folders
///
/// Files inside DAZ standard folders (data/, People/, Runtime/) are NOT touched.
pub fn cleanup_library_complete(library_path: &Path) -> AppResult<CleanupResult> {
    info!("Starting complete library cleanup: {:?}", library_path);

    let mut result = CleanupResult {
        success: true,
        files_deleted: 0,
        folders_deleted: 0,
        space_freed: 0,
        errors: Vec::new(),
        backup_path: None,
    };

    if !library_path.exists() {
        return Err(AppError::NotFound(library_path.to_path_buf()));
    }

    // Step 1: Remove unwanted files
    info!("Step 1: Scanning for unwanted files...");

    for entry in WalkDir::new(library_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let file_name_lower = file_name.to_lowercase();

        // Skip files inside DAZ standard folders (these are legitimate DAZ content)
        let path_str = path.to_string_lossy().to_string();
        let is_in_daz_folder = path_str.contains("\\data\\")
            || path_str.contains("/data/")
            || path_str.contains("\\People\\")
            || path_str.contains("/People/")
            || path_str.contains("\\Runtime\\")
            || path_str.contains("/Runtime/")
            || path_str.contains("\\Environments\\")
            || path_str.contains("/Environments/")
            || path_str.contains("\\Props\\")
            || path_str.contains("/Props/")
            || path_str.contains("\\Light Presets\\")
            || path_str.contains("/Light Presets/")
            || path_str.contains("\\Camera Presets\\")
            || path_str.contains("/Camera Presets/");

        if is_in_daz_folder {
            continue; // Don't touch DAZ content files
        }

        let should_delete =
            // Temporary files
            is_temp_file(&file_name_lower)
            // Documentation
            || file_name_lower.contains("readme")
            || file_name_lower.contains("license")
            || file_name_lower.starts_with("readthis")
            || file_name_lower.ends_with(".txt")
            || file_name_lower.ends_with(".pdf")
            || file_name_lower.ends_with(".html")
            || file_name_lower.ends_with(".htm")
            || file_name_lower.ends_with(".url")
            // Promo images (at root or in promo folders)
            || (file_name_lower.contains("promo") && (
                file_name_lower.ends_with(".png")
                || file_name_lower.ends_with(".jpg")
                || file_name_lower.ends_with(".jpeg")
            ))
            // Images at root level (likely promo)
            || (entry.depth() <= 2 && (
                file_name_lower.ends_with(".png")
                || file_name_lower.ends_with(".jpg")
                || file_name_lower.ends_with(".jpeg")
            ) && !file_name_lower.contains("icon"));

        if should_delete {
            let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);

            match fs::remove_file(path) {
                Ok(_) => {
                    info!("Deleted unwanted file: {:?}", file_name);
                    result.files_deleted += 1;
                    result.space_freed += size;
                }
                Err(e) => {
                    let error_msg = format!("Failed to delete {}: {}", file_name, e);
                    result.errors.push(error_msg);
                    result.success = false;
                }
            }
        }
    }

    info!(
        "Step 1 complete: {} files deleted, {} freed",
        result.files_deleted,
        format_size(result.space_freed)
    );

    // Step 2: Remove empty folders (multiple passes for nested empty folders)
    info!("Step 2: Removing empty folders...");

    let mut pass = 0;
    loop {
        pass += 1;
        let mut folders: Vec<PathBuf> = WalkDir::new(library_path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir() && e.path() != library_path)
            .map(|e| e.path().to_path_buf())
            .collect();

        // Sort by decreasing depth (delete children before parents)
        folders.sort_by(|a, b| b.components().count().cmp(&a.components().count()));

        let mut deleted_this_pass = 0;

        for folder in folders {
            // Check if empty
            if fs::read_dir(&folder)
                .map(|mut d| d.next().is_none())
                .unwrap_or(false)
            {
                match fs::remove_dir(&folder) {
                    Ok(_) => {
                        deleted_this_pass += 1;
                        result.folders_deleted += 1;
                    }
                    Err(e) => {
                        result.errors.push(format!(
                            "Failed to delete folder {}: {}",
                            folder.display(),
                            e
                        ));
                    }
                }
            }
        }

        info!("Pass {}: {} folders deleted", pass, deleted_this_pass);

        // Stop if no more deletions
        if deleted_this_pass == 0 || pass >= 10 {
            break;
        }
    }

    info!(
        "Complete cleanup finished: {} files, {} folders deleted, {} freed",
        result.files_deleted,
        result.folders_deleted,
        format_size(result.space_freed)
    );

    Ok(result)
}

// ============================================================================
// Helpers
// ============================================================================

/// Calcule un hash rapide (premiers + derniers bytes)
fn compute_fast_hash(path: &Path) -> AppResult<String> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let file = fs::File::open(path)?;
    let metadata = file.metadata()?;
    let size = metadata.len();

    let mut reader = BufReader::new(file);
    let mut buffer = vec![0u8; 8192.min(size as usize)];
    reader.read_exact(&mut buffer)?;

    let mut hasher = DefaultHasher::new();
    buffer.hash(&mut hasher);
    size.hash(&mut hasher);

    Ok(format!("{:x}", hasher.finish()))
}

/// Checks if a file is a temporary file
fn is_temp_file(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    TEMP_PATTERNS.iter().any(|pattern| lower.contains(pattern))
}

/// Normalizes a filename for comparison
fn normalize_filename(filename: &str) -> String {
    filename
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Formats a size in bytes
fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    if bytes == 0 {
        return "0 B".to_string();
    }
    let i = (bytes as f64).log(1024.0).floor() as usize;
    let size = bytes as f64 / 1024f64.powi(i as i32);
    format!("{:.1} {}", size, UNITS[i.min(UNITS.len() - 1)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_temp_file() {
        assert!(is_temp_file("Thumbs.db"));
        assert!(is_temp_file("~$document.docx"));
        assert!(is_temp_file(".DS_Store"));
        assert!(!is_temp_file("model.duf"));
    }

    #[test]
    fn test_normalize_filename() {
        assert_eq!(normalize_filename("My_Model-v2.duf"), "mymodelv2duf");
        assert_eq!(normalize_filename("TEST 123.png"), "test123png");
    }
}
