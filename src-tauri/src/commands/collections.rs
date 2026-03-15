//! Tauri commands for collection management

use crate::commands::products::{with_db, DbState};
use crate::db::Collection;
use crate::error::ApiResponse;
use tauri::State;
use tracing::info;

/// Creates a new collection.
#[tauri::command]
pub fn create_collection(name: String, db_state: State<DbState>) -> ApiResponse<Collection> {
    info!("create_collection: {}", name);
    with_db(&db_state, |db| db.create_collection(&name))
}

/// Lists all collections with item counts.
#[tauri::command]
pub fn list_collections(db_state: State<DbState>) -> ApiResponse<Vec<Collection>> {
    with_db(&db_state, |db| db.list_collections())
}

/// Renames a collection.
#[tauri::command]
pub fn rename_collection(id: i64, name: String, db_state: State<DbState>) -> ApiResponse<()> {
    info!("rename_collection: id={}, name={}", id, name);
    with_db(&db_state, |db| db.rename_collection(id, &name))
}

/// Deletes a collection.
#[tauri::command]
pub fn delete_collection(id: i64, db_state: State<DbState>) -> ApiResponse<()> {
    info!("delete_collection: id={}", id);
    with_db(&db_state, |db| db.delete_collection(id))
}

/// Adds products to a collection (bulk).
#[tauri::command]
pub fn add_to_collection(
    collection_id: i64,
    product_ids: Vec<i64>,
    db_state: State<DbState>,
) -> ApiResponse<usize> {
    info!(
        "add_to_collection: {} products -> collection {}",
        product_ids.len(),
        collection_id
    );
    with_db(&db_state, |db| {
        db.add_to_collection(collection_id, &product_ids)
    })
}

/// Removes products from a collection.
#[tauri::command]
pub fn remove_from_collection(
    collection_id: i64,
    product_ids: Vec<i64>,
    db_state: State<DbState>,
) -> ApiResponse<usize> {
    info!(
        "remove_from_collection: {} products from collection {}",
        product_ids.len(),
        collection_id
    );
    with_db(&db_state, |db| {
        db.remove_from_collection(collection_id, &product_ids)
    })
}
