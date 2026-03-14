//! Recursive extraction of nested archives
//!
//! Handles extraction of archives containing other archives,
//! with support for step notifications via callback.

use crate::config::SETTINGS;
use crate::core::analyzer::{analyze_content, AnalysisSummary};
use crate::error::{AppError, AppResult};
use rayon::prelude::*;
use serde::Serialize;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tracing::{debug, info, warn};
use chrono::Utc;

use super::timing::ExtractionTimingSession;
use super::move_log::{MoveLogCounts, MoveLogEntry, MoveLogger};
use super::utils::{
    count_directory_contents, find_archives_in_dir, format_size, merge_directories, ContentStats,
};
use super::{process_source, ArchiveFormat};
use walkdir::WalkDir;

#[cfg(test)]
use once_cell::sync::Lazy;

// ============================================================================
// Types
// ============================================================================

/// Information about an extracted nested archive
#[derive(Debug, Clone, Serialize)]
pub struct NestedArchiveInfo {
    pub name: String,
    pub format: ArchiveFormat,
    pub depth: usize,
    pub file_count: usize,
    pub total_size: u64,
}

/// Result of recursive extraction
#[derive(Debug, Clone, Serialize)]
pub struct RecursiveExtractResult {
    /// Original source path
    pub source_path: PathBuf,
    /// Final destination directory
    pub destination: PathBuf,
    /// Total number of files (all levels)
    pub total_files: usize,
    /// Total size
    pub total_size: u64,
    /// Main archive format (if applicable)
    pub archive_format: Option<ArchiveFormat>,
    /// Nested archives extraction info
    pub nested_archives: Vec<NestedArchiveInfo>,
    /// DAZ content analysis of final content
    pub analysis: Option<AnalysisSummary>,
    /// Profondeur maximale atteinte
    pub max_depth_reached: usize,
    /// Indicates if content was moved to library
    pub moved_to_library: bool,
    /// Paths of original source archives (for trash after import)
    /// Contains the source path if it's an archive, or archives found in a source folder
    #[serde(default)]
    pub source_archive_paths: Vec<PathBuf>,
}

// ============================================================================
// Recursive extraction
// ============================================================================

/// Processes a source with recursive extraction of nested archives
///
/// # Arguments
/// * `path` - Path to the archive or source folder
/// * `max_depth` - Maximum recursion depth (default: 5)
pub fn process_source_recursive(
    path: &Path,
    max_depth: usize,
) -> AppResult<RecursiveExtractResult> {
    info!(
        "Processing source recursively: {:?} (max_depth: {})",
        path, max_depth
    );

    // Collect source archives for trash after import
    let source_archive_paths = collect_source_archives(path);

    // First extraction (in temp folder)
    let initial_result = process_source(path)?;

    let mut nested_archives = Vec::new();
    let mut max_depth_reached = 0;

    // Find and extract nested archives
    let temp_destination = extract_nested_archives(
        &initial_result.destination,
        1,
        max_depth,
        &mut nested_archives,
        &mut max_depth_reached,
    )?;

    // Re-analyze final content
    let analysis = analyze_content(&temp_destination).ok();

    // Count content BEFORE move (in temp folder)
    let stats = count_directory_contents(&temp_destination)?;
    info!(
        "Content stats before move: {} files, {} bytes",
        stats.files, stats.size_bytes
    );

    // Move to default library if configured
    let (final_destination, moved_to_library) = move_to_default_library(&temp_destination, path)?;

    Ok(RecursiveExtractResult {
        source_path: path.to_path_buf(),
        destination: final_destination,
        total_files: stats.files,
        total_size: stats.size_bytes,
        archive_format: initial_result.archive_format,
        nested_archives,
        analysis,
        max_depth_reached,
        moved_to_library,
        source_archive_paths,
    })
}

