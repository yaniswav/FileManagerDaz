//! # Import Task Commands
//!
//! Tauri commands for managing persisted import tasks.
//!
//! ## Overview
//!
//! Import tasks are persisted to SQLite to survive application restarts.
//! This enables:
//! - Resuming interrupted imports
//! - Retrying failed imports
//! - Viewing import history
//!
//! ## Task Lifecycle
//!
//! ```text
//! pending -> processing -> done
//!                      \-> error (retryable)
//!                      \-> interrupted (on app restart, retryable)
//! ```

use crate::config::settings::SETTINGS;
use crate::core::manifest;
use crate::core::trash;
use crate::db::{Database, NewProduct};
use crate::db::import_tasks::{ImportTaskStatus, ImportTasksRepository, PersistedImportTask};
use crate::error::{ApiResponse, AppError};
use chrono::{TimeZone, Utc};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::State;
use tracing::{info, warn};

use super::products::DbState;

// =============================================================================
// TAURI STATE
// =============================================================================

/// Thread-safe state wrapper for import tasks database access.
///
/// Holds a lazily-initialized SQLite connection for import task persistence.
pub struct ImportTasksState {
    repository: Mutex<Option<ImportTasksRepository>>,
}

impl ImportTasksState {
    /// Creates a new uninitialized state.
    pub fn new() -> Self {
        Self {
            repository: Mutex::new(None),
        }
    }

    /// Initializes the repository with the given database path.
    ///
    /// Must be called before any command that accesses the repository.
    /// Also marks any "processing" tasks as "interrupted" (app crashed/restarted).
    pub fn init(&self, db_path: &str) -> Result<(), String> {
        let connection =
            Connection::open(db_path).map_err(|e| format!("Failed to open database: {}", e))?;

        let repo = ImportTasksRepository::new(connection);

        // Create schema if needed
        repo.initialize()
            .map_err(|e| format!("Failed to initialize schema: {}", e))?;

        // Mark "processing" tasks as "interrupted" (app was restarted)
        if let Err(e) = repo.mark_interrupted() {
            tracing::warn!("Failed to mark interrupted tasks: {}", e);
        }

        *self.repository.lock().map_err(|_| "Repository mutex poisoned".to_string())? = Some(repo);
        Ok(())
    }

    /// Executes a function with access to the repository.
    fn with_repo<T, F>(&self, operation: F) -> Result<T, String>
    where
        F: FnOnce(&ImportTasksRepository) -> Result<T, crate::error::AppError>,
    {
        let guard = self.repository.lock().map_err(|_| "Lock error")?;
        let repo = guard.as_ref().ok_or("Repository not initialized")?;
        operation(repo).map_err(|e| e.to_string())
    }
}

// =============================================================================
// CRUD COMMANDS
// =============================================================================

/// Creates a new import task in the database.
#[tauri::command]
pub async fn create_import_task(
    state: State<'_, ImportTasksState>,
    id: String,
    source_path: String,
    name: String,
    target_library: Option<String>,
) -> Result<ApiResponse<bool>, String> {
    info!("Creating import task: {} - {}", id, name);

    state
        .with_repo(|repo| {
            let task = PersistedImportTask {
                id: id.clone(),
                source_path,
                name,
                status: ImportTaskStatus::Pending,
                destination: None,
                error_message: None,
                files_count: None,
                total_size: None,
                content_type: None,
                started_at: chrono::Utc::now().timestamp_millis(),
                completed_at: None,
                target_library,
            };
            repo.add_task(&task)?;
            Ok(true)
        })
        .map(ApiResponse::success)
        .or_else(|e| Ok(ApiResponse::error_msg("CREATE_ERROR", &e)))
}

/// Updates the status of an existing task.
#[tauri::command]
pub async fn update_import_task_status(
    state: State<'_, ImportTasksState>,
    id: String,
    status: String,
) -> Result<ApiResponse<bool>, String> {
    let parsed_status = match status.as_str() {
        "pending" => ImportTaskStatus::Pending,
        "processing" => ImportTaskStatus::Processing,
        "done" => ImportTaskStatus::Done,
        "error" => ImportTaskStatus::Error,
        "interrupted" => ImportTaskStatus::Interrupted,
        _ => {
            return Ok(ApiResponse::error_msg(
                "INVALID_STATUS",
                &format!("Unknown status: {}", status),
            ))
        }
    };

    state
        .with_repo(|repo| {
            repo.update_status(&id, parsed_status)?;
            Ok(true)
        })
        .map(ApiResponse::success)
        .or_else(|e| Ok(ApiResponse::error_msg("UPDATE_ERROR", &e)))
}

