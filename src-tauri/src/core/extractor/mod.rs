//! # Archive Extraction Engine
//!
//! This module handles extraction of DAZ 3D asset archives and processing
//! of source folders. It supports multiple archive formats and recursive
//! extraction of nested archives.
//!
//! ## Supported Formats
//!
//! | Format | Library | Notes |
//! |--------|---------|-------|
//! | ZIP    | `zip` crate | Native Rust, full support |
//! | 7z     | `sevenz-rust` crate | Native Rust, full support |
//! | RAR    | External `unrar.exe` | Requires binary in PATH or bundled |
//!
//! ## Module Structure
//!
//! - `mod.rs`: Public types and [`process_source`] entry point
//! - `zip.rs`: Native ZIP extraction implementation
//! - `seven_zip.rs`: Native 7z extraction implementation
//! - `rar.rs`: RAR extraction via external `unrar.exe` binary
//! - `recursive.rs`: Recursive extraction of nested archives (parallel via rayon)
//! - `normalize.rs`: Batch normalization of extracted DAZ folder structures
//! - `utils.rs`: Shared utilities (file counting, directory merging)
//! - `timing.rs`: Extraction timing instrumentation
//!
//! ## Usage
//!
//! For simple extraction:
//! ```ignore
//! use crate::core::extractor::process_source;
//! let result = process_source(Path::new("archive.zip"))?;
//! ```
//!
//! For recursive extraction with events (recommended for UI):
//! ```ignore
//! use crate::core::extractor::process_source_recursive_with_events;
//! let result = process_source_recursive_with_events(path, app_handle)?;
//! ```

mod anchors;
mod batch;
pub mod checkpoint;
mod move_log;
pub mod multipart;
mod normalize;
mod rar;
mod recursive;
pub mod resilience;
mod seven_zip;
pub mod timing;
mod utils;
mod zip;

// Re-exports publics
pub use anchors::{
    detect_anchors, is_daz_file, is_system_junk, AnchorDetectionResult, AnchorPoint,
};
pub use batch::{
    process_batch_with_defaults, BatchItemFailure, BatchItemResult, BatchOperationResult,
    BatchProgress, BatchStats, RobustBatchProcessor,
};
pub use normalize::{normalize_and_merge_batch, NormalizeBatchResult};
#[allow(unused_imports)]
pub use recursive::NestedArchiveInfo;
pub use recursive::{
    extract_archive_by_format, process_source_recursive, process_source_recursive_with_events,
    RecursiveExtractResult,
};
#[allow(unused_imports)]
pub use utils::format_size;

use crate::config::settings::AppSettings;
use crate::core::analyzer::{analyze_content, AnalysisSummary};
use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{error, info};

use utils::{count_directory_contents, get_root_entries, merge_directories, ContentStats};

// =============================================================================
// PUBLIC TYPES
// =============================================================================

/// Type of source being processed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    /// Archive file (ZIP, RAR, 7z)
    Archive,
    /// Directory containing DAZ content
    Directory,
}

/// Supported archive formats.
///
/// Each format has different extraction characteristics:
/// - ZIP/7z: Native Rust extraction (fast, no external dependencies)
/// - RAR: Requires external `unrar.exe` (Windows only, user must provide)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ArchiveFormat {
    /// Standard ZIP format (most common for DAZ assets)
    Zip,
    /// RAR format (requires external unrar.exe)
    Rar,
    /// 7-Zip format (good compression, native support)
    SevenZip,
}

impl ArchiveFormat {
    /// Detects archive format from file extension.
    ///
    /// Returns `None` if the extension is not recognized as an archive format.
    pub fn from_extension(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_str()?.to_lowercase();

        // Check for .partN.rar pattern first
        if name.contains(".part") && name.ends_with(".rar") {
            return Some(ArchiveFormat::Rar);
        }

        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_lowercase())
            .and_then(|ext| match ext.as_str() {
                "zip" => Some(ArchiveFormat::Zip),
                "rar" => Some(ArchiveFormat::Rar),
                "7z" => Some(ArchiveFormat::SevenZip),
                _ => None,
            })
    }

    /// Returns the file extension for this format.
    pub fn extension(&self) -> &'static str {
        match self {
            ArchiveFormat::Zip => "zip",
            ArchiveFormat::Rar => "rar",
            ArchiveFormat::SevenZip => "7z",
        }
    }
}

/// Result of processing a source (archive or directory).
///
/// Contains metadata about the extracted/processed content, including
/// file counts, sizes, and DAZ content analysis.
#[derive(Debug, Clone, Serialize)]
pub struct ExtractResult {
    /// Original source path (archive or directory).
    pub source_path: PathBuf,

    /// Destination directory where content was extracted/copied.
    pub destination: PathBuf,