/// Processes a source with recursive extraction AND step notifications
///
/// Identical to `process_source_recursive` but calls `emit_step` at each important step.
/// Also records timings if `dev_log_extraction_timings` is enabled.
pub fn process_source_recursive_with_events<F>(
    path: &Path,
    max_depth: usize,
    emit_step: F,
) -> AppResult<RecursiveExtractResult>
where
    F: Fn(&str, Option<&str>) + Clone + Send + Sync,
{
    info!(
        "Processing source recursively with events: {:?} (max_depth: {})",
        path, max_depth
    );

    // Create the timing session
    let mut timing = ExtractionTimingSession::new(&path.to_string_lossy());

    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");

    // Collect source archives for trash after import
    timing.start_step("collect_source_archives");
    let source_archive_paths = collect_source_archives(path);
    timing.end_step();
    info!(
        "Source archives to trash after import: {:?}",
        source_archive_paths
    );

    // Step 1: Preparation
    emit_step("Preparing...", Some(file_name));

    // Determine the format
    let format = ArchiveFormat::from_extension(path);
    let format_name = format
        .map(|f| f.extension().to_uppercase())
        .unwrap_or_else(|| "folder".to_string());

    // Step 2: Main extraction
    emit_step(&format!("Extracting {}...", format_name), None);
    timing.start_step("initial_extraction");
    let initial_result = match process_source(path) {
        Ok(r) => {
            timing.end_step();
            r
        }
        Err(e) => {
            timing.finish_error(&e.to_string());
            return Err(e);
        }
    };

    // Step 3: Initial extraction result
    emit_step(
        "Content extracted",
        Some(&format!(
            "{} files, {} folders",
            initial_result.file_count, initial_result.dir_count
        )),
    );

    let mut nested_archives = Vec::new();
    let mut max_depth_reached = 0;

    // Step 4: Search for nested archives
    emit_step("Searching for nested archives...", None);
    timing.start_step("nested_extraction");

    // Find and extract nested archives with notifications
    let temp_destination = match extract_nested_archives_with_events(
        &initial_result.destination,
        1,
        max_depth,
        &mut nested_archives,
        &mut max_depth_reached,
        emit_step.clone(),
    ) {
        Ok(d) => {
            timing.end_step();
            d
        }
        Err(e) => {
            timing.finish_error(&e.to_string());
            return Err(e);
        }
    };

    // Step 5: Analyze content
    emit_step("Analyzing content...", None);
    timing.start_step("analyze_content");
    let analysis = analyze_content(&temp_destination).ok();
    timing.end_step();

    if let Some(ref a) = analysis {
        emit_step(
            "Analysis complete",
            Some(&format!(
                "Type: {:?}, {} DAZ files, {} textures",
                a.content_type, a.daz_file_count, a.texture_count
            )),
        );
    }

    // Step 5.5: Normalize DAZ structure (remove wrappers like "Content")
    emit_step("Normalizing structure...", None);
    if let Ok(normalized) = super::utils::normalize_daz_structure(&temp_destination) {
        if normalized {
            info!("Normalized DAZ structure in temp folder");
        }
    }

    // Step 6: Counting
    emit_step("Counting files...", None);
    timing.start_step("count_directory_contents");
    let stats = count_directory_contents(&temp_destination)?;
    timing.end_step();
    info!(
        "Content stats before move: {} files, {} bytes",
        stats.files, stats.size_bytes
    );

    // Step 7: Move to library
    emit_step("Moving to library...", None);
    timing.start_step("move_to_library");
    let (final_destination, moved_to_library) = move_to_default_library(&temp_destination, path)?;
    timing.end_step();

    if moved_to_library {
        emit_step(
            "Moved to library",
            Some(&final_destination.to_string_lossy()),
        );
    }

    // Final step
    emit_step(
        "Complete",
        Some(&format!(
            "{} files, {}",
            stats.files,
            format_size(stats.size_bytes)
        )),
    );

    // Finalize timing
    timing.set_stats(stats.files, 0, stats.size_bytes);
    timing.finish_success();

    Ok(RecursiveExtractResult {
        source_path: path.to_path_buf(),
        destination: final_destination,
        total_files: stats.files,
        total_size: stats.size_bytes,
        archive_format: initial_result.archive_format,
        nested_archives,
        analysis,
        max_depth_reached,
        moved_to_library,
        source_archive_paths,
    })
}

// ============================================================================
// Fonctions internes
// ============================================================================

/// Collects source archive paths for trash after import
/// - If path is an archive: returns [path]
/// - If path is a folder: returns all archives found in this folder (non-recursive)
fn collect_source_archives(path: &Path) -> Vec<PathBuf> {
    if path.is_file() {
        // It's a direct archive
        if ArchiveFormat::from_extension(path).is_some() {
            // If multi-part, collect all parts for trash
            if let Some(mp_info) = super::multipart::detect_multipart(path) {
                return mp_info.all_parts;
            }
            return vec![path.to_path_buf()];
        }
    } else if path.is_dir() {
        // It's a folder, scan for archives at root
        if let Ok(archives) = find_archives_in_dir(path) {
            return archives;
        }
    }
    Vec::new()
}

// ============================================================================
// Optimized copy with large buffers
// ============================================================================

/// Buffer size for cross-volume copies (16 MB)
#[allow(dead_code)]
const COPY_BUFFER_SIZE: usize = 16 * 1024 * 1024;

/// Copies a file with large buffers for better performance
///
/// More efficient than fs::copy() for large files as it uses
/// 16 MB buffers instead of ~8 KB.
#[allow(dead_code)]
fn copy_with_large_buffer(src: &Path, dst: &Path) -> AppResult<u64> {
    let src_file = File::open(src)?;
    let dst_file = File::create(dst)?;

    let file_size = src_file.metadata()?.len();

    // For small files, use standard fs::copy
    if file_size < COPY_BUFFER_SIZE as u64 {
        drop(src_file);
        drop(dst_file);
        return Ok(fs::copy(src, dst)?);
    }

    // For large files, use large buffers
    let mut reader = BufReader::with_capacity(COPY_BUFFER_SIZE, src_file);
    let mut writer = BufWriter::with_capacity(COPY_BUFFER_SIZE, dst_file);

    let bytes = std::io::copy(&mut reader, &mut writer)?;
    writer.flush()?;

    Ok(bytes)
}

