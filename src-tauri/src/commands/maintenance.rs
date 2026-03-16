//! Tauri commands for library maintenance
//!
//! Uses `spawn_blocking` for scan operations that can be long-running.

use crate::config::SettingsState;
use crate::core::maintenance::{
    cleanup_empty_folders, cleanup_files, cleanup_library_complete, scan_all_libraries,
    scan_library, CleanupResult, MaintenanceSummary, ScanOptions,
};
use crate::error::{ApiResponse, AppError};
use std::path::Path;
use tauri::State;
use tokio::task::spawn_blocking;
use tracing::{error, info};

/// Serializable scan options from the frontend
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanOptionsDto {
    #[serde(default = "default_true")]
    pub detect_duplicates: bool,
    #[serde(default)]
    pub detect_similar_names: bool,
    #[serde(default = "default_true")]
    pub detect_orphans: bool,
    #[serde(default = "default_true")]
    pub detect_empty_folders: bool,
    #[serde(default = "default_true")]
    pub detect_temp_files: bool,
    #[serde(default)]
    pub ignore_extensions: Vec<String>,
    #[serde(default = "default_min_size")]
    pub min_size_for_hash: u64,
}

fn default_true() -> bool {
    true
}
fn default_min_size() -> u64 {
    1024
}

impl From<ScanOptionsDto> for ScanOptions {
    fn from(dto: ScanOptionsDto) -> Self {
        Self {
            detect_duplicates: dto.detect_duplicates,
            detect_similar_names: dto.detect_similar_names,
            detect_orphans: dto.detect_orphans,
            detect_empty_folders: dto.detect_empty_folders,
            detect_temp_files: dto.detect_temp_files,
            ignore_extensions: if dto.ignore_extensions.is_empty() {
                vec![
                    ".db".to_string(),
                    ".db-journal".to_string(),
                    ".log".to_string(),
                ]
            } else {
                dto.ignore_extensions
            },
            min_size_for_hash: dto.min_size_for_hash,
        }
    }
}

/// Scans a specific library for maintenance issues
/// Executed on a blocking thread as the scan can be long-running.
#[tauri::command]
pub async fn scan_library_cmd(
    library_path: String,
    options: Option<ScanOptionsDto>,
) -> ApiResponse<MaintenanceSummary> {
    info!("Scanning library: {}", library_path);

    let opts = options.map(ScanOptions::from).unwrap_or_default();

    let result = spawn_blocking(move || scan_library(Path::new(&library_path), &opts)).await;

    match result {
        Ok(Ok(summary)) => ApiResponse::success(summary),
        Ok(Err(e)) => {
            error!("Failed to scan library: {}", e);
            ApiResponse::error(e)
        }
        Err(e) => {
            error!("Scan task panicked: {}", e);
            ApiResponse::error(AppError::Internal(format!("Task error: {}", e)))
        }
    }
}

/// Scans all configured libraries
/// Executed on a blocking thread.
#[tauri::command]
pub async fn scan_all_libraries_cmd(
    options: Option<ScanOptionsDto>,
    settings: State<'_, SettingsState>,
) -> Result<ApiResponse<MaintenanceSummary>, String> {
    info!("Scanning all libraries");

    let settings_snapshot = settings.read().map_err(|e| format!("Settings lock poisoned: {}", e))?.clone();
    let opts = options.map(ScanOptions::from).unwrap_or_default();

    let result = spawn_blocking(move || scan_all_libraries(&opts, &settings_snapshot)).await;

    match result {
        Ok(Ok(summary)) => Ok(ApiResponse::success(summary)),
        Ok(Err(e)) => {
            error!("Failed to scan libraries: {}", e);
            Ok(ApiResponse::error(e))
        }
        Err(e) => {
            error!("Scan task panicked: {}", e);
            Ok(ApiResponse::error(AppError::Internal(format!("Task error: {}", e))))
        }
    }
}