    /// Number of files in the extracted content.
    pub file_count: usize,

    /// Number of directories in the extracted content.
    pub dir_count: usize,

    /// Total size in bytes of extracted content.
    pub total_size: u64,

    /// Whether source was an archive or directory.
    pub source_type: SourceType,

    /// Archive format (only if source was an archive).
    pub archive_format: Option<ArchiveFormat>,

    /// Top-level entries in the extracted content.
    pub root_entries: Vec<String>,

    /// DAZ content analysis (manifest info, content types).
    pub analysis: Option<AnalysisSummary>,
}

// =============================================================================
// MAIN ENTRY POINT
// =============================================================================

/// Processes a source (archive or directory) and returns extraction result.
///
/// This is the simple, synchronous entry point. For UI integration with
/// progress events, use [`process_source_recursive_with_events`] instead.
///
/// # Arguments
///
/// * `path` - Path to the archive file or directory to process
///
/// # Behavior
///
/// - **Archives**: Extracts to a temporary directory, analyzes content
/// - **Directories**: Copies to temp directory for safe nested archive extraction
///
/// # Errors
///
/// Returns `AppError::NotFound` if path doesn't exist, or extraction-specific
/// errors for archive processing failures.
pub fn process_source(path: &Path, settings: &AppSettings) -> AppResult<ExtractResult> {
    info!("Processing source: {:?}", path);

    if !path.exists() {
        error!("Source not found: {:?}", path);
        return Err(AppError::NotFound(path.to_path_buf()));
    }

    // If this is a secondary part of a multi-part archive,
    // automatically resolve to the first part
    let effective_path = if path.is_file() && multipart::is_secondary_part(path) {
        if let Some(mp_info) = multipart::detect_multipart(path) {
            info!(
                "Secondary part detected, resolving to first part: {:?}",
                mp_info.first_part
            );
            mp_info.first_part
        } else {
            path.to_path_buf()
        }
    } else {
        path.to_path_buf()
    };

    let result = if effective_path.is_dir() {
        process_directory(&effective_path, settings)?
    } else if effective_path.is_file() {
        process_archive_file(&effective_path, settings)?
    } else {
        return Err(AppError::InvalidPath(format!(
            "Path is neither file nor directory: {}",
            effective_path.display()
        )));
    };

    info!(
        "Source processed: {} files, {} dirs, {} bytes",
        result.file_count, result.dir_count, result.total_size
    );

    Ok(result)
}

// =============================================================================
// DIRECTORY PROCESSING
// =============================================================================

/// Processes a directory as import source.
///
/// Copies the directory content to a temporary location to allow safe
/// recursive extraction of nested archives without modifying the original.
fn process_directory(source_dir: &Path, settings: &AppSettings) -> AppResult<ExtractResult> {
    info!("Processing directory: {:?}", source_dir);

    // Generate temp destination path
    let dir_name = source_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("folder");

    let temp_destination = settings.temp_dir.join(format!("{}_import", dir_name));

    // Clean up any existing temp folder
    if temp_destination.exists() {
        fs::remove_dir_all(&temp_destination)?;
    }
    fs::create_dir_all(&temp_destination)?;

    info!(
        "Copying directory to temp: {:?} -> {:?}",
        source_dir, temp_destination
    );

    // Copy source content to temp location
    merge_directories(source_dir, &temp_destination)?;

    // Analyze copied content
    let content_stats = count_directory_contents(&temp_destination)?;
    let root_entries = get_root_entries(&temp_destination)?;
    let analysis = analyze_content(&temp_destination).ok();

    Ok(ExtractResult {
        source_path: source_dir.to_path_buf(),
        destination: temp_destination,
        file_count: content_stats.files,
        dir_count: content_stats.dirs,
        total_size: content_stats.size_bytes,
        source_type: SourceType::Directory,
        archive_format: None,
        root_entries,
        analysis,
    })
}

// =============================================================================
// ARCHIVE EXTRACTION
// =============================================================================