/// Moves extracted content to the default DAZ library using intelligent anchor detection
fn move_to_default_library(temp_dir: &Path, source_path: &Path) -> AppResult<(PathBuf, bool)> {
    let settings = SETTINGS
        .read()
        .map_err(|e| AppError::Config(format!("Cannot read settings: {}", e)))?;

    // Check if a default destination is configured
    let default_dest = match &settings.default_destination {
        Some(dest) if dest.exists() => dest.clone(),
        _ => {
            info!("No default library configured, keeping content in temp dir");
            return Ok((temp_dir.to_path_buf(), false));
        }
    };

    drop(settings); // Release lock before long operations

    let session_id = build_session_id(source_path);
    let mut move_logger = MoveLogger::new(session_id);
    let mut move_counts = MoveLogCounts::default();
    let mut moved_roots: Vec<PathBuf> = Vec::new();

    info!("Detecting DAZ anchors in extracted content: {:?}", temp_dir);

    // Use anchor detection to find valid DAZ content
    let anchor_result = super::anchors::detect_anchors(temp_dir)?;

    info!(
        "Anchor detection complete: {} anchor points found, loose files: {}",
        anchor_result.anchor_points.len(),
        anchor_result.has_loose_files
    );

    log_move_entry(
        &mut move_logger,
        {
            let mut entry = MoveLogEntry::new("start");
            entry.source_path = Some(source_path.to_string_lossy().to_string());
            entry.temp_dir = Some(temp_dir.to_string_lossy().to_string());
            entry.dest_path = Some(default_dest.to_string_lossy().to_string());
            entry.anchor_count = Some(anchor_result.anchor_points.len());
            entry.has_loose_files = Some(anchor_result.has_loose_files);
            entry
        },
    );

    let mut moved_count = 0;

    // Process each anchor point
    for anchor in &anchor_result.anchor_points {
        info!(
            "Processing anchor at depth {}: {:?} (anchors: {:?})",
            anchor.depth, anchor.path, anchor.anchors
        );

        // Choose the anchor folder to use when a DAZ file is sitting directly at the anchor root
        let preferred_anchor = select_primary_anchor(&anchor.anchors);
        log_move_entry(
            &mut move_logger,
            {
                let mut entry = MoveLogEntry::new("anchor");
                entry.anchor_root = Some(anchor.path.to_string_lossy().to_string());
                entry.anchors = anchor.anchors.clone();
                entry.preferred_anchor = Some(preferred_anchor.clone());
                entry
            },
        );

        // Copy content from anchor point to library
        // We copy the CONTENTS of the anchor point, not the anchor point itself
        for entry in fs::read_dir(&anchor.path)? {
            let entry = entry?;
            let entry_path = entry.path();
            let entry_name = entry.file_name();
            let name_str = entry_name.to_string_lossy();

            // Skip system junk
            if super::anchors::is_system_junk(&name_str) {
                debug!("Skipping system junk: {:?}", entry_name);
                move_counts.skipped_entries += 1;
                log_move_entry(
                    &mut move_logger,
                    {
                        let mut entry = MoveLogEntry::new("skip");
                        entry.source_path = Some(entry_path.to_string_lossy().to_string());
                        entry.anchor_root = Some(anchor.path.to_string_lossy().to_string());
                        entry.reason = Some("system_junk".to_string());
                        entry.status = Some("skipped".to_string());
                        entry
                    },
                );
                continue;
            }

            // Skip non-anchor folders and non-DAZ files at anchor root
            if entry_path.is_dir() {
                // Only merge if this is an anchor folder
                if !anchor.anchors.contains(&name_str.to_string()) {
                    debug!("Skipping non-anchor folder: {:?}", entry_name);
                    move_counts.skipped_entries += 1;
                    log_move_entry(
                        &mut move_logger,
                        {
                            let mut entry = MoveLogEntry::new("skip");
                            entry.source_path = Some(entry_path.to_string_lossy().to_string());
                            entry.anchor_root = Some(anchor.path.to_string_lossy().to_string());
                            entry.reason = Some("non_anchor_dir".to_string());
                            entry.status = Some("skipped".to_string());
                            entry
                        },
                    );
                    continue;
                }
            } else {
                // For files at anchor root, only copy if it's a DAZ file
                if !super::anchors::is_daz_file(&entry_path) {
                    debug!("Skipping non-DAZ file: {:?}", entry_name);
                    move_counts.skipped_files += 1;
                    log_move_entry(
                        &mut move_logger,
                        {
                            let mut entry = MoveLogEntry::new("skip");
                            entry.source_path = Some(entry_path.to_string_lossy().to_string());
                            entry.anchor_root = Some(anchor.path.to_string_lossy().to_string());
                            entry.reason = Some("non_daz_file".to_string());
                            entry.status = Some("skipped".to_string());
                            entry
                        },
                    );
                    continue;
                }
            }

            let dest_path = if entry_path.is_dir() {
                default_dest.join(&entry_name)
            } else {
                // Place DAZ files found at the anchor root under the preferred anchor folder
                default_dest.join(&preferred_anchor).join(&entry_name)
            };

            if entry_path.is_dir() {
                info!("Merging anchor folder: {:?} -> library", entry_name);
                if let Err(e) = merge_directories(&entry_path, &dest_path) {
                    move_counts.errors += 1;
                    log_move_entry(
                        &mut move_logger,
                        {
                            let mut entry = MoveLogEntry::new("error");
                            entry.source_path = Some(entry_path.to_string_lossy().to_string());
                            entry.dest_path = Some(dest_path.to_string_lossy().to_string());
                            entry.reason = Some("merge_dir_failed".to_string());
                            entry.message = Some(e.to_string());
                            entry
                        },
                    );
                    return Err(e);
                }
                moved_count += 1;
                moved_roots.push(entry_path.clone());
                move_counts.merged_dirs += 1;
                log_move_entry(
                    &mut move_logger,
                    {
                        let mut entry = MoveLogEntry::new("merge_dir");
                        entry.source_path = Some(entry_path.to_string_lossy().to_string());
                        entry.dest_path = Some(dest_path.to_string_lossy().to_string());
                        entry.anchor_root = Some(anchor.path.to_string_lossy().to_string());
                        entry.reason = Some("anchor_dir".to_string());
                        entry.status = Some("merged".to_string());
                        entry
                    },
                );
                log_directory_files(
                    &mut move_logger,
                    &entry_path,
                    &dest_path,
                    Some(&anchor.path),
                    "anchor_dir",
                    &mut move_counts,
                );
            } else {
                info!("Copying DAZ file: {:?} -> library", entry_name);
                if let Some(parent) = dest_path.parent() {
                    if let Err(e) = fs::create_dir_all(parent) {
                        move_counts.errors += 1;
                        log_move_entry(
                            &mut move_logger,
                            {
                                let mut entry = MoveLogEntry::new("error");
                                entry.source_path = Some(entry_path.to_string_lossy().to_string());
                                entry.dest_path = Some(dest_path.to_string_lossy().to_string());
                                entry.reason = Some("create_parent_failed".to_string());
                                entry.message = Some(e.to_string());
                                entry
                            },
                        );
                        return Err(AppError::Io(e));
                    }
                }
                if let Err(e) = fs::copy(&entry_path, &dest_path) {
                    move_counts.errors += 1;
                    log_move_entry(
                        &mut move_logger,
                        {
                            let mut entry = MoveLogEntry::new("error");
                            entry.source_path = Some(entry_path.to_string_lossy().to_string());
                            entry.dest_path = Some(dest_path.to_string_lossy().to_string());
                            entry.reason = Some("copy_file_failed".to_string());
                            entry.message = Some(e.to_string());
                            entry
                        },
                    );
                    return Err(AppError::Io(e));
                }
                moved_count += 1;
                moved_roots.push(entry_path.clone());

                let status = if dest_path.exists() {
                    move_counts.moved_files += 1;
                    "moved"
                } else {
                    move_counts.missing_files += 1;
                    "missing"
                };

                log_move_entry(
                    &mut move_logger,
                    {
                        let mut entry = MoveLogEntry::new("file");
                        entry.source_path = Some(entry_path.to_string_lossy().to_string());
                        entry.dest_path = Some(dest_path.to_string_lossy().to_string());
                        entry.anchor_root = Some(anchor.path.to_string_lossy().to_string());
                        entry.preferred_anchor = Some(preferred_anchor.clone());
                        entry.reason = Some("anchor_root_file".to_string());
                        entry.status = Some(status.to_string());
                        entry
                    },
                );
            }
        }
    }

    // Handle loose files if no anchors were found
    if anchor_result.has_loose_files {
        if let Some(loose_path) = &anchor_result.loose_files_path {
            info!("Processing loose DAZ files from: {:?}", loose_path);

            // For loose files, copy them to a "Content" subfolder to avoid polluting library root
            let loose_dest = default_dest.join("Content");
            fs::create_dir_all(&loose_dest)?;

            for entry in fs::read_dir(loose_path)? {
                let entry = entry?;
                let entry_path = entry.path();

                if entry_path.is_dir() {
                    move_counts.skipped_entries += 1;
                    log_move_entry(
                        &mut move_logger,
                        {
                            let mut entry = MoveLogEntry::new("skip");
                            entry.source_path = Some(entry_path.to_string_lossy().to_string());
                            entry.reason = Some("loose_dir".to_string());
                            entry.status = Some("skipped".to_string());
                            entry
                        },
                    );
                    continue;
                }

                if !entry_path.is_file() {
                    continue;
                }

                if !super::anchors::is_daz_file(&entry_path) {
                    move_counts.skipped_files += 1;
                    log_move_entry(
                        &mut move_logger,
                        {
                            let mut entry = MoveLogEntry::new("skip");
                            entry.source_path = Some(entry_path.to_string_lossy().to_string());
                            entry.reason = Some("non_daz_file".to_string());
                            entry.status = Some("skipped".to_string());
                            entry
                        },
                    );
                    continue;
                }

                let file_name = entry.file_name();
                let dest_file = loose_dest.join(&file_name);

                info!("Copying loose file: {:?}", file_name);
                if let Err(e) = fs::copy(&entry_path, &dest_file) {
                    move_counts.errors += 1;
                    log_move_entry(
                        &mut move_logger,
                        {
                            let mut entry = MoveLogEntry::new("error");
                            entry.source_path = Some(entry_path.to_string_lossy().to_string());
                            entry.dest_path = Some(dest_file.to_string_lossy().to_string());
                            entry.reason = Some("copy_file_failed".to_string());
                            entry.message = Some(e.to_string());
                            entry
                        },
                    );
                    return Err(AppError::Io(e));
                }

                moved_count += 1;
                moved_roots.push(entry_path.clone());

                let status = if dest_file.exists() {
                    move_counts.moved_files += 1;
                    "moved"
                } else {
                    move_counts.missing_files += 1;
                    "missing"
                };

                log_move_entry(
                    &mut move_logger,
                    {
                        let mut entry = MoveLogEntry::new("file");
                        entry.source_path = Some(entry_path.to_string_lossy().to_string());
                        entry.dest_path = Some(dest_file.to_string_lossy().to_string());
                        entry.reason = Some("loose_file".to_string());
                        entry.status = Some(status.to_string());
                        entry
                    },
                );
            }
        }
    }

    log_unmoved_files(
        &mut move_logger,
        temp_dir,
        &moved_roots,
        &mut move_counts,
    );

    if moved_count == 0 {
        warn!("No DAZ content found to move to library!");
    } else {
        info!("Moved {} items to library", moved_count);
    }

    log_move_entry(
        &mut move_logger,
        {
            let mut entry = MoveLogEntry::new("summary");
            entry.counts = Some(move_counts.clone());
            entry
        },
    );

    // Cleanup temp folder
    if let Err(e) = fs::remove_dir_all(temp_dir) {
        warn!("Could not clean up temp dir {:?}: {}", temp_dir, e);
    }

    Ok((default_dest, true))
}

