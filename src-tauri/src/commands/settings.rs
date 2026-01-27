//! Tauri commands for settings and DAZ libraries management

use crate::config::SETTINGS;
use crate::error::{ApiResponse, AppError};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{error, info, warn};

// ============================================================================
// Response types
// ============================================================================

/// Information about a DAZ library
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DazLibrary {
    pub path: String,
    pub name: String,
    pub exists: bool,
    pub is_default: bool,
}

/// Application config exposed to frontend
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub database_path: String,
    pub temp_dir: String,
    pub thumbnails_dir: String,
    pub daz_libraries: Vec<DazLibrary>,
    pub default_destination: Option<String>,
    pub can_extract_rar: bool,
    pub can_extract_7z: bool,
    pub unrar_path: Option<String>,
    pub sevenzip_path: Option<String>,
    pub trash_archives_after_import: bool,
    pub dev_log_extraction_timings: bool,
    pub dev_log_extraction_details: bool,
    /// UI language ("fr" or "en")
    pub language: String,
}

/// Libraries detection result
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectionResult {
    pub libraries: Vec<DazLibrary>,
    pub new_count: usize,
}

// ============================================================================
// Tauri commands
// ============================================================================

/// Gets the complete configuration
#[tauri::command]
pub fn get_app_config() -> ApiResponse<AppConfig> {
    info!("get_app_config");

    match SETTINGS.read() {
        Ok(settings) => {
            let libraries: Vec<DazLibrary> = settings
                .daz_libraries
                .iter()
                .map(|p| {
                    let is_default = settings.default_destination.as_ref() == Some(p);
                    DazLibrary {
                        path: p.to_string_lossy().to_string(),
                        name: p
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("Unknown")
                            .to_string(),
                        exists: p.exists(),
                        is_default,
                    }
                })
                .collect();

            let config = AppConfig {
                database_path: settings.database_path.to_string_lossy().to_string(),
                temp_dir: settings.temp_dir.to_string_lossy().to_string(),
                thumbnails_dir: settings.thumbnails_dir.to_string_lossy().to_string(),
                daz_libraries: libraries,
                default_destination: settings
                    .default_destination
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
                can_extract_rar: settings.can_extract_rar(),
                can_extract_7z: true, // sevenz-rust is always available
                unrar_path: settings
                    .unrar_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
                sevenzip_path: settings
                    .sevenzip_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
                trash_archives_after_import: settings.trash_archives_after_import,
                dev_log_extraction_timings: settings.dev_log_extraction_timings,
                dev_log_extraction_details: settings.dev_log_extraction_details,
                language: settings.language.clone(),
            };

            ApiResponse::success(config)
        }
        Err(e) => {
            error!("Failed to read settings: {}", e);
            ApiResponse::error(AppError::Config(format!("Failed to read config: {}", e)))
        }
    }
}

/// Lists configured DAZ libraries
#[tauri::command]
pub fn list_daz_libraries() -> ApiResponse<Vec<DazLibrary>> {
    info!("list_daz_libraries");

    match SETTINGS.read() {
        Ok(settings) => {
            let libraries: Vec<DazLibrary> = settings
                .daz_libraries
                .iter()
                .map(|p| {
                    let is_default = settings.default_destination.as_ref() == Some(p);
                    DazLibrary {
                        path: p.to_string_lossy().to_string(),
                        name: p
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("Unknown")
                            .to_string(),
                        exists: p.exists(),
                        is_default,
                    }
                })
                .collect();

            ApiResponse::success(libraries)
        }
        Err(e) => {
            error!("Failed to read settings: {}", e);
            ApiResponse::error(AppError::Config(format!("Failed to read libraries: {}", e)))
        }
    }
}

