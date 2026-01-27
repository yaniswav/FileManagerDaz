//! # Application Settings
//!
//! Global application configuration with JSON persistence.
//!
//! ## Usage
//!
//! Access settings via the global [`SETTINGS`] instance:
//! ```ignore
//! use crate::config::SETTINGS;
//!
//! // Read settings
//! if let Ok(settings) = SETTINGS.read() {
//!     println!("Temp dir: {:?}", settings.temp_dir);
//! }
//!
//! // Write settings
//! if let Ok(mut settings) = SETTINGS.write() {
//!     settings.trash_archives_after_import = true;
//!     settings.save().ok();
//! }
//! ```
//!
//! ## Configuration File
//!
//! Settings are persisted to `settings.json` in the app data directory:
//! - Windows: `%LOCALAPPDATA%\FileManagerDaz\settings.json`

use crate::error::{AppError, AppResult};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;
use tracing::{debug, info, warn};

// =============================================================================
// GLOBAL SETTINGS INSTANCE
// =============================================================================

/// Global settings instance, lazily initialized on first access.
pub static SETTINGS: Lazy<RwLock<AppSettings>> =
    Lazy::new(|| RwLock::new(AppSettings::load_or_default()));

// =============================================================================
// SETTINGS STRUCT
// =============================================================================

/// Application-wide configuration settings.
///
/// This struct is serialized to JSON for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// Application data directory (databases, config, etc.).
    pub app_data_dir: PathBuf,

    /// Temporary directory for archive extraction.
    pub temp_dir: PathBuf,

    /// Directory for product thumbnails.
    pub thumbnails_dir: PathBuf,

    /// Path to the products SQLite database.
    pub database_path: PathBuf,

    /// List of detected/configured DAZ library paths.
    pub daz_libraries: Vec<PathBuf>,

    /// Default destination library for imports.
    pub default_destination: Option<PathBuf>,

    /// Path to unrar.exe (required for RAR extraction).
    pub unrar_path: Option<PathBuf>,

    /// Path to 7z.exe (optional, for external 7z fallback).
    pub sevenzip_path: Option<PathBuf>,

    /// If true, move source archives to trash after successful import.
    #[serde(default)]
    pub trash_archives_after_import: bool,

    /// Developer mode: log detailed extraction timings to file.
    #[serde(default)]
    pub dev_log_extraction_timings: bool,
    /// Developer mode: log detailed extraction move report to file.
    #[serde(default)]
    pub dev_log_extraction_details: bool,

    /// UI language code ("en" or "fr").
    #[serde(default = "default_language")]
    pub language: String,

    /// Maximum retry attempts for failed extractions.
    #[serde(default = "default_max_retries")]
    pub max_extraction_retries: u32,

    /// Timeout in seconds for a single extraction operation (0 = no timeout).
    #[serde(default = "default_extraction_timeout")]
    pub extraction_timeout_seconds: u64,

    /// Maximum archive size in GB (0 = unlimited).
    #[serde(default)]
    pub max_archive_size_gb: u64,

    /// Skip corrupted archives in batch operations instead of stopping.
    #[serde(default = "default_skip_corrupted")]
    pub skip_corrupted_archives: bool,
}

/// Default UI language.
fn default_language() -> String {
    "en".to_string()
}

/// Default max retries.
fn default_max_retries() -> u32 {
    3
}

/// Default extraction timeout (1 hour).
fn default_extraction_timeout() -> u64 {
    3600
}

/// Default skip corrupted flag.
fn default_skip_corrupted() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        let app_data = get_app_data_dir();

        Self {
            app_data_dir: app_data.clone(),
            temp_dir: std::env::temp_dir().join("FileManagerDaz"),
            thumbnails_dir: app_data.join("thumbnails"),
            database_path: app_data.join("database.db"),
            daz_libraries: Vec::new(),
            default_destination: None,
            unrar_path: None,
            sevenzip_path: None,
            trash_archives_after_import: false,
            dev_log_extraction_timings: false,
            dev_log_extraction_details: false,
            language: default_language(),
            max_extraction_retries: default_max_retries(),
            extraction_timeout_seconds: default_extraction_timeout(),
            max_archive_size_gb: 0,
            skip_corrupted_archives: default_skip_corrupted(),
        }
    }
}

impl AppSettings {
    /// Loads settings from file or returns default values
    pub fn load_or_default() -> Self {
        let config_path = get_config_file_path();

        if config_path.exists() {
            match fs::read_to_string(&config_path) {
                Ok(content) => match serde_json::from_str::<AppSettings>(&content) {
                    Ok(settings) => {
                        info!("Configuration loaded from {:?}", config_path);
                        return settings;
                    }
                    Err(e) => {
                        warn!("Config parsing error: {}", e);
                    }
                },
                Err(e) => {
                    warn!("Cannot read config file: {}", e);
                }
            }
        }

        let mut settings = Self::default();
        settings.detect_external_tools();
        settings.detect_daz_libraries();

        // Save default config
        if let Err(e) = settings.save() {
            warn!("Cannot save default config: {}", e);
        }

        settings
    }

    /// Saves settings to configuration file
    pub fn save(&self) -> AppResult<()> {
        let config_path = get_config_file_path();

        // Create parent directory if needed
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| AppError::Config(format!("Serialization failed: {}", e)))?;