/// Selects the most relevant anchor folder to host files found at an anchor root.
/// Prefers canonical DAZ anchors (data, People, Runtime...) when available.
fn select_primary_anchor(anchors: &[String]) -> String {
    const PRIORITY: &[&str] = &[
        "data",
        "people",
        "runtime",
        "environments",
        "props",
        "poses",
        "lights",
        "light presets",
    ];

    for candidate in PRIORITY {
        if let Some(found) = anchors.iter().find(|a| a.eq_ignore_ascii_case(candidate)) {
            return found.clone();
        }
    }

    anchors
        .first()
        .cloned()
        .unwrap_or_else(|| "Content".to_string())
}

fn build_session_id(source_path: &Path) -> String {
    let timestamp = Utc::now().format("%Y%m%dT%H%M%S").to_string();
    let name = source_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("source");
    let sanitized: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("{}-{}", timestamp, sanitized)
}

fn log_move_entry(logger: &mut Option<MoveLogger>, entry: MoveLogEntry) {
    if let Some(logger) = logger.as_mut() {
        if let Err(e) = logger.log(entry) {
            warn!("[MOVE_LOG] Failed to write entry: {}", e);
        }
    }
}

fn log_directory_files(
    logger: &mut Option<MoveLogger>,
    source_dir: &Path,
    dest_dir: &Path,
    anchor_root: Option<&Path>,
    reason: &str,
    counts: &mut MoveLogCounts,
) {
    let logger = match logger.as_mut() {
        Some(logger) => logger,
        None => return,
    };

    for entry in WalkDir::new(source_dir).min_depth(1) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                counts.errors += 1;
                warn!("[MOVE_LOG] Failed to walk directory: {}", e);
                continue;
            }
        };

        if !entry.file_type().is_file() {
            continue;
        }

        let rel = entry
            .path()
            .strip_prefix(source_dir)
            .unwrap_or(entry.path());
        let dest_path = dest_dir.join(rel);

        let status = if dest_path.exists() {
            counts.moved_files += 1;
            "moved"
        } else {
            counts.missing_files += 1;
            "missing"
        };

        let mut log_entry = MoveLogEntry::new("file");
        log_entry.source_path = Some(entry.path().to_string_lossy().to_string());
        log_entry.dest_path = Some(dest_path.to_string_lossy().to_string());
        if let Some(anchor_root) = anchor_root {
            log_entry.anchor_root = Some(anchor_root.to_string_lossy().to_string());
        }
        log_entry.reason = Some(reason.to_string());
        log_entry.status = Some(status.to_string());

        if let Err(e) = logger.log(log_entry) {
            counts.errors += 1;
            warn!("[MOVE_LOG] Failed to write entry: {}", e);
        }
    }
}

