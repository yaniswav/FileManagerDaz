//! Batch normalization of a folder containing loose DAZ content
//!
//! Processes a "messy" folder containing:
//! - ZIP/RAR/7z archives to extract
//! - Folders with wrappers (Content, product name) to unwrap
//! - Already correct DAZ folders to merge
//! - Loose files (poses, promo images, etc.)

use crate::config::settings::AppSettings;
use crate::error::{AppError, AppResult};
use rayon::prelude::*;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tracing::{debug, info, warn};

use super::recursive::extract_archive_by_format;
use super::utils::{count_directory_contents, merge_directories, normalize_daz_structure};
use super::ArchiveFormat;

// ============================================================================
// Types
// ============================================================================

/// Batch normalization result
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizeBatchResult {
    /// Processed source path
    pub source_path: PathBuf,
    /// Destination path
    pub destination_path: PathBuf,
    /// Number of archives extracted
    pub archives_extracted: usize,
    /// Number of folders normalized
    pub folders_normalized: usize,
    /// Number of DAZ folders merged
    pub folders_merged: usize,
    /// Number of promo/skipped files
    pub files_skipped: usize,
    /// Total number of files in result
    pub total_files: usize,
    /// Total size
    pub total_size: u64,
    /// Encountered errors (non-fatal)
    pub errors: Vec<String>,
}

/// File extensions to ignore (promos, previews, etc.)
const PROMO_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "bmp", // Images
    "pdf", // Documents
    "txt", "md", "rtf", // Text (except LICENSE)
    "html", "htm", // Web
];

/// Special files to keep
const KEEP_FILES: &[&str] = &[
    "license",
    "licence",
    "readme",
    "read me",
    "eula",
    "manifest.dsx",
    "supplement.dsx",
];

/// DAZ extensions to keep even at root
const DAZ_EXTENSIONS: &[&str] = &[
    "duf", "dsf", "dsa", "dse", "daz", "duf.png", "dsf.png", "dsa.png", // Thumbnails
];

// ============================================================================
// Main functions
// ============================================================================

/// Normalizes and merges a "messy" folder to a DAZ library
pub fn normalize_and_merge_batch<F>(
    source_dir: &Path,
    destination: Option<&Path>,
    settings: &AppSettings,
    emit_step: F,
) -> AppResult<NormalizeBatchResult>
where
    F: Fn(&str, Option<&str>) + Send + Sync + Clone,
{
    info!("Starting batch normalization of {:?}", source_dir);

    if !source_dir.exists() || !source_dir.is_dir() {
        return Err(AppError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Source directory not found: {:?}", source_dir),
        )));
    }

    // Determine destination
    let dest_path = match destination {
        Some(d) => d.to_path_buf(),
        None => {
            settings.default_destination.clone().ok_or_else(|| {
                AppError::Config(
                    "No destination specified and no default library configured".to_string(),
                )
            })?
        }
    };

    if !dest_path.exists() {
        fs::create_dir_all(&dest_path)?;
    }

    emit_step(
        "Analyse du dossier source...",
        Some(&source_dir.to_string_lossy()),
    );

    let mut result = NormalizeBatchResult {
        source_path: source_dir.to_path_buf(),
        destination_path: dest_path.clone(),
        archives_extracted: 0,
        folders_normalized: 0,
        folders_merged: 0,
        files_skipped: 0,
        total_files: 0,
        total_size: 0,
        errors: Vec::new(),
    };

    // Step 1: Extract all archives
    emit_step("Extracting archives...", None);
    let archives = find_archives_at_root(source_dir)?;
    if !archives.is_empty() {
        info!("Found {} archives to extract", archives.len());
        result.archives_extracted =
            extract_archives_parallel(&archives, source_dir, &mut result.errors, settings)?;
        emit_step(
            &format!("{} archives extracted", result.archives_extracted),
            None,
        );
    }

    // Step 2: Process all folders
    emit_step("Normalizing folders...", None);
    let folders = get_folders_at_root(source_dir)?;

    for folder in &folders {
        let folder_name = folder
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        emit_step(&format!("Processing: {}", folder_name), None);

        match process_single_folder(folder, &dest_path) {
            Ok((normalized, merged)) => {
                if normalized {
                    result.folders_normalized += 1;
                }
                if merged {
                    result.folders_merged += 1;
                }
            }
            Err(e) => {
                result.errors.push(format!("{}: {}", folder_name, e));
                warn!("Error processing folder {:?}: {}", folder, e);
            }
        }
    }

    // Step 3: Process loose files (DAZ poses at root)
    emit_step("Processing files...", None);
    let (daz_files, promo_files) = categorize_files_at_root(source_dir)?;

    // Move DAZ files to an appropriate folder
    if !daz_files.is_empty() {
        move_loose_daz_files(&daz_files, &dest_path, source_dir)?;
        info!("Moved {} loose DAZ files", daz_files.len());
    }

    result.files_skipped = promo_files.len();
    if result.files_skipped > 0 {
        info!("Skipped {} promo/preview files", result.files_skipped);
    }

    // Step 4: Count the final result
    emit_step("Final counting...", None);
    let stats = count_directory_contents(&dest_path)?;
    result.total_files = stats.files;
    result.total_size = stats.size_bytes;

    emit_step(
        "Normalization complete",
        Some(&format!(
            "{} archives, {} folders normalized, {} merged, {} skipped",
            result.archives_extracted,
            result.folders_normalized,
            result.folders_merged,
            result.files_skipped
        )),
    );

    info!("Batch normalization complete: {:?}", result);
    Ok(result)
}

