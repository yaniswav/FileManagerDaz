//! # FileManagerDaz Library
//!
//! Core library for the FileManagerDaz Tauri application.
//!
//! ## Modules
//!
//! - [`commands`]: Tauri command handlers (exposed to frontend via IPC)
//! - [`config`]: Application settings and configuration persistence
//! - [`core`]: Business logic (extraction, analysis, destination resolution)
//! - [`db`]: SQLite database layer for products and bundles
//! - [`error`]: Unified error types and API response wrappers

pub mod commands;
pub mod config;
pub mod core;
pub mod db;
pub mod error;
