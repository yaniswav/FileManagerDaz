//! # Error Handling Module
//!
//! Centralized error handling for FileManagerDaz.
//!
//! ## Overview
//!
//! This module provides:
//! - [`AppError`]: The main error enum covering all failure cases
//! - [`ApiResponse<T>`]: A structured response wrapper for Tauri commands
//! - [`ApiError`]: Error details with code, message, and optional details
//!
//! ## Usage Pattern
//!
//! Tauri commands should return `ApiResponse<T>` to provide consistent
//! error handling to the frontend:
//!
//! ```ignore
//! #[tauri::command]
//! pub fn my_command() -> ApiResponse<MyData> {
//!     my_fallible_function().into()
//! }
//! ```
//!
//! The frontend can then check `response.ok` and handle `response.error`
//! with structured error codes for i18n-aware error messages.

use serde::Serialize;
use std::path::PathBuf;
use thiserror::Error;

/// Main application error type.
///
/// All recoverable errors in the application are represented by this enum.
/// Each variant maps to an `ApiError` code for frontend consumption.
#[derive(Error, Debug)]
pub enum AppError {
    // === Filesystem Errors ===
    /// File or directory does not exist
    #[error("File or folder not found: {0}")]
    NotFound(PathBuf),

    /// Generic I/O error (read, write, seek, etc.)
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    // === Archive Extraction Errors ===
    /// Archive format not supported (neither ZIP, RAR, nor 7z)
    #[error("Unsupported archive format: {0}")]
    UnsupportedFormat(String),

    /// Archive file is corrupted or malformed
    #[error("Corrupted or invalid archive: {0}")]
    InvalidArchive(String),

    /// Error during ZIP extraction (via `zip` crate)
    #[error("ZIP extraction error: {0}")]
    ZipError(String),

    /// Error during 7z extraction (via `sevenz-rust` crate)
    #[error("7z extraction error: {0}")]
    SevenZipError(String),

    /// Error during RAR extraction (via external `unrar.exe`)
    #[error("RAR extraction error: {0}")]
    RarError(String),

    /// Required external tool (e.g., unrar.exe) not found
    #[error("External tool not found: {0}")]
    ExternalToolNotFound(String),

    // === Database Errors ===
    /// SQLite database operation failed
    #[error("Database error: {0}")]
    Database(String),

    // === Configuration Errors ===
    /// Application configuration is invalid or missing
    #[error("Configuration error: {0}")]
    Config(String),

    /// Provided path is invalid (e.g., contains illegal characters)
    #[error("Invalid path: {0}")]
    InvalidPath(String),

    // === Generic Errors ===
    /// Unexpected internal error (bug or unhandled case)
    #[error("Internal error: {0}")]
    Internal(String),
}

// === Error Conversions ===
// These `From` implementations allow using `?` operator with external error types.

impl From<rusqlite::Error> for AppError {
    fn from(err: rusqlite::Error) -> Self {
        AppError::Database(err.to_string())
    }
}

impl From<zip::result::ZipError> for AppError {
    fn from(err: zip::result::ZipError) -> Self {
        AppError::ZipError(err.to_string())
    }
}

impl From<walkdir::Error> for AppError {
    fn from(err: walkdir::Error) -> Self {
        AppError::Io(err.into())
    }
}

/// Convenient result type alias for application functions.
pub type AppResult<T> = Result<T, AppError>;

// =============================================================================
// API RESPONSE TYPES
// =============================================================================
//
// These types provide structured responses for Tauri commands.
// The frontend can rely on consistent error codes for i18n and error handling.
// =============================================================================

/// Structured error details for API responses.
///
/// Provides machine-readable error codes alongside human-readable messages,
/// enabling the frontend to display localized error messages.
#[derive(Debug, Clone, Serialize)]
pub struct ApiError {
    /// Machine-readable error code (e.g., "NOT_FOUND", "IO_ERROR").
    /// Used by frontend for i18n lookup.
    pub code: String,

    /// Human-readable error message (English).
    pub message: String,

    /// Additional context (e.g., file path, product ID).
    pub details: Option<String>,
}

/// Converts an `AppError` to an `ApiError` with appropriate error codes.
impl From<&AppError> for ApiError {
    fn from(err: &AppError) -> Self {
        let (code, details) = match err {
            // Filesystem errors
            AppError::NotFound(path) => ("NOT_FOUND", Some(path.display().to_string())),
            AppError::Io(_) => ("IO_ERROR", None),

            // Archive errors
            AppError::UnsupportedFormat(format) => ("UNSUPPORTED_FORMAT", Some(format.clone())),
            AppError::InvalidArchive(msg) => ("INVALID_ARCHIVE", Some(msg.clone())),
            AppError::ZipError(msg) => ("ZIP_ERROR", Some(msg.clone())),
            AppError::SevenZipError(msg) => ("SEVENZ_ERROR", Some(msg.clone())),
            AppError::RarError(msg) => ("RAR_ERROR", Some(msg.clone())),
            AppError::ExternalToolNotFound(tool) => ("TOOL_NOT_FOUND", Some(tool.clone())),

            // Database errors
            AppError::Database(msg) => ("DATABASE_ERROR", Some(msg.clone())),

            // Configuration errors
            AppError::Config(msg) => ("CONFIG_ERROR", Some(msg.clone())),
            AppError::InvalidPath(path) => ("INVALID_PATH", Some(path.clone())),

            // Generic errors
            AppError::Internal(msg) => ("INTERNAL_ERROR", Some(msg.clone())),
        };

        ApiError {
            code: code.to_string(),
            message: err.to_string(),
            details,
        }
    }
}

/// Generic API response wrapper for Tauri commands.
///
/// Provides a consistent structure for all command responses:
/// - `ok: true` with `data` on success
/// - `ok: false` with `error` on failure
///
/// This allows the frontend to handle all responses uniformly.
#[derive(Debug, Clone, Serialize)]
pub struct ApiResponse<T: Serialize> {
    /// Whether the operation succeeded.
    pub ok: bool,

    /// The response data (present only when `ok` is true).
    pub data: Option<T>,

    /// Error details (present only when `ok` is false).
    pub error: Option<ApiError>,
}

impl<T: Serialize> ApiResponse<T> {
    /// Creates a successful response with the given data.
    pub fn success(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    /// Creates an error response from an `AppError`.
    pub fn error(err: AppError) -> Self {
        tracing::error!("API error: {:?}", err);
        Self {
            ok: false,
            data: None,
            error: Some(ApiError::from(&err)),
        }
    }

    /// Creates an error response with a custom code and message.
    ///
    /// Use this for errors that don't fit into `AppError` variants.
    pub fn error_msg(code: &str, message: &str) -> Self {
        tracing::error!("API error [{}]: {}", code, message);
        Self {
            ok: false,
            data: None,
            error: Some(ApiError {
                code: code.to_string(),
                message: message.to_string(),
                details: None,
            }),
        }
    }
}

/// Enables using `.into()` to convert `AppResult<T>` to `ApiResponse<T>`.
///
/// This makes command implementations concise:
/// ```ignore
/// #[tauri::command]
/// pub fn my_cmd() -> ApiResponse<Data> {
///     my_function().into()
/// }
/// ```
impl<T: Serialize> From<AppResult<T>> for ApiResponse<T> {
    fn from(result: AppResult<T>) -> Self {
        match result {
            Ok(data) => ApiResponse::success(data),
            Err(err) => ApiResponse::error(err),
        }
    }
}