/// Automatically detects DAZ libraries (Windows registry + default paths)
#[tauri::command]
pub fn detect_daz_libraries() -> ApiResponse<DetectionResult> {
    info!("detect_daz_libraries");

    match SETTINGS.write() {
        Ok(mut settings) => {
            let before_count = settings.daz_libraries.len();

            // Reset and re-detect
            settings.daz_libraries.clear();
            settings.detect_daz_libraries();

            // Save
            if let Err(e) = settings.save() {
                warn!("Failed to save after detection: {}", e);
            }

            let libraries: Vec<DazLibrary> = settings
                .daz_libraries
                .iter()
                .map(|p| {
                    let is_default = settings.default_destination.as_ref() == Some(p);
                    DazLibrary {
                        path: p.to_string_lossy().to_string(),
                        name: p
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("Unknown")
                            .to_string(),
                        exists: p.exists(),
                        is_default,
                    }
                })
                .collect();

            let new_count = libraries.len().saturating_sub(before_count);
            info!("Detected {} libraries ({} new)", libraries.len(), new_count);

            ApiResponse::success(DetectionResult {
                libraries,
                new_count,
            })
        }
        Err(e) => {
            error!("Failed to write settings: {}", e);
            ApiResponse::error(AppError::Config(format!("Failed to write config: {}", e)))
        }
    }
}

/// Manually adds a DAZ library
#[tauri::command]
pub fn add_daz_library(path: String) -> ApiResponse<DazLibrary> {
    info!("add_daz_library: {}", path);

    let lib_path = PathBuf::from(&path);

    if !lib_path.exists() {
        return ApiResponse::error(AppError::NotFound(lib_path));
    }

    if !lib_path.is_dir() {
        return ApiResponse::error(AppError::InvalidPath("Path must be a folder".to_string()));
    }

    match SETTINGS.write() {
        Ok(mut settings) => {
            // Check if already present
            if settings.daz_libraries.contains(&lib_path) {
                return ApiResponse::error(AppError::Config(
                    "This library already exists".to_string(),
                ));
            }

            settings.daz_libraries.push(lib_path.clone());

            // If it's the first one, set it as default
            if settings.daz_libraries.len() == 1 {
                settings.default_destination = Some(lib_path.clone());
            }

            if let Err(e) = settings.save() {
                warn!("Failed to save: {}", e);
            }

            let is_default = settings.default_destination.as_ref() == Some(&lib_path);

            let library = DazLibrary {
                path: lib_path.to_string_lossy().to_string(),
                name: lib_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Unknown")
                    .to_string(),
                exists: true,
                is_default,
            };

            info!("Library added: {:?}", library);
            ApiResponse::success(library)
        }
        Err(e) => {
            error!("Failed to write settings: {}", e);
            ApiResponse::error(AppError::Config(format!("Failed to add: {}", e)))
        }
    }
}

/// Removes a DAZ library
#[tauri::command]
pub fn remove_daz_library(path: String) -> ApiResponse<bool> {
    info!("remove_daz_library: {}", path);

    let lib_path = PathBuf::from(&path);

    match SETTINGS.write() {
        Ok(mut settings) => {
            let initial_len = settings.daz_libraries.len();
            settings.daz_libraries.retain(|p| p != &lib_path);

            if settings.daz_libraries.len() == initial_len {
                return ApiResponse::error(AppError::NotFound(lib_path));
            }

            // If it was the default, update it
            if settings.default_destination.as_ref() == Some(&lib_path) {
                settings.default_destination = settings.daz_libraries.first().cloned();
            }

            if let Err(e) = settings.save() {
                warn!("Failed to save: {}", e);
            }

            info!("Library removed: {}", path);
            ApiResponse::success(true)
        }
        Err(e) => {
            error!("Failed to write settings: {}", e);
            ApiResponse::error(AppError::Config(format!("Failed to remove: {}", e)))
        }
    }
}

/// Sets the default library
#[tauri::command]
pub fn set_default_library(path: String) -> ApiResponse<bool> {
    info!("set_default_library: {}", path);

    let lib_path = PathBuf::from(&path);

    match SETTINGS.write() {
        Ok(mut settings) => {
            if !settings.daz_libraries.contains(&lib_path) {
                return ApiResponse::error(AppError::NotFound(lib_path));
            }

            settings.default_destination = Some(lib_path);

            if let Err(e) = settings.save() {
                warn!("Failed to save: {}", e);
            }

            ApiResponse::success(true)
        }
        Err(e) => {
            error!("Failed to write settings: {}", e);
            ApiResponse::error(AppError::Config(format!("Failed to set: {}", e)))
        }
    }
}