// ============================================================================
// Fonctions internes
// ============================================================================

/// Finds archives at the root of a folder (non-recursive)
fn find_archives_at_root(dir: &Path) -> AppResult<Vec<PathBuf>> {
    let mut archives = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && ArchiveFormat::from_extension(&entry.path()).is_some()
        {
            archives.push(entry.path());
        }
    }

    Ok(archives)
}

/// Gets folders at the root
fn get_folders_at_root(dir: &Path) -> AppResult<Vec<PathBuf>> {
    let mut folders = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            folders.push(entry.path());
        }
    }

    Ok(folders)
}

/// Extracts archives in parallel
fn extract_archives_parallel(
    archives: &[PathBuf],
    source_dir: &Path,
    errors: &mut Vec<String>,
    settings: &AppSettings,
) -> AppResult<usize> {
    let errors_mutex: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let success_count: Mutex<usize> = Mutex::new(0);

    archives.par_iter().for_each(|archive| {
        let archive_name = archive
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("archive");

        let format = match ArchiveFormat::from_extension(archive) {
            Some(f) => f,
            None => return,
        };

        // Create a folder for extraction
        let extract_name = archive
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("extracted");
        let extract_dir = source_dir.join(extract_name);

        match extract_archive_by_format(archive, &extract_dir, format, settings) {
            Ok(_) => {
                // Delete the archive after successful extraction
                if let Err(e) = fs::remove_file(archive) {
                    warn!("Could not delete archive {:?}: {}", archive, e);
                }
                if let Ok(mut count) = success_count.lock() {
                    *count += 1;
                }
            }
            Err(e) => {
                if let Ok(mut errs) = errors_mutex.lock() {
                    errs.push(format!("{}: {}", archive_name, e));
                }
            }
        }
    });

    // Get the errors
    if let Ok(errs) = errors_mutex.into_inner() {
        errors.extend(errs);
    }

    Ok(success_count.into_inner().unwrap_or(0))
}

/// Processes a single folder: normalizes and merges
fn process_single_folder(folder: &Path, destination: &Path) -> AppResult<(bool, bool)> {
    // First, normalize the internal structure
    let normalized = normalize_daz_structure(folder)?;

    // Check if it's a DAZ folder (contains data/, People/, Runtime/, etc.)
    if has_daz_content(folder) {
        // Merge content to destination
        merge_daz_content(folder, destination)?;
        return Ok((normalized, true));
    }

    // Check if it's a custom content folder (GCC, etc.)
    // These folders contain .duf files but not the standard structure
    if has_duf_files(folder) {
        // Keep the structure but copy to destination
        let folder_name = folder
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("content");

        // Determine where to place this content (People/Genesis 8 Female/ by default)
        let dest_folder = destination
            .join("People")
            .join("Genesis 8 Female")
            .join(folder_name);
        merge_directories(folder, &dest_folder)?;
        return Ok((normalized, true));
    }

    // Unrecognized folder, ignore
    debug!("Skipping unrecognized folder: {:?}", folder);
    Ok((normalized, false))
}

/// Checks if a folder contains standard DAZ content
fn has_daz_content(dir: &Path) -> bool {
    const DAZ_FOLDERS: &[&str] = &["data", "People", "Runtime", "Documentation", "Environments"];

    for folder in DAZ_FOLDERS {
        if dir.join(folder).exists() {
            return true;
        }
    }
    false
}

/// Checks if a folder contains .duf/.dsa files, up to `max_depth` levels deep.
fn has_duf_files(dir: &Path) -> bool {
    has_duf_files_recursive(dir, 2)
}

