//! Tauri commands for folder watching

use crate::core::auto_import::{validate_archive, ArchiveValidation};
use crate::core::watcher::{WatchEvent, WatchEventType, WatcherState};
use crate::error::ApiResponse;
use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;
use tauri::State;
use tracing::info;

/// Serializable event for the frontend
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchEventDto {
    pub path: String,
    pub event_type: String,
    pub file_name: String,
}

impl From<WatchEvent> for WatchEventDto {
    fn from(event: WatchEvent) -> Self {
        Self {
            file_name: event
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string(),
            path: event.path.to_string_lossy().to_string(),
            event_type: match event.event_type {
                WatchEventType::Created => "created".to_string(),
                WatchEventType::Modified => "modified".to_string(),
                WatchEventType::Removed => "removed".to_string(),
            },
        }
    }
}

/// Watcher state info
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatcherInfo {
    pub is_watching: bool,
    pub watch_path: Option<String>,
}

/// Tauri state for watcher (reuses State pattern)
pub struct TauriWatcherState(pub Mutex<WatcherState>);

impl TauriWatcherState {
    pub fn new() -> Self {
        Self(Mutex::new(WatcherState::new()))
    }
}

/// Starts watching a folder
#[tauri::command]
pub async fn start_watching(
    state: State<'_, TauriWatcherState>,
    path: String,
) -> Result<ApiResponse<bool>, String> {
    info!("Starting watch on: {}", path);

    let watcher = state.0.lock().map_err(|_| "Lock error".to_string())?;

    match watcher.start(Path::new(&path)) {
        Ok(()) => Ok(ApiResponse::success(true)),
        Err(e) => Ok(ApiResponse::error_msg("WATCH_ERROR", &e)),
    }
}

/// Stops watching
#[tauri::command]
pub async fn stop_watching(
    state: State<'_, TauriWatcherState>,
) -> Result<ApiResponse<bool>, String> {
    info!("Stopping watch");

    let watcher = state.0.lock().map_err(|_| "Lock error".to_string())?;
    watcher.stop();

    Ok(ApiResponse::success(true))
}

/// Checks watcher status
#[tauri::command]
pub async fn get_watcher_info(
    state: State<'_, TauriWatcherState>,
) -> Result<ApiResponse<WatcherInfo>, String> {
    let watcher = state.0.lock().map_err(|_| "Lock error".to_string())?;

    Ok(ApiResponse::success(WatcherInfo {
        is_watching: watcher.is_watching(),
        watch_path: watcher
            .get_watch_path()
            .map(|p| p.to_string_lossy().to_string()),
    }))
}

/// Gets pending events
#[tauri::command]
pub async fn poll_watch_events(
    state: State<'_, TauriWatcherState>,
) -> Result<ApiResponse<Vec<WatchEventDto>>, String> {
    let watcher = state.0.lock().map_err(|_| "Lock error".to_string())?;

    let events: Vec<WatchEventDto> = watcher
        .poll_events()
        .into_iter()
        .map(WatchEventDto::from)
        .collect();

    Ok(ApiResponse::success(events))
}

/// Scans existing archives in the watched folder
#[tauri::command]
pub async fn scan_watched_folder(
    state: State<'_, TauriWatcherState>,
) -> Result<ApiResponse<Vec<String>>, String> {
    let watcher = state.0.lock().map_err(|_| "Lock error".to_string())?;

    let paths: Vec<String> = watcher
        .scan_existing()
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    Ok(ApiResponse::success(paths))
}

/// Gets the user's Downloads folder path
#[tauri::command]
pub async fn get_downloads_folder() -> Result<ApiResponse<Option<String>>, String> {
    let path = directories::UserDirs::new()
        .and_then(|dirs| dirs.download_dir().map(|p| p.to_path_buf()))
        .map(|p| p.to_string_lossy().to_string());

    Ok(ApiResponse::success(path))
}

/// Validates whether a file is a DAZ archive
#[tauri::command]
pub async fn validate_daz_archive(path: String) -> Result<ApiResponse<ArchiveValidation>, String> {
    info!("validate_daz_archive: {}", path);
    let result = validate_archive(Path::new(&path));
    Ok(ApiResponse::success(result))
}
