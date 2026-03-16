//! # Archive Processing Commands
//!
//! Tauri commands for processing source archives and folders.
//!
//! ## Overview
//!
//! This module exposes the archive extraction engine to the frontend:
//! - Single and batch source processing
//! - Recursive nested archive extraction
//! - Progress event emission for UI feedback
//! - Post-import trash functionality
//!
//! ## Threading Model
//!
//! All I/O-heavy operations use `tokio::spawn_blocking` to avoid
//! blocking Tauri's main thread. Event emission via `app.emit()`
//! is thread-safe and can be called from blocking threads.

use crate::config::SettingsState;
use crate::core::analyzer::{analyze_content, ContentType};
use crate::core::extractor::{
    get_supported_formats, is_format_supported, process_source, process_source_recursive,
    process_source_recursive_with_events, ArchiveFormat, ExtractResult, RecursiveExtractResult,
    SourceType,
};
use crate::error::{ApiResponse, AppError, AppResult};
use serde::Serialize;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, State};
use tokio::task::spawn_blocking;
use tracing::{error, info};

// =============================================================================
// RESPONSE TYPES
// =============================================================================

/// Information about a source file or directory.
///
/// Used for quick source inspection without full processing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    /// Full path to the source.
    pub path: String,
    /// File or directory name.
    pub name: String,
    /// Whether it's an archive or directory.
    pub source_type: SourceType,
    /// Archive format (if applicable).
    pub archive_format: Option<ArchiveFormat>,
    /// File size in bytes (for archives only).
    pub file_size: Option<u64>,
    /// Whether the format is supported for extraction.
    pub is_supported: bool,
}

/// Information about currently supported archive formats.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportedFormats {
    /// List of supported format extensions.
    pub formats: Vec<String>,
    /// Whether RAR extraction is available.
    pub can_extract_rar: bool,
    /// Whether 7z extraction is available.
    pub can_extract_7z: bool,
}

/// Frontend-friendly destination proposal shape.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationProposalFront {
    pub recommended_path: String,
    pub library_path: String,
    pub subfolders: Vec<String>,
    pub full_paths: Vec<String>,
    pub content_type: Option<String>,
    /// Confidence 0.0 - 1.0
    pub confidence: f32,
    pub reason: String,
    pub alternatives: Vec<DestinationAlternativeFront>,
}

/// Alternative destination for the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationAlternativeFront {
    pub path: String,
    pub library_path: String,
    pub subfolder: String,
    pub label: String,
    pub content_type: Option<String>,
    /// Confidence 0.0 - 1.0
    pub confidence: f32,
}

// =============================================================================
// TAURI COMMANDS
// =============================================================================

/// Processes a single source (archive or folder).
///
/// This is the main command called on file drop or selection.
/// Extracts archives and analyzes content structure.
///
/// Runs on a blocking thread to avoid UI freezes.
#[tauri::command]
pub async fn process_source_cmd(
    path: String,
    settings: State<'_, SettingsState>,
) -> Result<ApiResponse<ExtractResult>, String> {
    info!("process_source_cmd: {}", path);

    let settings_snapshot = settings.read().map_err(|e| format!("Settings lock poisoned: {}", e))?.clone();

    let result = spawn_blocking(move || {
        let source_path = PathBuf::from(&path);
        catch_unwind_safe(|| process_source(&source_path, &settings_snapshot))
    })
    .await;

    match result {
        Ok(Ok(extract_result)) => {
            info!(
                "Source processed successfully: {} files, {:?}",
                extract_result.file_count,
                extract_result.content_type()
            );
            Ok(ApiResponse::success(extract_result))
        }
        Ok(Err(app_error)) => {
            error!("Failed to process source: {}", app_error);
            Ok(ApiResponse::error(app_error))
        }
        Err(join_error) => {
            error!("Task panicked: {}", join_error);
            Ok(ApiResponse::error(AppError::Internal(format!("Task error: {}", join_error))))
        }
    }
}