        fs::write(&config_path, content)?;
        info!("Configuration saved to {:?}", config_path);

        Ok(())
    }

    /// Detects external tools (unrar, 7z)
    pub fn detect_external_tools(&mut self) {
        // Look for unrar
        if let Ok(path) = which::which("unrar") {
            info!("UnRAR found: {:?}", path);
            self.unrar_path = Some(path);
        } else {
            // Look in common Windows locations
            let common_paths = [
                r"C:\Program Files\WinRAR\UnRAR.exe",
                r"C:\Program Files (x86)\WinRAR\UnRAR.exe",
            ];
            for p in common_paths {
                let path = PathBuf::from(p);
                if path.exists() {
                    info!("UnRAR found: {:?}", path);
                    self.unrar_path = Some(path);
                    break;
                }
            }
        }

        // Look for 7z
        if let Ok(path) = which::which("7z") {
            info!("7-Zip found: {:?}", path);
            self.sevenzip_path = Some(path);
        } else {
            let common_paths = [
                r"C:\Program Files\7-Zip\7z.exe",
                r"C:\Program Files (x86)\7-Zip\7z.exe",
            ];
            for p in common_paths {
                let path = PathBuf::from(p);
                if path.exists() {
                    info!("7-Zip found: {:?}", path);
                    self.sevenzip_path = Some(path);
                    break;
                }
            }
        }
    }

    /// Detects DAZ libraries from Windows registry
    pub fn detect_daz_libraries(&mut self) {
        #[cfg(windows)]
        {
            use winreg::enums::*;
            use winreg::RegKey;

            let hklm = RegKey::predef(HKEY_CURRENT_USER);

            // Look in DAZ Studio registry keys
            let paths_to_try = [
                r"SOFTWARE\DAZ\Studio4\ContentDirectorySetups",
                r"SOFTWARE\DAZ\Studio4",
            ];

            for reg_path in paths_to_try {
                if let Ok(key) = hklm.open_subkey(reg_path) {
                    // Iterate over values
                    for (name, value) in key.enum_values().filter_map(|r| r.ok()) {
                        if name.contains("ContentDir") || name.contains("Library") {
                            let path_str = value.to_string();
                            let path = PathBuf::from(&path_str);
                            if path.exists() && !self.daz_libraries.contains(&path) {
                                debug!("DAZ library found: {:?}", path);
                                self.daz_libraries.push(path);
                            }
                        }
                    }
                }
            }
        }

        // Add default paths if they exist
        let default_paths = [
            dirs::document_dir().map(|d| d.join("DAZ 3D").join("Studio").join("My Library")),
            dirs::public_dir().map(|d| d.join("Documents").join("My DAZ 3D Library")),
        ];

        for path_opt in default_paths.into_iter().flatten() {
            if path_opt.exists() && !self.daz_libraries.contains(&path_opt) {
                debug!("Default DAZ library found: {:?}", path_opt);
                self.daz_libraries.push(path_opt);
            }
        }
    }

    /// Returns the temp directory, creating it if needed
    pub fn get_temp_dir(&self) -> AppResult<PathBuf> {
        if !self.temp_dir.exists() {
            fs::create_dir_all(&self.temp_dir)?;
        }
        Ok(self.temp_dir.clone())
    }

    /// Returns a unique temp subdirectory for an extraction
    pub fn get_extraction_dir(&self, name: &str) -> AppResult<PathBuf> {
        let base = self.get_temp_dir()?;
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let dir = base.join(format!("{}_{}", name, timestamp));
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Checks if RAR extraction is available
    pub fn can_extract_rar(&self) -> bool {
        self.unrar_path.is_some()
    }

    /// Checks if external 7z extraction is available
    #[allow(dead_code)]
    pub fn can_extract_7z_external(&self) -> bool {
        self.sevenzip_path.is_some()
    }

    /// Creates a resilience configuration from current settings
    pub fn to_resilience_config(&self) -> crate::core::extractor::resilience::ResilienceConfig {
        use crate::core::extractor::resilience::ResilienceConfig;
        use std::time::Duration;

        ResilienceConfig {
            max_retries: self.max_extraction_retries,
            base_retry_delay: Duration::from_secs(2),
            extraction_timeout: if self.extraction_timeout_seconds > 0 {
                Some(Duration::from_secs(self.extraction_timeout_seconds))
            } else {
                None
            },
            max_archive_size: if self.max_archive_size_gb > 0 {
                Some(self.max_archive_size_gb * 1024 * 1024 * 1024)
            } else {
                None
            },
            skip_corrupted: self.skip_corrupted_archives,
        }
    }
}

/// Returns the application data directory
fn get_app_data_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "filemanagerdaz", "FileManagerDaz")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| {
            // Fallback: current directory
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        })
}

/// Returns the configuration file path
fn get_config_file_path() -> PathBuf {
    get_app_data_dir().join("settings.json")
}

// Dirs module for standard paths (Windows)
mod dirs {
    use std::path::PathBuf;

    pub fn document_dir() -> Option<PathBuf> {
        directories::UserDirs::new().and_then(|u| u.document_dir().map(|p| p.to_path_buf()))
    }

    pub fn public_dir() -> Option<PathBuf> {
        #[cfg(windows)]
        {
            std::env::var("PUBLIC").ok().map(PathBuf::from)
        }
        #[cfg(not(windows))]
        {
            None
        }
    }
}