fn log_unmoved_files(
    logger: &mut Option<MoveLogger>,
    temp_dir: &Path,
    moved_roots: &[PathBuf],
    counts: &mut MoveLogCounts,
) {
    let logger = match logger.as_mut() {
        Some(logger) => logger,
        None => return,
    };

    for entry in WalkDir::new(temp_dir).min_depth(1) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                counts.errors += 1;
                warn!("[MOVE_LOG] Failed to walk temp dir: {}", e);
                continue;
            }
        };

        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        if !super::anchors::is_daz_file(path) {
            continue;
        }

        let is_moved = moved_roots.iter().any(|root| path.starts_with(root));
        if is_moved {
            continue;
        }

        counts.unmoved_files += 1;
        let mut log_entry = MoveLogEntry::new("unmoved");
        log_entry.source_path = Some(path.to_string_lossy().to_string());
        log_entry.reason = Some("unmoved_unanchored".to_string());
        log_entry.status = Some("missing".to_string());

        if let Err(e) = logger.log(log_entry) {
            counts.errors += 1;
            warn!("[MOVE_LOG] Failed to write entry: {}", e);
        }
    }
}

/// Finds and extracts nested archives in a folder
///
/// Uses a queue approach to avoid re-scanning the entire folder
/// after each extraction.
fn extract_nested_archives(
    dir: &Path,
    current_depth: usize,
    max_depth: usize,
    nested_archives: &mut Vec<NestedArchiveInfo>,
    max_depth_reached: &mut usize,
) -> AppResult<PathBuf> {
    use std::collections::VecDeque;

    if current_depth > max_depth {
        warn!("Max recursion depth {} reached", max_depth);
        return Ok(dir.to_path_buf());
    }

    *max_depth_reached = (*max_depth_reached).max(current_depth);

    // Queue of archives to process
    let mut queue: VecDeque<PathBuf> = find_archives_in_dir(dir)?.into_iter().collect();

    if !queue.is_empty() {
        info!(
            "Found {} archive(s) at depth {}",
            queue.len(),
            current_depth
        );
    }

    while let Some(archive_path) = queue.pop_front() {
        // Check that the file still exists
        if !archive_path.exists() {
            warn!("Archive no longer exists, skipping: {:?}", archive_path);
            continue;
        }

        // Extract and get newly found archives
        let new_archives = extract_single_nested_archive_queued(
            &archive_path,
            dir,
            current_depth,
            nested_archives,
            max_depth_reached,
        )?;

        // Add new archives to the queue
        queue.extend(new_archives);
    }

    Ok(dir.to_path_buf())
}