/// Marks a task as successfully completed.
///
/// Also moves source archives to trash if the `trash_archives_after_import`
/// setting is enabled.
#[tauri::command]
pub async fn complete_import_task(
    state: State<'_, ImportTasksState>,
    products_db_state: State<'_, DbState>,
    id: String,
    destination: String,
    files_count: i64,
    total_size: i64,
    content_type: Option<String>,
    source_archive_paths: Option<Vec<String>>,
) -> Result<ApiResponse<bool>, String> {
    info!("Completing import task: {}", id);

    // Mark the task as completed in database
    let result = state.with_repo(|repo| {
        repo.set_result(
            &id,
            &destination,
            files_count,
            total_size,
            content_type.as_deref(),
        )?;
        Ok(true)
    });

    // Best-effort: create a product record from the completed task
    if result.is_ok() {
        match state.with_repo(|repo| repo.get_task(&id)) {
            Ok(Some(task)) => {
                if let Err(e) = upsert_product_from_task(&products_db_state, &task) {
                    warn!("Failed to create product for task {}: {}", id, e);
                }
            }
            Ok(None) => warn!("Cannot create product: task not found after completion: {}", id),
            Err(e) => warn!("Cannot create product: failed to fetch task {}: {}", id, e),
        }
    }

    // If import succeeded and trash setting is enabled, move archives to trash
    if result.is_ok() {
        let should_trash = SETTINGS
            .read()
            .map(|s| s.trash_archives_after_import)
            .unwrap_or(false);

        if should_trash {
            if let Some(archive_paths) = source_archive_paths {
                trash_source_archives(&archive_paths, &destination);
            }
        }
    }

    result
        .map(ApiResponse::success)
        .or_else(|e| Ok(ApiResponse::error_msg("COMPLETE_ERROR", &e)))
}

/// Creates a product record from a completed import task.
/// Returns `Ok(true)` if created, `Ok(false)` if skipped (already exists or missing data).
///
/// Also parses Manifest.dsx/Supplement.dsx from the destination to:
/// - Enrich product metadata (global_id, product name)
/// - Store per-file inventory in `product_files` table
fn upsert_product_from_task(
    products_db_state: &State<'_, DbState>,
    task: &PersistedImportTask,
) -> Result<bool, AppError> {
    let destination = match &task.destination {
        Some(d) => d,
        None => return Ok(false),
    };

    // Optional installed date from task completion timestamp
    let installed_at = task
        .completed_at
        .and_then(|ts| Utc.timestamp_millis_opt(ts).single())
        .map(|dt| dt.to_rfc3339());

    let mut new_product = NewProduct::new(&task.name, destination)
        .with_import_task_id(task.id.clone())
        .with_stats(task.files_count.unwrap_or(0), task.total_size.unwrap_or(0));

    if let Some(installed_at) = installed_at {
        new_product = new_product.with_installed_at(installed_at);
    }

    // Link source archive if the source is an archive file
    if trash::is_archive_file(Path::new(&task.source_path)) {
        new_product = new_product.with_source(task.source_path.clone());
    }

    if let Some(content_type) = &task.content_type {
        new_product = new_product.with_content_type(content_type.clone());
    }

    // Parse Manifest.dsx / Supplement.dsx from destination library
    let product_manifest = manifest::parse_product_manifests(Path::new(destination))
        .unwrap_or_else(|e| {
            warn!("Failed to parse manifests in {}: {}", destination, e);
            manifest::ProductManifest::default()
        });

    // Enrich product with manifest metadata
    if let Some(global_id) = &product_manifest.global_id {
        new_product = new_product.with_global_id(global_id.clone());
    }
    // Use Supplement product name if it's more descriptive than the archive filename
    if let Some(product_name) = &product_manifest.product_name {
        if !product_name.is_empty() {
            new_product.name = product_name.clone();
        }
    }

    let guard = products_db_state
        .0
        .lock()
        .map_err(|_| AppError::Internal("Database mutex poisoned".to_string()))?;

    let db: &Database = guard
        .as_ref()
        .ok_or_else(|| AppError::Internal("Database not initialized".to_string()))?;

    // Idempotency: skip if we already created a product for this task.
    if db.get_product_by_import_task_id(&task.id)?.is_some() {
        return Ok(false);
    }

    let product_id = db.add_product(&new_product)?;

    // Store per-file inventory from manifest
    if !product_manifest.files.is_empty() {
        let file_entries: Vec<(String, String)> = product_manifest
            .files
            .iter()
            .map(|f| (f.relative_path.clone(), f.target.clone()))
            .collect();

        db.with_connection(|conn| {
            crate::db::product_files::insert_product_files_batch(conn, product_id, &file_entries)
        })
        .unwrap_or_else(|e| {
            warn!("Failed to store product files for {}: {}", task.name, e);
            0
        });

        info!(
            "Stored {} manifest files for product {} (id={})",
            file_entries.len(),
            new_product.name,
            product_id
        );
    }

    Ok(true)
}

