//! # Core Business Logic
//!
//! This module contains the core business logic for FileManagerDaz,
//! separated from Tauri command handlers for testability and reusability.
//!
//! ## Modules
//!
//! - [`extractor`]: Archive extraction engine (ZIP, RAR, 7z) with recursive nested handling
//! - [`analyzer`]: DAZ content analysis (manifest parsing, product identification)
//! - [`destination`]: Intelligent destination resolution based on content structure
//! - [`maintenance`]: Library maintenance (duplicate detection, orphan files, cleanup)
//! - [`catalog`]: Library catalog indexing (metadata scanning for products)
//! - [`trash`]: Safe file deletion via Windows Recycle Bin
//! - [`watcher`]: Folder watching for automatic import of new archives
//!
//! ## Design Principles
//!
//! 1. **Pure functions where possible**: Business logic takes inputs, returns outputs
//! 2. **No Tauri dependencies**: Core modules don't import `tauri::*`
//! 3. **Error propagation**: All functions return `AppResult<T>` for consistent handling

pub mod analyzer;
pub mod catalog;
pub mod destination;
pub mod extractor;
pub mod maintenance;
pub mod trash;
pub mod watcher;