/// Recursively checks for .duf/.dsa files with bounded depth to prevent stack overflow.
fn has_duf_files_recursive(dir: &Path, remaining_depth: u32) -> bool {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext.eq_ignore_ascii_case("duf") || ext.eq_ignore_ascii_case("dsa") {
                        return true;
                    }
                }
            } else if path.is_dir()
                && remaining_depth > 0
                && has_duf_files_recursive(&path, remaining_depth - 1)
            {
                return true;
            }
        }
    }
    false
}

/// Merges DAZ content from a folder to the destination
fn merge_daz_content(source: &Path, destination: &Path) -> AppResult<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let entry_path = entry.path();
        let entry_name = entry.file_name();

        if entry_path.is_dir() {
            let dest_path = destination.join(&entry_name);
            merge_directories(&entry_path, &dest_path)?;
        } else {
            // Files at root (LICENSE, README, etc.)
            let name_lower = entry_name.to_string_lossy().to_lowercase();

            // Ignore Manifest.dsx, Supplement.dsx, etc. files
            if name_lower.contains("manifest") || name_lower.contains("supplement") {
                continue;
            }

            // Copy important files
            if KEEP_FILES.iter().any(|k| name_lower.contains(k)) {
                let dest_path = destination.join("Documentation").join(&entry_name);
                if let Some(parent) = dest_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                if let Err(e) = fs::copy(&entry_path, &dest_path) {
                    warn!("Failed to copy {:?} to Documentation: {}", entry_name, e);
                }
            }
        }
    }

    Ok(())
}

/// Categorizes files at root into DAZ vs promo
fn categorize_files_at_root(dir: &Path) -> AppResult<(Vec<PathBuf>, Vec<PathBuf>)> {
    let mut daz_files = Vec::new();
    let mut promo_files = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }

        let path = entry.path();
        let name_lower = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();

        // Check the extension
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext_lower = ext.to_lowercase();

            if DAZ_EXTENSIONS
                .iter()
                .any(|e| ext_lower == *e || name_lower.ends_with(e))
            {
                daz_files.push(path);
            } else if PROMO_EXTENSIONS.contains(&ext_lower.as_str()) {
                // Ignore unless it's an important file
                if !KEEP_FILES.iter().any(|k| name_lower.contains(k)) {
                    promo_files.push(path);
                }
            }
        }
    }

    Ok((daz_files, promo_files))
}