/// Gets source metadata without processing.
///
/// Returns information about the source type, format, and support status.
#[tauri::command]
pub async fn get_source_info(
    path: String,
    settings: State<'_, SettingsState>,
) -> Result<ApiResponse<SourceInfo>, String> {
    info!("get_source_info: {}", path);

    let settings_snapshot = settings.read().map_err(|e| format!("Settings lock poisoned: {}", e))?.clone();
    let source_path = PathBuf::from(&path);

    match build_source_info(&source_path, &settings_snapshot) {
        Ok(info) => Ok(ApiResponse::success(info)),
        Err(app_error) => {
            error!("Failed to get source info: {}", app_error);
            Ok(ApiResponse::error(app_error))
        }
    }
}

/// Returns the list of currently supported archive formats.
#[tauri::command]
pub fn get_supported_formats_cmd(
    settings: State<'_, SettingsState>,
) -> ApiResponse<SupportedFormats> {
    let settings_guard = match settings.read() {
        Ok(s) => s,
        Err(e) => return ApiResponse::error(AppError::Config(format!("Settings lock poisoned: {}", e))),
    };
    let formats = get_supported_formats(&settings_guard);

    let supported = SupportedFormats {
        formats: formats.iter().map(|f| f.extension().to_string()).collect(),
        can_extract_rar: formats.contains(&ArchiveFormat::Rar),
        can_extract_7z: formats.contains(&ArchiveFormat::SevenZip),
    };

    ApiResponse::success(supported)
}

/// Processes multiple sources in batch.
///
/// Returns results for each source, including both successes and failures.
/// Runs on a blocking thread.
#[tauri::command]
pub async fn process_sources_batch(
    paths: Vec<String>,
    settings: State<'_, SettingsState>,
) -> Result<ApiResponse<Vec<BatchResult>>, String> {
    info!("process_sources_batch: {} sources", paths.len());

    let settings_snapshot = settings.read().map_err(|e| format!("Settings lock poisoned: {}", e))?.clone();

    let result = spawn_blocking(move || {
        paths
            .into_iter()
            .map(|path| {
                let source_path = PathBuf::from(&path);
                match catch_unwind_safe(|| process_source(&source_path, &settings_snapshot)) {
                    Ok(extract_result) => BatchResult {
                        path,
                        success: true,
                        result: Some(extract_result),
                        error: None,
                    },
                    Err(app_error) => BatchResult {
                        path,
                        success: false,
                        result: None,
                        error: Some(app_error.to_string()),
                    },
                }
            })
            .collect::<Vec<_>>()
    })
    .await;

    match result {
        Ok(batch_results) => {
            let success_count = batch_results.iter().filter(|r| r.success).count();
            info!(
                "Batch complete: {}/{} succeeded",
                success_count,
                batch_results.len()
            );
            Ok(ApiResponse::success(batch_results))
        }
        Err(join_error) => {
            error!("Batch task panicked: {}", join_error);
            Ok(ApiResponse::error(AppError::Internal(format!("Task error: {}", join_error))))
        }
    }
}

/// Processes a source with recursive extraction of nested archives.
///
/// Automatically extracts archives found inside other archives,
/// up to `max_depth` levels deep (default: 5).
///
/// Runs on a blocking thread.
#[tauri::command]
pub async fn process_source_recursive_cmd(
    path: String,
    max_depth: Option<usize>,
    settings: State<'_, SettingsState>,
) -> Result<ApiResponse<RecursiveExtractResult>, String> {
    let depth = max_depth.unwrap_or(5);
    info!("process_source_recursive: {} (max_depth: {})", path, depth);

    let settings_snapshot = settings.read().map_err(|e| format!("Settings lock poisoned: {}", e))?.clone();

    let result = spawn_blocking(move || {
        let source_path = PathBuf::from(&path);
        catch_unwind_safe(|| process_source_recursive(&source_path, depth, &settings_snapshot))
    })
    .await;

    match result {
        Ok(Ok(extract_result)) => {
            info!(
                "Recursive extraction complete: {} files, {} nested archives, max depth {}",
                extract_result.total_files,
                extract_result.nested_archives.len(),
                extract_result.max_depth_reached
            );
            Ok(ApiResponse::success(extract_result))
        }
        Ok(Err(app_error)) => {
            error!("Failed to process source recursively: {}", app_error);
            Ok(ApiResponse::error(app_error))
        }
        Err(join_error) => {
            error!("Task panicked: {}", join_error);
            Ok(ApiResponse::error(AppError::Internal(format!("Task error: {}", join_error))))
        }
    }
}