/// Moves source archives to trash after successful import.
fn trash_source_archives(archive_paths: &[String], destination: &str) {
    for source in archive_paths {
        let source_path = Path::new(source);

        // Only trash actual archives (not folders)
        if !trash::is_archive_file(source_path) {
            continue;
        }

        // Safety: don't trash if destination is inside source
        let dest_path = Path::new(destination);
        if source_path == dest_path || destination.starts_with(source) {
            continue;
        }

        // Skip if archive no longer exists (already removed during extraction)
        if !source_path.exists() {
            info!("Archive already removed, skipping trash: {}", source);
            continue;
        }

        match trash::move_to_trash(source_path) {
            Ok(true) => {
                info!("Archive moved to trash after successful import: {}", source);
            }
            Ok(false) => {
                info!("Archive not found for trash: {}", source);
            }
            Err(e) => {
                // Trash failure is a warning only, doesn't affect import success
                warn!("Failed to move archive to trash: {} - {}", source, e);
            }
        }
    }
}

/// Marks a task as failed with an error message.
#[tauri::command]
pub async fn fail_import_task(
    state: State<'_, ImportTasksState>,
    id: String,
    error: String,
) -> Result<ApiResponse<bool>, String> {
    info!("Failing import task: {} - {}", id, error);

    state
        .with_repo(|repo| {
            repo.set_error(&id, &error)?;
            Ok(true)
        })
        .map(ApiResponse::success)
        .or_else(|e| Ok(ApiResponse::error_msg("FAIL_ERROR", &e)))
}

// =============================================================================
// QUERY COMMANDS
// =============================================================================

/// Retrieves a single task by ID.
#[tauri::command]
pub async fn get_import_task(
    state: State<'_, ImportTasksState>,
    id: String,
) -> Result<ApiResponse<Option<PersistedImportTask>>, String> {
    state
        .with_repo(|repo| repo.get_task(&id))
        .map(ApiResponse::success)
        .or_else(|e| Ok(ApiResponse::error_msg("GET_ERROR", &e)))
}

/// Lists all import tasks.
#[tauri::command]
pub async fn list_import_tasks(
    state: State<'_, ImportTasksState>,
) -> Result<ApiResponse<Vec<PersistedImportTask>>, String> {
    state
        .with_repo(|repo| repo.list_tasks())
        .map(ApiResponse::success)
        .or_else(|e| Ok(ApiResponse::error_msg("LIST_ERROR", &e)))
}

/// Lists recent import tasks within the last `days` (default: 7).
#[tauri::command]
pub async fn list_recent_import_tasks(
    state: State<'_, ImportTasksState>,
    days: Option<i64>,
) -> Result<ApiResponse<Vec<PersistedImportTask>>, String> {
    let lookback_days = days.unwrap_or(7);
    state
        .with_repo(|repo| repo.list_tasks_since_days(lookback_days))
        .map(ApiResponse::success)
        .or_else(|e| Ok(ApiResponse::error_msg("LIST_ERROR", &e)))
}