/// Moves loose DAZ files to an appropriate folder in the destination
fn move_loose_daz_files(
    files: &[PathBuf],
    destination: &Path,
    source_name: &Path,
) -> AppResult<()> {
    if files.is_empty() {
        return Ok(());
    }

    // Create a folder based on the source folder name
    let folder_name = source_name
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Imported");

    // Detect content type (poses, scripts, etc.)
    let first_file = &files[0];
    let ext = first_file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let dest_folder = match ext.to_lowercase().as_str() {
        "dsa" | "dse" => destination.join("Scripts").join(folder_name),
        "duf" => {
            // .duf files can be poses, presets, etc.
            // Put them in an "Imported" folder for now
            destination
                .join("People")
                .join("Genesis 8 Female")
                .join("Poses")
                .join(folder_name)
        }
        _ => destination.join("Imported").join(folder_name),
    };

    fs::create_dir_all(&dest_folder)?;

    for file in files {
        let file_name = file.file_name().unwrap_or_default();
        let dest_path = dest_folder.join(file_name);

        // Also copy the thumbnail if present
        let thumbnail = file.with_extension(format!(
            "{}.png",
            file.extension().and_then(|e| e.to_str()).unwrap_or("")
        ));

        if fs::rename(file, &dest_path).is_err() {
            fs::copy(file, &dest_path)?;
            if let Err(e) = fs::remove_file(file) {
                warn!("Failed to remove source file after copy {:?}: {}", file, e);
            }
        }

        if thumbnail.exists() {
            let thumb_dest = dest_folder.join(thumbnail.file_name().unwrap_or_default());
            if let Err(e) = fs::rename(&thumbnail, &thumb_dest) {
                warn!("Failed to move thumbnail {:?}: {}", thumbnail, e);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    //! Tests for the destructive batch-normalization helpers.
    //!
    //! The primary risk flagged by the audit is `extract_archives_parallel`,
    //! which deletes source archives after extraction. We pin its contract:
    //! a successful extraction removes the source, a failed one keeps it
    //! and records an error.

    use super::*;
    use std::io::Write;
    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    /// Writes a small, valid zip that drops a single text file when extracted.
    fn write_test_zip(path: &Path) {
        let file = fs::File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("payload.txt", opts).unwrap();
        zip.write_all(b"daz").unwrap();
        zip.finish().unwrap();
    }

    fn touch(path: &Path) {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(path, b"").unwrap();
    }

    #[test]
    fn find_archives_at_root_filters_non_archive_files_and_dirs() {
        let tmp = TempDir::new().unwrap();
        touch(&tmp.path().join("a.zip"));
        touch(&tmp.path().join("readme.txt"));
        touch(&tmp.path().join("promo.jpg"));
        fs::create_dir_all(tmp.path().join("subdir")).unwrap();
        touch(&tmp.path().join("subdir/inside.zip")); // must NOT be picked (non-recursive)

        let archives = find_archives_at_root(tmp.path()).unwrap();
        assert_eq!(archives.len(), 1);
        assert_eq!(archives[0].file_name().unwrap(), "a.zip");
    }

    #[test]
    fn has_daz_content_detects_standard_folders() {
        let tmp = TempDir::new().unwrap();
        assert!(!has_daz_content(tmp.path()), "empty dir is not DAZ content");

        fs::create_dir_all(tmp.path().join("data")).unwrap();
        assert!(has_daz_content(tmp.path()), "presence of `data/` is enough");
    }

    #[test]
    fn has_duf_files_respects_depth_limit() {
        let tmp = TempDir::new().unwrap();
        // depth 0 — direct file
        touch(&tmp.path().join("root.duf"));
        assert!(has_duf_files(tmp.path()));

        // Fresh tree at depth > 2 should NOT trip the limit (max_depth = 2).
        let deep = TempDir::new().unwrap();
        let very_deep = deep.path().join("a/b/c/d");
        fs::create_dir_all(&very_deep).unwrap();
        touch(&very_deep.join("hidden.duf")); // depth 4 from root
        assert!(
            !has_duf_files(deep.path()),
            "depth 4 is beyond the recursion budget"
        );
    }

    #[test]
    fn categorize_files_separates_daz_keep_and_promo() {
        let tmp = TempDir::new().unwrap();
        touch(&tmp.path().join("scene.duf"));
        touch(&tmp.path().join("model.dsf"));
        touch(&tmp.path().join("promo.jpg")); // promo, dropped
        touch(&tmp.path().join("README.txt")); // promo extension but KEEP_FILES → dropped from promo
        touch(&tmp.path().join("LICENSE.txt")); // same logic — kept out of promo
        fs::create_dir_all(tmp.path().join("sub")).unwrap();

        let (daz, promo) = categorize_files_at_root(tmp.path()).unwrap();
        assert_eq!(daz.len(), 2, "duf + dsf are DAZ files");
        // README and LICENSE are excluded from promo because they hit KEEP_FILES;
        // only `promo.jpg` should remain as a promo.
        assert_eq!(promo.len(), 1);
        assert_eq!(promo[0].file_name().unwrap(), "promo.jpg");
    }

    #[test]
    fn extract_archives_parallel_removes_source_after_success() {
        let tmp = TempDir::new().unwrap();
        let archive = tmp.path().join("ok.zip");
        write_test_zip(&archive);

        let mut errors = Vec::new();
        let settings = AppSettings::default();
        let count = extract_archives_parallel(
            std::slice::from_ref(&archive),
            tmp.path(),
            &mut errors,
            &settings,
        )
        .unwrap();

        assert_eq!(count, 1, "exactly one archive should be reported extracted");
        assert!(errors.is_empty(), "no error expected on a valid zip");
        assert!(
            !archive.exists(),
            "source archive must be removed after a successful extraction"
        );
        assert!(
            tmp.path().join("ok/payload.txt").exists(),
            "payload should land in `<archive_stem>/`"
        );
    }

    #[test]
    fn extract_archives_parallel_keeps_source_on_failure() {
        let tmp = TempDir::new().unwrap();
        let archive = tmp.path().join("bad.zip");
        // Garbage with a .zip extension — extract_archive_by_format must fail.
        fs::write(&archive, b"this is not a zip").unwrap();

        let mut errors = Vec::new();
        let settings = AppSettings::default();
        let count = extract_archives_parallel(
            std::slice::from_ref(&archive),
            tmp.path(),
            &mut errors,
            &settings,
        )
        .unwrap();

        assert_eq!(count, 0, "nothing should be reported extracted");
        assert_eq!(errors.len(), 1, "failure must be recorded in errors");
        assert!(errors[0].starts_with("bad.zip:"), "error must be tagged with the archive name");
        assert!(
            archive.exists(),
            "source archive MUST be preserved when extraction fails — this is the audit's I5 invariant"
        );
    }
}
