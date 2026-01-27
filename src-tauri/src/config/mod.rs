//! # Configuration Module
//!
//! Centralized configuration management for FileManagerDaz.
//!
//! ## Overview
//!
//! The [`SETTINGS`] global provides thread-safe access to:
//! - Application data directories (database, temp files)
//! - DAZ library paths
//! - External tool paths (unrar.exe)
//! - User preferences (trash after import, language, etc.)
//!
//! ## Persistence
//!
//! Settings are persisted to `settings.json` in the app data directory
//! and loaded on startup.

pub mod settings;

pub use settings::SETTINGS;