/// Updates the temporary folder path
#[tauri::command]
pub fn set_temp_dir(path: String) -> ApiResponse<bool> {
    info!("set_temp_dir: {}", path);

    let dir_path = PathBuf::from(&path);

    // Create folder if it doesn't exist
    if !dir_path.exists() {
        if let Err(e) = std::fs::create_dir_all(&dir_path) {
            return ApiResponse::error(AppError::Io(e));
        }
    }

    match SETTINGS.write() {
        Ok(mut settings) => {
            settings.temp_dir = dir_path;

            if let Err(e) = settings.save() {
                warn!("Failed to save: {}", e);
            }

            ApiResponse::success(true)
        }
        Err(e) => {
            error!("Failed to write settings: {}", e);
            ApiResponse::error(AppError::Config(format!("Failed to modify: {}", e)))
        }
    }
}

/// Sets whether archives should be moved to trash after a successful import
#[tauri::command]
pub fn set_trash_archives_after_import(enabled: bool) -> ApiResponse<bool> {
    info!("set_trash_archives_after_import: {}", enabled);

    match SETTINGS.write() {
        Ok(mut settings) => {
            settings.trash_archives_after_import = enabled;

            if let Err(e) = settings.save() {
                warn!("Failed to save: {}", e);
            }

            ApiResponse::success(enabled)
        }
        Err(e) => {
            error!("Failed to write settings: {}", e);
            ApiResponse::error(AppError::Config(format!("Failed to modify: {}", e)))
        }
    }
}

/// Enables/disables extraction timing logs (developer option)
#[tauri::command]
pub fn set_dev_log_extraction_timings(enabled: bool) -> ApiResponse<bool> {
    info!("set_dev_log_extraction_timings: {}", enabled);

    match SETTINGS.write() {
        Ok(mut settings) => {
            settings.dev_log_extraction_timings = enabled;

            if let Err(e) = settings.save() {
                warn!("Failed to save: {}", e);
            }

            ApiResponse::success(enabled)
        }
        Err(e) => {
            error!("Failed to write settings: {}", e);
            ApiResponse::error(AppError::Config(format!("Failed to modify: {}", e)))
        }
    }
}

/// Enables/disables extraction move detail logs (developer option)
#[tauri::command]
pub fn set_dev_log_extraction_details(enabled: bool) -> ApiResponse<bool> {
    info!("set_dev_log_extraction_details: {}", enabled);

    match SETTINGS.write() {
        Ok(mut settings) => {
            settings.dev_log_extraction_details = enabled;

            if let Err(e) = settings.save() {
                warn!("Failed to save settings: {}", e);
            }

            ApiResponse::success(enabled)
        }
        Err(e) => {
            error!("Failed to write settings: {}", e);
            ApiResponse::error(AppError::Config(format!("Failed to modify: {}", e)))
        }
    }
}

/// Re-detect external tools (UnRAR, 7-Zip)
#[tauri::command]
pub fn detect_external_tools() -> ApiResponse<AppConfig> {
    info!("detect_external_tools");

    match SETTINGS.write() {
        Ok(mut settings) => {
            settings.detect_external_tools();

            if let Err(e) = settings.save() {
                warn!("Failed to save settings: {}", e);
            }

            drop(settings);

            // Return updated config
            get_app_config()
        }
        Err(e) => {
            error!("Failed to write settings: {}", e);
            ApiResponse::error(AppError::Config(format!("Failed to detect tools: {}", e)))
        }
    }
}

/// Set the UI language ("fr" or "en")
#[tauri::command]
pub fn set_language(language: String) -> ApiResponse<String> {
    info!("set_language: {}", language);

    // Validate language
    if language != "fr" && language != "en" {
        return ApiResponse::error(AppError::Config(format!(
            "Unsupported language: {}. Supported: fr, en",
            language
        )));
    }

    match SETTINGS.write() {
        Ok(mut settings) => {
            settings.language = language.clone();

            if let Err(e) = settings.save() {
                warn!("Failed to save settings: {}", e);
            }

            ApiResponse::success(language)
        }
        Err(e) => {
            error!("Failed to write settings: {}", e);
            ApiResponse::error(AppError::Config(format!("Failed to set language: {}", e)))
        }
    }
}
