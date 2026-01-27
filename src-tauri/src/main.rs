//! # FileManagerDaz - Application Entry Point
//!
//! This is the main entry point for the FileManagerDaz Tauri application.
//!
//! ## Application Overview
//!
//! FileManagerDaz is a desktop application designed to simplify the management
//! of DAZ 3D asset archives (ZIP, RAR, 7z). Key features include:
//!
//! - **Archive extraction**: Recursive extraction with nested archive handling
//! - **Content analysis**: Manifest parsing and product identification
//! - **Library management**: Multiple DAZ library destinations support
//! - **Import tracking**: Persistent import task history with retry capabilities
//! - **Folder watching**: Automatic processing of new archives in watched folders
//!
//! ## Architecture
//!
//! The application follows a Tauri v2 architecture:
//! - **Backend** (Rust): Core logic in `src-tauri/src/`
//! - **Frontend** (Svelte 5): UI components in `src/lib/`
//!
//! State is managed through Tauri's managed state with thread-safe wrappers.
//!
//! ## Module Organization
//!
//! - [`commands`]: Tauri command handlers exposed to the frontend
//! - [`config`]: Application configuration and settings persistence
//! - [`core`]: Business logic (extraction, analysis, destination resolution)
//! - [`db`]: SQLite database layer for products and bundles
//! - [`error`]: Unified error types and API response wrappers

// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use commands::import_tasks::ImportTasksState;
use commands::products::DbState;
use config::SETTINGS;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod commands;
mod config;
mod core;
mod db;
mod error;

/// Application entry point.
///
/// Initializes all application subsystems in order:
/// 1. Logging (tracing-subscriber with env filter)
/// 2. Products database (SQLite for product catalog)
/// 3. Import tasks database (SQLite for import history)
/// 4. Folder watcher state (for automatic import)
/// 5. Tauri application with all plugins and command handlers
fn main() {
    // === Logging Initialization ===
    // Uses RUST_LOG env var if set, otherwise defaults to "info" level
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .init();

    info!("Starting FileManagerDaz");

    // === Configuration Logging ===
    if let Ok(settings) = SETTINGS.read() {
        info!("Database path: {:?}", settings.database_path);
        info!("Temp dir: {:?}", settings.temp_dir);
        info!(
            "RAR extraction: {}",
            if settings.can_extract_rar() {
                "available"
            } else {
                "unavailable"
            }
        );
    }

    // === Products Database Initialization ===
    let products_db_state = DbState::new();
    if let Err(e) = products_db_state.init() {
        error!("Failed to initialize products database: {}", e);
    } else {
        info!("Products database initialized successfully");
    }

    // === Import Tasks Database Initialization ===
    let import_tasks_state = ImportTasksState::new();

    // Initialize import tasks database (separate from products database)
    if let Ok(settings) = SETTINGS.read() {
        let import_tasks_db_path = settings.app_data_dir.join("import_tasks.db");
        if let Err(e) = import_tasks_state.init(import_tasks_db_path.to_string_lossy().as_ref()) {
            error!("Failed to initialize import tasks database: {}", e);
        } else {
            info!("Import tasks database initialized");
        }
    }

    // === Folder Watcher State ===
    // Manages automatic watching of download folders for new archives
    let folder_watcher_state = commands::watcher::TauriWatcherState::new();

    // === Tauri Application Setup ===
    tauri::Builder::default()
        // Plugins
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        // Managed state (thread-safe, accessible from commands)
        .manage(products_db_state)
        .manage(import_tasks_state)
        .manage(folder_watcher_state)
        // Command handlers (grouped by feature)
        .invoke_handler(tauri::generate_handler![
            // --- System Commands ---
            commands::system::ping,
            // --- Archive Processing Commands ---
            commands::archive::process_source_cmd,
            commands::archive::get_source_info,
            commands::archive::get_supported_formats_cmd,
            commands::archive::process_sources_batch,
            commands::archive::process_batch_robust,
            commands::archive::cleanup_temp_extractions,
            commands::archive::get_checkpoint_status,
            commands::archive::process_source_recursive_cmd,
            commands::archive::process_source_recursive_with_events_cmd,
            commands::archive::propose_destination_cmd,
            commands::archive::move_to_custom_destination,
            commands::archive::trash_source_archive,
            commands::archive::normalize_batch_cmd,
            // --- Import Task Commands (persistence) ---
            commands::import_tasks::create_import_task,
            commands::import_tasks::update_import_task_status,
            commands::import_tasks::complete_import_task,
            commands::import_tasks::fail_import_task,
            commands::import_tasks::get_import_task,
            commands::import_tasks::list_import_tasks,
            commands::import_tasks::list_recent_import_tasks,
            commands::import_tasks::list_retryable_tasks,
            commands::import_tasks::backfill_products_from_import_tasks,
            commands::import_tasks::prepare_task_retry,
            commands::import_tasks::delete_import_task,
            commands::import_tasks::cleanup_old_import_tasks,
            commands::import_tasks::clear_completed_import_tasks,
            // --- Product Catalog Commands (CRUD) ---
            commands::products::list_products,
            commands::products::list_library_products,
            commands::products::create_product,
            commands::products::get_product,
            commands::products::update_product,
            commands::products::delete_product,
            commands::products::search_products,
            commands::products::search_library_products,
            commands::products::scan_library_products,
            // --- Settings & Library Commands ---
            commands::settings::get_app_config,
            commands::settings::list_daz_libraries,
            commands::settings::detect_daz_libraries,
            commands::settings::add_daz_library,
            commands::settings::remove_daz_library,
            commands::settings::set_default_library,
            commands::settings::set_temp_dir,
            commands::settings::detect_external_tools,
            commands::settings::set_trash_archives_after_import,
            commands::settings::set_dev_log_extraction_timings,
            commands::settings::set_dev_log_extraction_details,
            commands::settings::set_language,
            // --- Maintenance Commands ---
            commands::maintenance::scan_library_cmd,
            commands::maintenance::scan_all_libraries_cmd,
            commands::maintenance::cleanup_files_cmd,
            commands::maintenance::cleanup_empty_folders_cmd,
            commands::maintenance::cleanup_library_complete_cmd,
            commands::maintenance::quick_maintenance_scan_cmd,
            // --- Folder Watcher Commands ---
            commands::watcher::start_watching,
            commands::watcher::stop_watching,
            commands::watcher::get_watcher_info,
            commands::watcher::poll_watch_events,
            commands::watcher::scan_watched_folder,
            commands::watcher::get_downloads_folder,
            // --- Bundle Tracking Commands ---
            commands::bundles::check_bundle_installed,
            commands::bundles::register_bundle,
            commands::bundles::list_installed_bundles,
            commands::bundles::verify_bundle_integrity,
            commands::bundles::remove_bundle_record,
        ])
        .run(tauri::generate_context!())
        .expect("Failed to run Tauri application");
}