/// Lists tasks that can be retried (error or interrupted status).
#[tauri::command]
pub async fn list_retryable_tasks(
    state: State<'_, ImportTasksState>,
) -> Result<ApiResponse<Vec<PersistedImportTask>>, String> {
    state
        .with_repo(|repo| {
            let mut tasks = repo.list_tasks_by_status(ImportTaskStatus::Error)?;
            tasks.extend(repo.list_tasks_by_status(ImportTaskStatus::Interrupted)?);
            Ok(tasks)
        })
        .map(ApiResponse::success)
        .or_else(|e| Ok(ApiResponse::error_msg("LIST_ERROR", &e)))
}

// =============================================================================
// MANAGEMENT COMMANDS
// =============================================================================

/// Creates missing product records from completed import tasks.
///
/// Useful to migrate existing users who have import history but an empty products database.
#[tauri::command]
pub async fn backfill_products_from_import_tasks(
    state: State<'_, ImportTasksState>,
    products_db_state: State<'_, DbState>,
) -> Result<ApiResponse<usize>, String> {
    info!("Backfilling products from import tasks");

    let tasks = match state.with_repo(|repo| repo.list_tasks_by_status(ImportTaskStatus::Done)) {
        Ok(tasks) => tasks,
        Err(e) => return Ok(ApiResponse::error_msg("BACKFILL_ERROR", &e)),
    };

    let mut created = 0usize;
    for task in tasks {
        match upsert_product_from_task(&products_db_state, &task) {
            Ok(true) => created += 1,
            Ok(false) => {}
            Err(e) => warn!("Failed to backfill product for task {}: {}", task.id, e),
        }
    }

    Ok(ApiResponse::success(created))
}

/// Prepares a task for retry by resetting its status to pending.
#[tauri::command]
pub async fn prepare_task_retry(
    state: State<'_, ImportTasksState>,
    id: String,
) -> Result<ApiResponse<bool>, String> {
    info!("Preparing task for retry: {}", id);

    // Validate source path still exists
    let task_opt = state
        .with_repo(|repo| repo.get_task(&id))
        .map_err(|e| e.to_string())?;

    match task_opt {
        Some(task) => {
            let source_path = Path::new(&task.source_path);
            if !source_path.exists() {
                return Ok(ApiResponse::error(AppError::NotFound(
                    source_path.to_path_buf(),
                )));
            }
        }
        None => {
            return Ok(ApiResponse::error(AppError::NotFound(PathBuf::from(id))));
        }
    }

    state
        .with_repo(|repo| {
            repo.prepare_retry(&id)?;
            Ok(true)
        })
        .map(ApiResponse::success)
        .or_else(|e| Ok(ApiResponse::error_msg("RETRY_ERROR", &e)))
}

/// Deletes a task from the database.
#[tauri::command]
pub async fn delete_import_task(
    state: State<'_, ImportTasksState>,
    id: String,
) -> Result<ApiResponse<bool>, String> {
    info!("Deleting import task: {}", id);

    state
        .with_repo(|repo| {
            repo.delete_task(&id)?;
            Ok(true)
        })
        .map(ApiResponse::success)
        .or_else(|e| Ok(ApiResponse::error_msg("DELETE_ERROR", &e)))
}

/// Deletes tasks older than the specified number of days.
#[tauri::command]
pub async fn cleanup_old_import_tasks(
    state: State<'_, ImportTasksState>,
    days: i64,
) -> Result<ApiResponse<usize>, String> {
    info!("Cleaning up tasks older than {} days", days);

    state
        .with_repo(|repo| repo.cleanup_old_tasks(days))
        .map(ApiResponse::success)
        .or_else(|e| Ok(ApiResponse::error_msg("CLEANUP_ERROR", &e)))
}

/// Deletes all completed tasks (done or error status).
#[tauri::command]
pub async fn clear_completed_import_tasks(
    state: State<'_, ImportTasksState>,
) -> Result<ApiResponse<usize>, String> {
    info!("Clearing all completed import tasks");

    state
        .with_repo(|repo| repo.delete_completed_tasks())
        .map(ApiResponse::success)
        .or_else(|e| Ok(ApiResponse::error_msg("CLEAR_ERROR", &e)))
}