/// Event emitted for each extraction step.
///
/// Sent via Tauri's event system with event name "import_step".
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportStepEvent {
    /// Task ID (provided by frontend for correlation).
    pub task_id: String,
    /// Human-readable step description.
    pub message: String,
    /// Optional additional details.
    pub details: Option<String>,
    /// Unix timestamp in milliseconds.
    pub timestamp: u64,
}

/// Processes a source with recursive extraction AND progress events.
///
/// Identical to `process_source_recursive_cmd` but emits "import_step"
/// events for real-time progress tracking in the UI.
///
/// Event emission uses `app.emit()` which is thread-safe.
#[tauri::command]
pub async fn process_source_recursive_with_events_cmd(
    app: AppHandle,
    task_id: String,
    path: String,
    max_depth: Option<usize>,
    settings: State<'_, SettingsState>,
) -> Result<ApiResponse<RecursiveExtractResult>, String> {
    info!(
        "process_source_recursive_with_events: {} (task_id: {}, max_depth: {:?})",
        path, task_id, max_depth
    );

    let depth = max_depth.unwrap_or(5);
    info!(
        "process_source_recursive_with_events: {} (task_id: {}, max_depth: {})",
        path, task_id, depth
    );

    let settings_snapshot = settings.read().map_err(|e| format!("Settings lock poisoned: {}", e))?.clone();
    let task_id_for_events = task_id.clone();
    let app_for_events = app.clone();

    let result = spawn_blocking(move || {
        let source_path = PathBuf::from(&path);

        // Event emission callback (called from blocking thread)
        let emit_step = |message: &str, details: Option<&str>| {
            let event = ImportStepEvent {
                task_id: task_id_for_events.clone(),
                message: message.to_string(),
                details: details.map(String::from),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            };
            if let Err(emit_error) = app_for_events.emit("import_step", &event) {
                error!("Failed to emit import_step event: {}", emit_error);
            }
        };

        catch_unwind_safe(|| process_source_recursive_with_events(&source_path, depth, &settings_snapshot, emit_step))
    })
    .await;

    match result {
        Ok(Ok(extract_result)) => {
            info!(
                "Recursive extraction complete: {} files, {} nested archives, max depth {}",
                extract_result.total_files,
                extract_result.nested_archives.len(),
                extract_result.max_depth_reached
            );
            Ok(ApiResponse::success(extract_result))
        }
        Ok(Err(app_error)) => {
            error!("Failed to process source recursively: {}", app_error);
            Ok(ApiResponse::error(app_error))
        }
        Err(join_error) => {
            error!("Task panicked: {}", join_error);
            Ok(ApiResponse::error(AppError::Internal(format!("Task error: {}", join_error))))
        }
    }
}

