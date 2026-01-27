//! # Tauri Command Handlers
//!
//! This module contains all Tauri commands exposed to the frontend via IPC.
//! Commands are thin wrappers around core business logic, handling:
//!
//! - Parameter validation
//! - State access (database, watcher)
//! - Response formatting via [`ApiResponse<T>`]
//!
//! ## Modules
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`archive`] | Archive extraction, source processing, destination handling |
//! | [`bundles`] | Installed bundle tracking, duplicate detection |
//! | [`import_tasks`] | Import history persistence and retry management |
//! | [`maintenance`] | Library scanning, cleanup, orphan detection |
//! | [`products`] | Product catalog CRUD (SQLite) |
//! | [`settings`] | App configuration, DAZ library management |
//! | [`system`] | Health check (ping) |
//! | [`watcher`] | Download folder watching for auto-import |
//!
//! ## Naming Convention
//!
//! Commands follow the pattern `{action}_{resource}` or `{resource}_{action}`:
//! - `create_import_task`, `list_products`, `get_app_config`
//! - Suffix `_cmd` is used to avoid name collisions with core functions

pub mod archive;
pub mod bundles;
pub mod import_tasks;
pub mod maintenance;
pub mod products;
pub mod settings;
pub mod system;
pub mod watcher;