/// Deletes selected files
/// Executed on a blocking thread.
#[tauri::command]
pub async fn cleanup_files_cmd(
    files: Vec<String>,
    backup: bool,
    backup_dir: Option<String>,
    settings: State<'_, SettingsState>,
) -> Result<ApiResponse<CleanupResult>, String> {
    info!("Cleaning up {} files (backup: {})", files.len(), backup);

    let settings_snapshot = settings.read().map_err(|e| format!("Settings lock poisoned: {}", e))?.clone();

    let result = spawn_blocking(move || {
        let backup_path = backup_dir.as_ref().map(|s| Path::new(s.as_str()));
        cleanup_files(&files, backup, backup_path, &settings_snapshot)
    })
    .await;

    match result {
        Ok(Ok(cleanup_result)) => Ok(ApiResponse::success(cleanup_result)),
        Ok(Err(e)) => {
            error!("Failed to cleanup files: {}", e);
            Ok(ApiResponse::error(e))
        }
        Err(e) => {
            error!("Cleanup task panicked: {}", e);
            Ok(ApiResponse::error(AppError::Internal(format!("Task error: {}", e))))
        }
    }
}

/// Deletes empty folders from a library
/// Executed on a blocking thread.
#[tauri::command]
pub async fn cleanup_empty_folders_cmd(library_path: String) -> ApiResponse<CleanupResult> {
    info!("Cleaning empty folders in: {}", library_path);

    let result = spawn_blocking(move || cleanup_empty_folders(Path::new(&library_path))).await;

    match result {
        Ok(Ok(cleanup_result)) => ApiResponse::success(cleanup_result),
        Ok(Err(e)) => {
            error!("Failed to cleanup empty folders: {}", e);
            ApiResponse::error(e)
        }
        Err(e) => {
            error!("Cleanup task panicked: {}", e);
            ApiResponse::error(AppError::Internal(format!("Task error: {}", e)))
        }
    }
}

/// Quick scan to get an overview of issues
/// Executed on a blocking thread.
#[tauri::command]
pub async fn quick_maintenance_scan_cmd(
    settings: State<'_, SettingsState>,
) -> Result<ApiResponse<MaintenanceSummary>, String> {
    info!("Quick maintenance scan");

    let settings_snapshot = settings.read().map_err(|e| format!("Settings lock poisoned: {}", e))?.clone();

    let result = spawn_blocking(move || {
        let opts = ScanOptions {
            detect_duplicates: false, // Disabled for quick scan (expensive)
            detect_similar_names: false,
            detect_orphans: false,
            detect_empty_folders: true,
            detect_temp_files: true,
            ignore_extensions: vec![".db".to_string()],
            min_size_for_hash: 1024 * 1024, // 1 MB
        };

        scan_all_libraries(&opts, &settings_snapshot)
    })
    .await;

    match result {
        Ok(Ok(summary)) => Ok(ApiResponse::success(summary)),
        Ok(Err(e)) => {
            error!("Failed to quick scan: {}", e);
            Ok(ApiResponse::error(e))
        }
        Err(e) => {
            error!("Quick scan task panicked: {}", e);
            Ok(ApiResponse::error(AppError::Internal(format!("Task error: {}", e))))
        }
    }
}

/// Complete library cleanup: removes unwanted files and empty folders
///
/// This is a more aggressive cleanup that removes:
/// - Promo/marketing files
/// - Documentation files (README, LICENSE, etc.)
/// - Temporary files
/// - Empty folders
///
/// Files inside DAZ standard folders are preserved.
#[tauri::command]
pub async fn cleanup_library_complete_cmd(library_path: String) -> ApiResponse<CleanupResult> {
    info!("Starting complete cleanup of library: {}", library_path);

    let result = spawn_blocking(move || cleanup_library_complete(Path::new(&library_path))).await;

    match result {
        Ok(Ok(cleanup_result)) => {
            info!(
                "Complete cleanup finished: {} files, {} folders deleted",
                cleanup_result.files_deleted, cleanup_result.folders_deleted
            );
            ApiResponse::success(cleanup_result)
        }
        Ok(Err(e)) => {
            error!("Failed to cleanup library: {}", e);
            ApiResponse::error(e)
        }
        Err(e) => {
            error!("Cleanup task panicked: {}", e);
            ApiResponse::error(AppError::Internal(format!("Task error: {}", e)))
        }
    }
}