/// Proposes a destination path based on content analysis.
///
/// Frontend expects:
/// - Params: tempPath (extracted content path), optional libraryPath override
/// - Response: recommendedPath, libraryPath, fullPaths, confidence (0.0-1.0),
///             contentType (lowercase), alternatives with labels.
#[tauri::command]
pub fn propose_destination_cmd(
    temp_path: String,
    library_path: Option<String>,
    settings: State<'_, SettingsState>,
) -> ApiResponse<DestinationProposalFront> {
    use crate::core::destination::propose_destination;

    let settings_guard = match settings.read() {
        Ok(s) => s,
        Err(e) => return ApiResponse::error(AppError::Config(format!("Settings lock poisoned: {}", e))),
    };

    let temp = PathBuf::from(&temp_path);
    if !temp.exists() {
        return ApiResponse::error(AppError::NotFound(temp));
    }

    // Analyze extracted content
    let analysis = match analyze_content(&temp) {
        Ok(a) => a,
        Err(e) => {
            error!("Failed to analyze content: {}", e);
            return ApiResponse::error(e);
        }
    };

    let source_name = temp
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("content")
        .to_string();

    match propose_destination(&analysis, &source_name, &settings_guard) {
        Ok(proposal) => {
            let front = DestinationProposalFront::from_backend(proposal, &analysis, library_path);
            ApiResponse::success(front)
        }
        Err(app_error) => {
            error!("Failed to propose destination: {}", app_error);
            ApiResponse::error(app_error)
        }
    }
}

/// Moves extracted content to a user-specified destination.
///
/// Merges the source folder into the destination, then cleans up the source.
/// Runs on a blocking thread.
#[tauri::command]
pub async fn move_to_custom_destination(
    source_path: String,
    destination_path: String,
) -> ApiResponse<MoveResult> {
    info!("Moving {} to {}", source_path, destination_path);

    let result = spawn_blocking(move || {
        let source = PathBuf::from(&source_path);
        let destination = PathBuf::from(&destination_path);

        if !source.exists() {
            return Err(AppError::NotFound(source));
        }

        // Ensure destination directory exists
        std::fs::create_dir_all(&destination)?;

        let mut counters = MoveCounters::default();
        merge_directory_with_counts(&source, &destination, &mut counters)?;

        // Clean up the source folder
        if let Err(cleanup_error) = std::fs::remove_dir_all(&source) {
            error!("Failed to cleanup source: {}", cleanup_error);
        }

        Ok(MoveResult {
            success: counters.errors.is_empty(),
            source_path: source_path.clone(),
            destination_path: destination_path.clone(),
            files_moved: counters.files_moved,
            files_skipped: counters.files_skipped,
            errors: counters.errors,
        })
    })
    .await;

    match result {
        Ok(Ok(move_result)) => {
            info!(
                "Move completed: {} files, {} skipped",
                move_result.files_moved, move_result.files_skipped
            );
            ApiResponse::success(move_result)
        }
        Ok(Err(app_error)) => {
            error!("Failed to move content: {}", app_error);
            ApiResponse::error(app_error)
        }
        Err(join_error) => {
            error!("Task panicked: {}", join_error);
            ApiResponse::error(AppError::Internal(format!("Task error: {}", join_error)))
        }
    }
}

#[derive(Debug, Default)]
struct MoveCounters {
    files_moved: usize,
    files_skipped: usize,
    errors: Vec<String>,
}

