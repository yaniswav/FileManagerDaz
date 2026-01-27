//! Tauri commands for bundle management
//!
//! Handles detection of already installed bundles and integrity verification.

use super::products::DbState;
use crate::db::bundles;
use crate::error::{ApiResponse, AppError};
use std::path::PathBuf;
use tauri::State;
use tracing::{debug, error, info};

// ============================================================================
// Database access helper
// ============================================================================

/// Executes a closure with database access
fn with_db<T, F>(db_state: &State<DbState>, f: F) -> ApiResponse<T>
where
    T: serde::Serialize,
    F: FnOnce(&crate::db::Database) -> crate::error::AppResult<T>,
{
    let state_guard = match db_state.0.lock() {
        Ok(guard) => guard,
        Err(_) => {
            return ApiResponse::error(AppError::Internal("Database mutex poisoned".to_string()));
        }
    };

    let db = match state_guard.as_ref() {
        Some(db) => db,
        None => {
            return ApiResponse::error(AppError::Internal("Database not initialized".to_string()));
        }
    };

    match f(db) {
        Ok(result) => ApiResponse::success(result),
        Err(e) => {
            error!("Database operation failed: {}", e);
            ApiResponse::error(e)
        }
    }
}

// ============================================================================
// Tauri commands
// ============================================================================

/// Checks if an archive is already installed
#[tauri::command]
pub fn check_bundle_installed(
    archive_path: String,
    db_state: State<'_, DbState>,
) -> ApiResponse<bundles::PreInstallCheck> {
    info!("Checking if bundle is installed: {}", archive_path);

    let path = PathBuf::from(&archive_path);

    // Compute archive hash
    let hash = match bundles::compute_archive_hash(&path) {
        Ok(h) => h,
        Err(e) => {
            error!("Failed to compute hash: {}", e);
            return ApiResponse::error(e);
        }
    };

    debug!("Archive hash: {}", &hash[..16]);

    with_db(&db_state, |db| {
        db.with_connection(|conn| bundles::check_bundle_by_hash(conn, &hash))
    })
}

/// Registers a bundle as installed
#[tauri::command]
pub fn register_bundle(
    archive_path: String,
    file_count: usize,
    total_size: u64,
    destination_path: String,
    db_state: State<'_, DbState>,
) -> ApiResponse<i64> {
    info!(
        "Registering bundle: {} -> {}",
        archive_path, destination_path
    );

    let archive = PathBuf::from(&archive_path);
    let dest = PathBuf::from(&destination_path);

    // Compute the hash
    let hash = match bundles::compute_archive_hash(&archive) {
        Ok(h) => h,
        Err(e) => {
            error!("Failed to compute hash: {}", e);
            return ApiResponse::error(e);
        }
    };

    with_db(&db_state, |db| {
        db.with_connection(|conn| {
            bundles::register_bundle(conn, &hash, &archive, file_count, total_size, &dest)
        })
    })
}

/// Lists all installed bundles
#[tauri::command]
pub fn list_installed_bundles(
    db_state: State<'_, DbState>,
) -> ApiResponse<Vec<bundles::InstalledBundle>> {
    debug!("Listing installed bundles");

    with_db(&db_state, |db| {
        db.with_connection(|conn| bundles::list_bundles(conn))
    })
}

/// Verifies the integrity of an installed bundle
#[tauri::command]
pub fn verify_bundle_integrity(
    bundle_id: i64,
    db_state: State<'_, DbState>,
) -> ApiResponse<bundles::IntegrityCheckResult> {
    info!("Verifying bundle integrity: {}", bundle_id);

    with_db(&db_state, |db| {
        db.with_connection(|conn| bundles::verify_bundle_integrity(conn, bundle_id))
    })
}

/// Removes a bundle from the database (does not delete files)
#[tauri::command]
pub fn remove_bundle_record(bundle_id: i64, db_state: State<'_, DbState>) -> ApiResponse<()> {
    info!("Removing bundle record: {}", bundle_id);

    with_db(&db_state, |db| {
        db.with_connection(|conn| bundles::remove_bundle(conn, bundle_id))
    })
}