/// Extracts an archive file to a temporary directory.
fn process_archive_file(archive_path: &Path, settings: &AppSettings) -> AppResult<ExtractResult> {
    let format = ArchiveFormat::from_extension(archive_path).ok_or_else(|| {
        AppError::UnsupportedFormat(format!(
            "Unknown archive extension: {}",
            archive_path.display()
        ))
    })?;

    // Generate destination directory name from archive name
    let archive_name = archive_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("extracted");

    let extraction_dir = settings.get_extraction_dir(archive_name)?;

    info!("Extracting {:?} to {:?}", archive_path, extraction_dir);

    // Check for multi-part archive
    let mp_info = multipart::detect_multipart(archive_path);

    // Dispatch to format-specific extractor
    let content_stats: ContentStats = match (&mp_info, format) {
        // ZIP split: reassemble then extract
        (Some(info), ArchiveFormat::Zip)
            if info.format == multipart::MultiPartFormat::ZipSplit =>
        {
            info!(
                "Detected ZIP split archive with {} parts",
                info.all_parts.len()
            );
            let reassembled = multipart::reassemble_zip_split(info, &extraction_dir)?;
            let stats = zip::extract_zip(&reassembled, &extraction_dir)?;
            // Clean up reassembled file
            let _ = fs::remove_file(&reassembled);
            stats
        }
        // RAR multi-part: unrar handles it natively from the first part
        (Some(info), ArchiveFormat::Rar) => {
            info!(
                "Detected RAR multi-part archive with {} parts",
                info.all_parts.len()
            );
            rar::extract_rar(&info.first_part, &extraction_dir, settings)?
        }
        // Regular single archive
        (_, ArchiveFormat::Zip) => zip::extract_zip(archive_path, &extraction_dir)?,
        (_, ArchiveFormat::SevenZip) => seven_zip::extract_7z(archive_path, &extraction_dir)?,
        (_, ArchiveFormat::Rar) => rar::extract_rar(archive_path, &extraction_dir, settings)?,
    };

    let root_entries = get_root_entries(&extraction_dir)?;
    let analysis = analyze_content(&extraction_dir).ok();

    Ok(ExtractResult {
        source_path: archive_path.to_path_buf(),
        destination: extraction_dir,
        file_count: content_stats.files,
        dir_count: content_stats.dirs,
        total_size: content_stats.size_bytes,
        source_type: SourceType::Archive,
        archive_format: Some(format),
        root_entries,
        analysis,
    })
}

// =============================================================================
// PUBLIC UTILITIES
// =============================================================================

/// Checks if an archive format is currently supported.
///
/// ZIP and 7z are always supported. RAR requires `unrar.exe` to be available.
pub fn is_format_supported(format: ArchiveFormat, settings: &AppSettings) -> bool {
    match format {
        ArchiveFormat::Zip => true,
        ArchiveFormat::SevenZip => true,
        ArchiveFormat::Rar => settings.can_extract_rar(),
    }
}

/// Returns a list of all currently supported archive formats.
///
/// RAR is included only if `unrar.exe` is available.
pub fn get_supported_formats(settings: &AppSettings) -> Vec<ArchiveFormat> {
    let mut formats = vec![ArchiveFormat::Zip, ArchiveFormat::SevenZip];

    if settings.can_extract_rar() {
        formats.push(ArchiveFormat::Rar);
    }

    formats
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_directory(dir: &Path) -> PathBuf {
        let content_dir = dir.join("test_content");
        fs::create_dir_all(content_dir.join("data")).unwrap();
        fs::create_dir_all(content_dir.join("Runtime")).unwrap();

        fs::write(content_dir.join("data/info.txt"), "Test content").unwrap();
        fs::write(content_dir.join("Runtime/file.duf"), "{}").unwrap();

        content_dir
    }

    #[test]
    fn test_archive_format_detection() {
        assert_eq!(
            ArchiveFormat::from_extension(Path::new("test.zip")),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(
            ArchiveFormat::from_extension(Path::new("test.ZIP")),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(
            ArchiveFormat::from_extension(Path::new("test.rar")),
            Some(ArchiveFormat::Rar)
        );
        assert_eq!(
            ArchiveFormat::from_extension(Path::new("test.7z")),
            Some(ArchiveFormat::SevenZip)
        );
        assert_eq!(ArchiveFormat::from_extension(Path::new("test.txt")), None);
    }

    #[test]
    fn test_process_directory() {
        let temp_dir = TempDir::new().unwrap();
        let content_dir = create_test_directory(temp_dir.path());

        let settings = AppSettings::default();
        let result = process_source(&content_dir, &settings).unwrap();

        assert_eq!(result.source_type, SourceType::Directory);
        assert_eq!(result.archive_format, None);
        assert_eq!(result.file_count, 2);
        assert!(result.root_entries.contains(&"data".to_string()));
        assert!(result.root_entries.contains(&"Runtime".to_string()));
    }

    #[test]
    fn test_process_nonexistent_source() {
        let settings = AppSettings::default();
        let result = process_source(Path::new("/nonexistent/path"), &settings);
        assert!(result.is_err());
    }

    #[test]
    fn test_archive_format_extension() {
        assert_eq!(ArchiveFormat::Zip.extension(), "zip");
        assert_eq!(ArchiveFormat::Rar.extension(), "rar");
        assert_eq!(ArchiveFormat::SevenZip.extension(), "7z");
    }
}