/// Recursively merges contents from source to destination directory with counting.
fn merge_directory_with_counts(
    source: &Path,
    destination: &Path,
    counters: &mut MoveCounters,
) -> AppResult<()> {
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let entry_source_path = entry.path();
        let entry_dest_path = destination.join(entry.file_name());

        if entry_source_path.is_dir() {
            std::fs::create_dir_all(&entry_dest_path)?;
            merge_directory_with_counts(&entry_source_path, &entry_dest_path, counters)?;
        } else {
            if let Some(parent) = entry_dest_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            match std::fs::copy(&entry_source_path, &entry_dest_path) {
                Ok(_) => counters.files_moved += 1,
                Err(e) => {
                    counters.files_skipped += 1;
                    counters.errors.push(format!(
                        "{} -> {}: {}",
                        entry_source_path.display(),
                        entry_dest_path.display(),
                        e
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Wraps a fallible operation and converts panics into AppError::Internal
fn catch_unwind_safe<F, T>(f: F) -> AppResult<T>
where
    F: FnOnce() -> AppResult<T>,
{
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(res) => res,
        Err(_) => Err(AppError::Internal(
            "Operation panicked during extraction".to_string(),
        )),
    }
}

// =============================================================================
// Helpers for destination mapping
// =============================================================================

impl DestinationProposalFront {
    fn from_backend(
        backend: crate::core::destination::DestinationProposal,
        analysis: &crate::core::analyzer::AnalysisSummary,
        override_library: Option<String>,
    ) -> Self {
        let library_path = override_library.unwrap_or_else(|| backend.library_path.clone());
        let base = PathBuf::from(&library_path);

        // Build recommended full path with override if provided
        let recommended_path = if backend.relative_path.is_empty() {
            backend.full_path.clone()
        } else {
            base.join(&backend.relative_path)
                .to_string_lossy()
                .to_string()
        };

        let mut full_paths = vec![recommended_path.clone()];
        full_paths.extend(backend.alternatives.iter().map(|alt| {
            let base_alt = PathBuf::from(&library_path);
            base_alt
                .join(&alt.relative_path)
                .to_string_lossy()
                .to_string()
        }));

        let alternatives = backend
            .alternatives
            .iter()
            .map(|alt| {
                let base_alt = PathBuf::from(&library_path);
                DestinationAlternativeFront {
                    path: base_alt
                        .join(&alt.relative_path)
                        .to_string_lossy()
                        .to_string(),
                    library_path: library_path.clone(),
                    subfolder: alt.relative_path.clone(),
                    label: alt.reason.clone(),
                    content_type: analysis_content_type(analysis),
                    confidence: (backend.confidence as f32 / 100.0).clamp(0.0, 1.0),
                }
            })
            .collect();

        Self {
            recommended_path,
            library_path: library_path.clone(),
            subfolders: backend
                .relative_path
                .split(|c| c == '/' || c == '\\')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect(),
            full_paths,
            content_type: analysis_content_type(analysis),
            confidence: (backend.confidence as f32 / 100.0).clamp(0.0, 1.0),
            reason: backend.reason,
            alternatives,
        }
    }
}

fn analysis_content_type(analysis: &crate::core::analyzer::AnalysisSummary) -> Option<String> {
    let ct = match analysis.content_type {
        ContentType::Character => "character",
        ContentType::Clothing => "clothing",
        ContentType::Hair => "hair",
        ContentType::Prop => "prop",
        ContentType::Environment => "environment",
        ContentType::Pose => "pose",
        ContentType::Light => "light",
        ContentType::Material => "material",
        ContentType::Script => "script",
        ContentType::Morph => "morph",
        ContentType::Hdri => "hdri",
        ContentType::Other => "other",
    };
    Some(ct.to_string())
}

// =============================================================================
// BATCH RESULT TYPE
// =============================================================================

/// Result of processing a single source in a batch operation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchResult {
    /// Path to the processed source.
    pub path: String,
    /// Whether processing succeeded.
    pub success: bool,
    /// Extraction result (on success).
    pub result: Option<ExtractResult>,
    /// Error message (on failure).
    pub error: Option<String>,
}

/// Result of a move operation (frontend expects this shape).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveResult {
    pub success: bool,
    pub source_path: String,
    pub destination_path: String,
    pub files_moved: usize,
    pub files_skipped: usize,
    pub errors: Vec<String>,
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Builds source information from a path.
fn build_source_info(path: &Path, settings: &crate::config::AppSettings) -> AppResult<SourceInfo> {
    if !path.exists() {
        return Err(AppError::NotFound(path.to_path_buf()));
    }

    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    if path.is_dir() {
        Ok(SourceInfo {
            path: path.to_string_lossy().to_string(),
            name,
            source_type: SourceType::Directory,
            archive_format: None,
            file_size: None,
            is_supported: true,
        })
    } else {
        let format = ArchiveFormat::from_extension(path);
        let is_supported = format.map(|f| is_format_supported(f, settings)).unwrap_or(false);
        let file_size = std::fs::metadata(path).map(|m| m.len()).ok();

        Ok(SourceInfo {
            path: path.to_string_lossy().to_string(),
            name,
            source_type: SourceType::Archive,
            archive_format: format,
            file_size,
            is_supported,
        })
    }
}

// =============================================================================
// TRASH ARCHIVE AFTER IMPORT
// =============================================================================

/// Result of attempting to move an archive to trash.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashResult {
    /// Whether the file was successfully moved to trash.
    pub trashed: bool,
    /// Path of the processed file.
    pub path: String,
    /// Error message if move failed (import remains successful).
    pub error: Option<String>,
}

/// Moves a source archive to trash after successful import.
///
/// This command performs several checks:
/// 1. Global `trash_archives_after_import` setting is enabled
/// 2. The path is an archive file (ZIP/RAR/7z)
/// 3. The destination exists (confirming successful import)
///
/// If the trash operation fails, it does NOT affect the import status.
#[tauri::command]
pub async fn trash_source_archive(
    source_path: String,
    destination_path: String,
    settings: tauri::State<'_, crate::config::SettingsState>,
) -> Result<ApiResponse<TrashResult>, String> {
    use crate::core::trash::{is_archive_file, move_to_trash};

    info!(
        "trash_source_archive: {} (dest: {})",
        source_path, destination_path
    );

    let source = PathBuf::from(&source_path);
    let destination = PathBuf::from(&destination_path);

    // Check global setting
    let trash_enabled = settings
        .read()
        .map(|s| s.trash_archives_after_import)
        .unwrap_or(false);

    if !trash_enabled {
        info!("Trash after import is disabled, skipping");
        return Ok(ApiResponse::success(TrashResult {
            trashed: false,
            path: source_path,
            error: None,
        }));
    }

    // Check that it's an archive file
    if !is_archive_file(&source) {
        info!("Source is not an archive file, skipping trash");
        return Ok(ApiResponse::success(TrashResult {
            trashed: false,
            path: source_path,
            error: None,
        }));
    }

    // Check that source still exists
    if !source.exists() {
        info!("Source archive no longer exists, skipping trash");
        return Ok(ApiResponse::success(TrashResult {
            trashed: false,
            path: source_path,
            error: Some("Source archive no longer exists".to_string()),
        }));
    }

    // Check that destination exists (successful import)
    if !destination.exists() {
        info!("Destination does not exist, skipping trash");
        return Ok(ApiResponse::success(TrashResult {
            trashed: false,
            path: source_path,
            error: Some("Destination does not exist, import not confirmed".to_string()),
        }));
    }

    // Execute trash move on a blocking thread
    let source_for_trash = source.clone();
    let result = spawn_blocking(move || move_to_trash(&source_for_trash)).await;

    match result {
        Ok(Ok(_)) => {
            info!("Archive successfully moved to trash: {}", source_path);
            Ok(ApiResponse::success(TrashResult {
                trashed: true,
                path: source_path,
                error: None,
            }))
        }
        Ok(Err(trash_error)) => {
            // Trash failed, but import remains successful
            let error_msg = format!("Failed to move to trash: {}", trash_error);
            error!("{}", error_msg);
            Ok(ApiResponse::success(TrashResult {
                trashed: false,
                path: source_path,
                error: Some(error_msg),
            }))
        }
        Err(join_error) => {
            let error_msg = format!("Error during trash operation: {}", join_error);
            error!("{}", error_msg);
            Ok(ApiResponse::success(TrashResult {
                trashed: false,
                path: source_path,
                error: Some(error_msg),
            }))
        }
    }
}

// =============================================================================
// BATCH NORMALIZATION
// =============================================================================

use crate::core::extractor::{normalize_and_merge_batch, NormalizeBatchResult};

/// Event emitted during batch folder normalization.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizeStepEvent {
    /// Current step name.
    pub step: String,
    /// Optional step details.
    pub detail: Option<String>,
}

/// Normalizes and merges a "messy" folder to the DAZ library.
///
/// Processes all contents from the source folder:
/// - Extracts ZIP/RAR/7z archives
/// - Normalizes structures (unwraps Content wrappers, etc.)
/// - Merges DAZ content to destination
/// - Ignores promotional files (images, PDFs, etc.)
#[tauri::command]
pub async fn normalize_batch_cmd(
    app: AppHandle,
    source_path: String,
    destination_path: Option<String>,
    task_id: Option<String>,
    settings: State<'_, SettingsState>,
) -> Result<ApiResponse<NormalizeBatchResult>, String> {
    info!(
        "normalize_batch_cmd: source={}, dest={:?}",
        source_path, destination_path
    );

    let settings_snapshot = settings.read().map_err(|e| format!("Settings lock poisoned: {}", e))?.clone();
    let source = PathBuf::from(&source_path);
    let destination = destination_path.map(PathBuf::from);
    let event_task_id = task_id.clone().unwrap_or_else(|| "normalize".to_string());

    let result = spawn_blocking(move || {
        let dest_ref = destination.as_deref();

        normalize_and_merge_batch(&source, dest_ref, &settings_snapshot, |step: &str, detail: Option<&str>| {
            let event = NormalizeStepEvent {
                step: step.to_string(),
                detail: detail.map(String::from),
            };
            let event_name = format!("normalize-step-{}", event_task_id);
            let _ = app.emit(&event_name, &event);
        })
    })
    .await;

    match result {
        Ok(Ok(normalize_result)) => {
            info!(
                "Batch normalization complete: {} archives, {} folders normalized, {} merged",
                normalize_result.archives_extracted,
                normalize_result.folders_normalized,
                normalize_result.folders_merged
            );
            Ok(ApiResponse::success(normalize_result))
        }
        Ok(Err(app_error)) => {
            error!("Failed to normalize batch: {}", app_error);
            Ok(ApiResponse::error(app_error))
        }
        Err(join_error) => {
            error!("Task panicked: {}", join_error);
            Ok(ApiResponse::error(AppError::Internal(format!("Task error: {}", join_error))))
        }
    }
}

/// Processes multiple sources with robust error handling.
///
/// This command is designed for batch operations on large datasets.
/// Features:
/// - Automatic retry on transient failures
/// - Timeout protection per item
/// - Archive size validation
/// - Graceful error isolation (skip corrupted, continue processing)
/// - Progress reporting via events
///
/// # Events Emitted
/// - `batch-progress`: Progress updates during processing
///
/// # Arguments
/// * `paths` - List of archive/folder paths to process
/// * `task_id` - Optional task ID for event tracking
#[tauri::command]
pub async fn process_batch_robust(
    app: AppHandle,
    paths: Vec<String>,
    task_id: Option<String>,
    settings: tauri::State<'_, crate::config::SettingsState>,
) -> Result<ApiResponse<crate::core::extractor::BatchOperationResult>, String> {
    use crate::core::extractor::{BatchProgress, RobustBatchProcessor};

    let event_task_id = task_id.unwrap_or_else(|| {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("batch_{}", timestamp)
    });
    info!(
        "process_batch_robust: {} items, task_id={}",
        paths.len(),
        event_task_id
    );

    // Snapshot settings before entering spawn_blocking
    let settings_snapshot = match settings.read() {
        Ok(s) => s.clone(),
        Err(_) => return Ok(ApiResponse::error(AppError::Config("Settings lock poisoned".into()))),
    };

    // Convert string paths to PathBuf
    let source_paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();

    // Prepare checkpoint directory
    let checkpoint_dir = std::env::temp_dir()
        .join("FileManagerDaz")
        .join("checkpoints");

    let session_id = event_task_id.clone();

    let result = spawn_blocking(move || {
        let config = settings_snapshot.to_resilience_config();

        // Create processor with progress callback, checkpoint, and cleanup
        RobustBatchProcessor::new(config, settings_snapshot)
            .with_progress(move |progress: BatchProgress| {
                let event_name = format!("batch-progress-{}", event_task_id);
                let _ = app.emit(&event_name, &progress);
            })
            .with_checkpoint(checkpoint_dir, session_id)
            .with_cleanup(true)
            .process_batch(source_paths)
    })
    .await;

    match result {
        Ok(Ok(batch_result)) => {
            info!(
                "Batch processing complete: {}/{} successful, {} failed",
                batch_result.stats.successful,
                batch_result.stats.total_items,
                batch_result.stats.failed
            );
            Ok(ApiResponse::success(batch_result))
        }
        Ok(Err(app_error)) => {
            error!("Failed to process batch: {}", app_error);
            Ok(ApiResponse::error(app_error))
        }
        Err(join_error) => {
            error!("Task panicked: {}", join_error);
            Ok(ApiResponse::error(AppError::Internal(format!("Task error: {}", join_error))))
        }
    }
}

/// Cleanup extracted folders in temp directory
///
/// Removes all `*_extracted` folders left from previous crashes or interrupted operations.
///
/// # Returns
///
/// Number of folders cleaned up
#[tauri::command]
pub async fn cleanup_temp_extractions() -> ApiResponse<usize> {
    use crate::core::extractor::checkpoint::cleanup_extracted_folders;

    let temp_dir = std::env::temp_dir()
        .join("FileManagerDaz")
        .join("downloads_import");

    info!("Cleaning up temp extractions in {:?}", temp_dir);

    let result = spawn_blocking(move || cleanup_extracted_folders(&temp_dir)).await;

    match result {
        Ok(Ok(count)) => {
            info!("Cleanup complete: {} folders removed", count);
            ApiResponse::success(count)
        }
        Ok(Err(app_error)) => {
            error!("Failed to cleanup: {}", app_error);
            ApiResponse::error(app_error)
        }
        Err(join_error) => {
            error!("Task panicked: {}", join_error);
            ApiResponse::error(AppError::Internal(format!("Task error: {}", join_error)))
        }
    }
}

/// Get checkpoint status for a session
///
/// Returns information about a checkpoint including progress and failed items.
#[tauri::command]
pub async fn get_checkpoint_status(
    session_id: String,
) -> ApiResponse<Option<crate::core::extractor::checkpoint::Checkpoint>> {
    use crate::core::extractor::checkpoint::Checkpoint;

    let checkpoint_dir = std::env::temp_dir()
        .join("FileManagerDaz")
        .join("checkpoints");

    info!("Getting checkpoint status for session: {}", session_id);

    let result = spawn_blocking(
        move || match Checkpoint::load(&checkpoint_dir, &session_id) {
            Ok(checkpoint) => Ok(Some(checkpoint)),
            Err(AppError::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        },
    )
    .await;

    match result {
        Ok(Ok(checkpoint)) => ApiResponse::success(checkpoint),
        Ok(Err(app_error)) => {
            error!("Failed to load checkpoint: {}", app_error);
            ApiResponse::error(app_error)
        }
        Err(join_error) => {
            error!("Task panicked: {}", join_error);
            ApiResponse::error(AppError::Internal(format!("Task error: {}", join_error)))
        }
    }
}

// =============================================================================
// EXTENSION TRAITS
// =============================================================================

impl ExtractResult {
    /// Returns the detected DAZ content type (if analysis succeeded).
    pub fn content_type(&self) -> Option<&crate::core::analyzer::ContentType> {
        self.analysis.as_ref().map(|a| &a.content_type)
    }
}