/// Extracts nested archives with notifications
///
/// Uses a parallel batch approach: extracts all archives
/// at the current level in parallel, then merges sequentially.
fn extract_nested_archives_with_events<F>(
    dir: &Path,
    current_depth: usize,
    max_depth: usize,
    nested_archives: &mut Vec<NestedArchiveInfo>,
    max_depth_reached: &mut usize,
    emit_step: F,
) -> AppResult<PathBuf>
where
    F: Fn(&str, Option<&str>) + Clone + Send + Sync,
{
    if current_depth > max_depth {
        return Ok(dir.to_path_buf());
    }

    *max_depth_reached = (*max_depth_reached).max(current_depth);

    // Find all archives at the current level
    let mut archives_to_process = find_archives_in_dir(dir)?;
    let mut total_processed = 0;

    if !archives_to_process.is_empty() {
        emit_step(
            &format!("{} nested archive(s) detected", archives_to_process.len()),
            None,
        );
    }

    // Loop while there are archives to process
    while !archives_to_process.is_empty() {
        let batch_size = archives_to_process.len();

        emit_step(
            &format!("Parallel extraction of {} archive(s)...", batch_size),
            None,
        );

        // Mutex for thread-safe result collection
        let nested_mutex: Mutex<Vec<NestedArchiveInfo>> = Mutex::new(Vec::new());
        let new_archives_mutex: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
        let errors_mutex: Mutex<Vec<String>> = Mutex::new(Vec::new());

        // Extract all archives in parallel
        archives_to_process.par_iter().for_each(|archive_path| {
            if !archive_path.exists() {
                warn!("Archive no longer exists, skipping: {:?}", archive_path);
                return;
            }

            let archive_name = archive_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("archive");

            let format = match ArchiveFormat::from_extension(archive_path) {
                Some(f) => f,
                None => {
                    warn!("Unknown format for {:?}, skipping", archive_path);
                    return;
                }
            };

            // Create a unique destination folder for this archive
            let archive_stem = archive_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("nested");
            let extract_dir = dir.join(format!("{}_extracted", archive_stem));

            // Extract the archive
            match extract_archive_by_format(archive_path, &extract_dir, format) {
                Ok(_) => {
                    // Normalize DAZ structure (remove wrappers like "Content")
                    if let Ok(normalized) = super::utils::normalize_daz_structure(&extract_dir) {
                        if normalized {
                            debug!("Normalized DAZ structure in {:?}", extract_dir);
                        }
                    }

                    // Count extracted content
                    if let Ok(nested_stats) = count_directory_contents(&extract_dir) {
                        // Record info
                        if let Ok(mut nested) = nested_mutex.lock() {
                            nested.push(NestedArchiveInfo {
                                name: archive_name.to_string(),
                                format,
                                depth: current_depth,
                                file_count: nested_stats.files,
                                total_size: nested_stats.size_bytes,
                            });
                        }

                        // Scan for new archives in extracted content
                        if let Ok(new_archives) = find_archives_in_dir(&extract_dir) {
                            if let Ok(mut queue) = new_archives_mutex.lock() {
                                // Calculate relative paths for after the merge
                                for archive in new_archives {
                                    if let Ok(relative) = archive.strip_prefix(&extract_dir) {
                                        let relocated = dir.join(relative);
                                        queue.push(relocated);
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    if let Ok(mut errs) = errors_mutex.lock() {
                        errs.push(format!("{}: {}", archive_name, e));
                    }
                }
            }
        });

        // Sequential phase: delete source archives and merge
        for archive_path in &archives_to_process {
            if !archive_path.exists() {
                continue;
            }

            let archive_stem = archive_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("nested");
            let extract_dir = dir.join(format!("{}_extracted", archive_stem));

            // Delete source archive
            if let Err(e) = fs::remove_file(archive_path) {
                warn!(
                    "Could not remove extracted archive {:?}: {}",
                    archive_path, e
                );
            }

            // Merge extracted content to parent
            if extract_dir.exists() {
                if let Err(e) = merge_directories(&extract_dir, dir) {
                    warn!("Could not merge {:?}: {}", extract_dir, e);
                }

                // Cleanup temporary folder
                if let Err(e) = fs::remove_dir_all(&extract_dir) {
                    warn!("Could not remove temp dir {:?}: {}", extract_dir, e);
                }
            }
        }

        // Retrieve results
        let batch_nested = nested_mutex.into_inner().unwrap_or_default();
        let batch_new_archives = new_archives_mutex.into_inner().unwrap_or_default();
        let batch_errors = errors_mutex.into_inner().unwrap_or_default();

        // Log errors
        for err in batch_errors {
            warn!("Extraction error: {}", err);
        }

        // Add extracted archives info
        total_processed += batch_nested.len();
        nested_archives.extend(batch_nested);

        emit_step(&format!("{} archive(s) extracted", total_processed), None);

        // Filter new archives that actually exist after the merge
        archives_to_process = batch_new_archives
            .into_iter()
            .filter(|p| p.exists())
            .collect();

        if !archives_to_process.is_empty() {
            debug!("Found {} more nested archive(s)", archives_to_process.len());
        }
    }

    Ok(dir.to_path_buf())
}

/// Extracts a single nested archive (queue version)
/// Returns the list of new archives found in the extracted content
fn extract_single_nested_archive_queued(
    archive_path: &Path,
    parent_dir: &Path,
    current_depth: usize,
    nested_archives: &mut Vec<NestedArchiveInfo>,
    max_depth_reached: &mut usize,
) -> AppResult<Vec<PathBuf>> {
    let format = match ArchiveFormat::from_extension(archive_path) {
        Some(f) => f,
        None => return Ok(Vec::new()),
    };

    // Check that the format is supported
    if !super::is_format_supported(format) {
        warn!("Skipping unsupported format: {:?}", archive_path);
        return Ok(Vec::new());
    }

    let archive_name = archive_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("nested");

    // Create a temporary folder for this extraction
    let extract_dest = archive_path
        .parent()
        .unwrap_or(parent_dir)
        .join(format!("{}_extracted", archive_name));

    info!(
        "Extracting nested archive: {:?} -> {:?}",
        archive_path, extract_dest
    );

    let extract_stats = extract_archive_by_format(archive_path, &extract_dest, format)?;

    *max_depth_reached = (*max_depth_reached).max(current_depth);

    // Record nested archive info
    nested_archives.push(NestedArchiveInfo {
        name: archive_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string(),
        format,
        depth: current_depth,
        file_count: extract_stats.files,
        total_size: extract_stats.size_bytes,
    });

    // Normalize DAZ structure (remove wrappers like "Content")
    if let Ok(normalized) = super::utils::normalize_daz_structure(&extract_dest) {
        if normalized {
            debug!("Normalized DAZ structure in {:?}", extract_dest);
        }
    }

    // Scan for new archives BEFORE deleting/merging
    let new_archives_in_extract = find_archives_in_dir(&extract_dest)?;

    // Delete source archive after successful extraction
    if let Err(e) = fs::remove_file(archive_path) {
        warn!(
            "Could not delete extracted archive {:?}: {}",
            archive_path, e
        );
    }

    // Merge extracted content into parent folder
    merge_directories(&extract_dest, parent_dir)?;

    // Cleanup temporary folder
    if let Err(e) = fs::remove_dir_all(&extract_dest) {
        warn!("Could not remove temp dir {:?}: {}", extract_dest, e);
    }

    // Recalculate archive paths after the merge
    // They are now in parent_dir instead of extract_dest
    let mut relocated_archives = Vec::new();
    for archive in new_archives_in_extract {
        if let Ok(relative) = archive.strip_prefix(&extract_dest) {
            let new_path = parent_dir.join(relative);
            if new_path.exists() {
                debug!("Relocated nested archive: {:?}", new_path);
                relocated_archives.push(new_path);
            } else {
                warn!("Nested archive not found after merge: {:?}", new_path);
            }
        }
    }

    Ok(relocated_archives)
}

/// Extracts a single nested archive (recursive version - legacy)
#[allow(dead_code)]
fn extract_single_nested_archive(
    archive_path: &Path,
    parent_dir: &Path,
    current_depth: usize,
    max_depth: usize,
    nested_archives: &mut Vec<NestedArchiveInfo>,
    max_depth_reached: &mut usize,
) -> AppResult<()> {
    let format = match ArchiveFormat::from_extension(archive_path) {
        Some(f) => f,
        None => return Ok(()),
    };

    // Check that the format is supported
    if !super::is_format_supported(format) {
        warn!("Skipping unsupported format: {:?}", archive_path);
        return Ok(());
    }

    let archive_name = archive_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("nested");

    // Create a temporary folder for this extraction
    let extract_dest = archive_path
        .parent()
        .unwrap_or(parent_dir)
        .join(format!("{}_extracted", archive_name));

    info!(
        "Extracting nested archive: {:?} -> {:?}",
        archive_path, extract_dest
    );

    let extract_stats = extract_archive_by_format(archive_path, &extract_dest, format)?;

    // Record nested archive info
    nested_archives.push(NestedArchiveInfo {
        name: archive_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string(),
        format,
        depth: current_depth,
        file_count: extract_stats.files,
        total_size: extract_stats.size_bytes,
    });

    // Delete source archive after successful extraction
    if let Err(e) = fs::remove_file(archive_path) {
        warn!(
            "Could not delete extracted archive {:?}: {}",
            archive_path, e
        );
    }

    // Recurse on extracted content
    extract_nested_archives(
        &extract_dest,
        current_depth + 1,
        max_depth,
        nested_archives,
        max_depth_reached,
    )?;

    Ok(())
}

/// Extracts an archive according to its format
///
/// Handles multi-part archives automatically:
/// - RAR multi-part: delegates to unrar with the first part
/// - ZIP split: reassembles parts then extracts
pub fn extract_archive_by_format(
    archive_path: &Path,
    dest_dir: &Path,
    format: ArchiveFormat,
) -> AppResult<ContentStats> {
    use super::multipart;

    let mp_info = multipart::detect_multipart(archive_path);

    match (&mp_info, format) {
        // ZIP split: reassemble then extract
        (Some(info), ArchiveFormat::Zip)
            if info.format == multipart::MultiPartFormat::ZipSplit =>
        {
            let reassembled = multipart::reassemble_zip_split(info, dest_dir)?;
            let stats = super::zip::extract_zip(&reassembled, dest_dir)?;
            let _ = std::fs::remove_file(&reassembled);
            Ok(stats)
        }
        // RAR multi-part: unrar handles it natively
        (Some(info), ArchiveFormat::Rar) => {
            super::rar::extract_rar(&info.first_part, dest_dir)
        }
        // Regular archives
        (_, ArchiveFormat::Zip) => super::zip::extract_zip(archive_path, dest_dir),
        (_, ArchiveFormat::SevenZip) => super::seven_zip::extract_7z(archive_path, dest_dir),
        (_, ArchiveFormat::Rar) => super::rar::extract_rar(archive_path, dest_dir),
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::settings::AppSettings;
    use crate::config::SETTINGS;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Serialize SETTINGS mutations across tests to avoid interleaving
    static TEST_SETTINGS_MUTEX: Lazy<std::sync::Mutex<()>> =
        Lazy::new(|| std::sync::Mutex::new(()));

    fn unique_path(name: &str) -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        std::env::temp_dir().join(format!("fmd_test_{}_{}", name, ts))
    }

    fn with_default_library(lib_path: &Path) -> AppSettings {
        let original = SETTINGS.read().unwrap().clone();
        {
            let mut settings = SETTINGS.write().unwrap();
            settings.default_destination = Some(lib_path.to_path_buf());
        }
        original
    }

    fn restore_settings(original: AppSettings) {
        if let Ok(mut settings) = SETTINGS.write() {
            *settings = original;
        }
    }

    #[test]
    fn anchor_root_file_stays_under_anchor() -> AppResult<()> {
        let _guard = TEST_SETTINGS_MUTEX.lock().unwrap();

        let base = unique_path("anchor_root");
        let extraction = base.join("extract");
        let lib = base.join("lib");

        fs::create_dir_all(&lib)?;
        fs::create_dir_all(extraction.join("data"))?;
        fs::write(extraction.join("data").join("MonFichier.duf"), b"dummy")?;

        let original = with_default_library(&lib);
        let result = move_to_default_library(&extraction, &extraction)?;
        restore_settings(original);

        assert!(result.1, "content should be moved to library");
        assert!(lib.join("data").join("MonFichier.duf").exists());
        assert!(!lib.join("MonFichier.duf").exists());

        let _ = fs::remove_dir_all(base);
        Ok(())
    }

    #[test]
    fn anchor_subfolder_keeps_relative_path() -> AppResult<()> {
        let _guard = TEST_SETTINGS_MUTEX.lock().unwrap();

        let base = unique_path("anchor_subfolder");
        let extraction = base.join("extract");
        let lib = base.join("lib");

        fs::create_dir_all(&lib)?;
        fs::create_dir_all(extraction.join("People").join("Char"))?;
        fs::write(
            extraction.join("People").join("Char").join("Bar.duf"),
            b"dummy",
        )?;

        let original = with_default_library(&lib);
        let result = move_to_default_library(&extraction, &extraction)?;
        restore_settings(original);

        assert!(result.1);
        assert!(lib.join("People").join("Char").join("Bar.duf").exists());
        assert!(!lib.join("Bar.duf").exists());

        let _ = fs::remove_dir_all(base);
        Ok(())
    }

    #[test]
    fn loose_file_goes_to_content_folder() -> AppResult<()> {
        let _guard = TEST_SETTINGS_MUTEX.lock().unwrap();

        let base = unique_path("loose_file");
        let extraction = base.join("extract");
        let lib = base.join("lib");

        fs::create_dir_all(&lib)?;
        fs::create_dir_all(&extraction)?;
        fs::write(extraction.join("Loose.duf"), b"dummy")?;

        let original = with_default_library(&lib);
        let result = move_to_default_library(&extraction, &extraction)?;
        restore_settings(original);

        assert!(result.1);
        assert!(lib.join("Content").join("Loose.duf").exists());
        assert!(!lib.join("Loose.duf").exists());

        let _ = fs::remove_dir_all(base);
        Ok(())
    }
}
