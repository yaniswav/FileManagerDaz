//! Tauri commands for product management

use crate::config::SETTINGS;
use blake3;
use crate::core::catalog::{list_support_metadata_files, normalize_rel_path, parse_daz_metadata_file, CatalogProduct};
use crate::db::{Database, LibraryProductInput, NewProduct, Product, UpdateProduct};
use crate::error::{ApiResponse, AppError};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::State;
use tokio::task::spawn_blocking;
use tracing::{error, info};

// ============================================================================
// Shared State
// ============================================================================

/// Shared database state
pub struct DbState(pub Mutex<Option<Database>>);

impl DbState {
    /// Creates a new empty state
    pub fn new() -> Self {
        Self(Mutex::new(None))
    }

    /// Initializes the database
    pub fn init(&self) -> Result<(), AppError> {
        let settings = SETTINGS
            .read()
            .map_err(|e| AppError::Config(format!("Cannot read settings: {}", e)))?;

        let db_path = &settings.database_path;
        info!("Opening database at: {:?}", db_path);

        let db = Database::open(db_path)?;

        let mut state = self
            .0
            .lock()
            .map_err(|_| AppError::Internal("Database mutex poisoned".to_string()))?;
        *state = Some(db);

        Ok(())
    }
}

impl Default for DbState {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Command Parameters
// ============================================================================

/// Parameters for creating a product
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProductParams {
    pub name: String,
    pub path: String,
    pub source_archive: Option<String>,
    pub tags: Option<Vec<String>>,
    pub content_type: Option<String>,
    pub files_count: Option<i64>,
    pub total_size: Option<i64>,
}

/// Parameters for updating a product
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProductParams {
    pub name: Option<String>,
    pub tags: Option<Vec<String>>,
    pub content_type: Option<String>,
    pub notes: Option<String>,
}

// ============================================================================
// CRUD Commands
// ============================================================================

/// Lists all products
#[tauri::command]
pub fn list_products(db_state: State<DbState>) -> ApiResponse<Vec<Product>> {
    with_db(&db_state, |db| db.list_products())
}

/// Creates a new product
#[tauri::command]
pub fn create_product(
    params: CreateProductParams,
    db_state: State<DbState>,
) -> ApiResponse<Product> {
    info!("create_product: {}", params.name);

    with_db(&db_state, |db| {
        let mut new_product = NewProduct::new(&params.name, &params.path);

        if let Some(source) = &params.source_archive {
            new_product = new_product.with_source(source.clone());
        }
        if let Some(tags) = &params.tags {
            new_product = new_product.with_tags(tags.join(","));
        }
        if let Some(content_type) = &params.content_type {
            new_product = new_product.with_content_type(content_type.clone());
        }
        if let (Some(files), Some(size)) = (params.files_count, params.total_size) {
            new_product = new_product.with_stats(files, size);
        }

        let id = db.add_product(&new_product)?;

        // Get the created product
        db.get_product(id)?
            .ok_or_else(|| AppError::NotFound(std::path::PathBuf::from(format!("product:{}", id))))
    })
}

/// Gets a product by ID
#[tauri::command]
pub fn get_product(id: i64, db_state: State<DbState>) -> ApiResponse<Option<Product>> {
    with_db(&db_state, |db| db.get_product(id))
}

/// Updates a product
#[tauri::command]
pub fn update_product(
    id: i64,
    params: UpdateProductParams,
    db_state: State<DbState>,
) -> ApiResponse<Product> {
    info!("update_product: id={}", id);

    with_db(&db_state, |db| {
        let update = UpdateProduct {
            name: params.name,
            tags: params.tags.map(|t| t.join(",")),
            content_type: params.content_type,
            notes: params.notes,
        };

        db.update_product(id, &update)?;

        db.get_product(id)?
            .ok_or_else(|| AppError::NotFound(std::path::PathBuf::from(format!("product:{}", id))))
    })
}

/// Deletes a product
#[tauri::command]
pub fn delete_product(id: i64, db_state: State<DbState>) -> ApiResponse<bool> {
    info!("delete_product: id={}", id);
    with_db(&db_state, |db| db.delete_product(id))
}

/// Searches for products
#[tauri::command]
pub fn search_products(query: String, db_state: State<DbState>) -> ApiResponse<Vec<Product>> {
    info!("search_products: {}", query);
    with_db(&db_state, |db| db.search_products(&query))
}

/// Lists products indexed from DAZ libraries.
#[tauri::command]
pub fn list_library_products(db_state: State<DbState>) -> ApiResponse<Vec<Product>> {
    with_db(&db_state, |db| db.list_library_products())
}

/// Searches products indexed from DAZ libraries.
#[tauri::command]
pub fn search_library_products(query: String, db_state: State<DbState>) -> ApiResponse<Vec<Product>> {
    info!("search_library_products: {}", query);
    with_db(&db_state, |db| db.search_library_products(&query))
}

/// Scans DAZ libraries and rebuilds the product catalog from metadata.
#[tauri::command]
pub async fn scan_library_products(
    library_path: Option<String>,
) -> ApiResponse<usize> {
    let (db_path, thumbnails_dir, libraries) = match SETTINGS.read() {
        Ok(settings) => {
            let libs = match library_path {
                Some(path) => vec![PathBuf::from(path)],
                None => settings.daz_libraries.clone(),
            };
            (settings.database_path.clone(), settings.thumbnails_dir.clone(), libs)
        }
        Err(e) => {
            return ApiResponse::error(AppError::Config(format!(
                "Cannot read settings: {}",
                e
            )))
        }
    };

    let result = spawn_blocking(move || {
        let db = Database::open(&db_path)?;
        let mut total = 0usize;

        for library in libraries {
            total += scan_library(&db, &library, &thumbnails_dir)?;
        }

        Ok(total)
    })
    .await;

    match result {
        Ok(Ok(count)) => ApiResponse::success(count),
        Ok(Err(err)) => ApiResponse::error(err),
        Err(join_error) => ApiResponse::error(AppError::Internal(format!(
            "Scan task error: {}",
            join_error
        ))),
    }
}

// ============================================================================
// Helper
// ============================================================================

/// Executes an operation on the database
fn with_db<T, F>(db_state: &State<DbState>, f: F) -> ApiResponse<T>
where
    T: serde::Serialize,
    F: FnOnce(&Database) -> Result<T, AppError>,
{
    let guard = match db_state.0.lock() {
        Ok(g) => g,
        Err(_) => {
            error!("Database mutex poisoned");
            return ApiResponse::error(AppError::Internal("Database lock error".to_string()));
        }
    };

    match guard.as_ref() {
        Some(db) => match f(db) {
            Ok(data) => ApiResponse::success(data),
            Err(e) => {
                error!("Database operation failed: {}", e);
                ApiResponse::error(e)
            }
        },
        None => {
            error!("Database not initialized");
            ApiResponse::error(AppError::Internal("Database not initialized".to_string()))
        }
    }
}

// =============================================================================
// Library catalog helpers
// =============================================================================

fn scan_library(db: &Database, library_path: &Path, thumbnails_dir: &Path) -> Result<usize, AppError> {
    if !library_path.exists() {
        return Ok(0);
    }

    fs::create_dir_all(thumbnails_dir)?;

    let metadata_files = list_support_metadata_files(library_path)?;
    let mut indexed = 0usize;

    for metadata_path in metadata_files {
        let products = parse_daz_metadata_file(&metadata_path)?;
        if products.is_empty() {
            continue;
        }

        // DSX files usually contain a single product; index the first one.
        let product = products.first().cloned().unwrap();

        let support_file = metadata_path
            .strip_prefix(library_path)
            .unwrap_or(&metadata_path)
            .to_string_lossy()
            .replace('\\', "/");

        let installed_at = metadata_path
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .map(|mtime| DateTime::<Utc>::from(mtime).to_rfc3339())
            .unwrap_or_else(|| Utc::now().to_rfc3339());

        let categories = sorted_categories(&product.categories);
        let content_type = infer_content_type(&product.content_types, &categories);
        let vendor = select_vendor(&product);

        let product_path = resolve_product_path(library_path, &product.assets);
        let thumbnail_path = select_and_cache_thumbnail(
            library_path,
            thumbnails_dir,
            &support_file,
            &product,
        )?;

        let input = LibraryProductInput {
            name: product.name,
            path: product_path.to_string_lossy().to_string(),
            library_path: library_path.to_string_lossy().to_string(),
            support_file,
            product_token: product.product_token,
            global_id: product.global_id,
            vendor,
            categories,
            content_type,
            installed_at,
            thumbnail_path: thumbnail_path.map(|p| p.to_string_lossy().to_string()),
            files_count: 0,
            total_size: 0,
        };

        db.upsert_library_product(&input)?;
        indexed += 1;
    }

    Ok(indexed)
}

fn sorted_categories(categories: &std::collections::HashSet<String>) -> Vec<String> {
    let mut list: Vec<String> = categories.iter().cloned().collect();
    list.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    list
}

fn select_vendor(product: &CatalogProduct) -> Option<String> {
    if !product.artists.is_empty() {
        return Some(product.artists.join(", "));
    }
    product.store_id.clone()
}

fn resolve_product_path(library_path: &Path, assets: &[String]) -> PathBuf {
    let asset = select_primary_asset(assets);
    if let Some(asset) = asset {
        let asset_path = library_path.join(normalize_rel_path(&asset));
        if let Some(parent) = asset_path.parent() {
            return parent.to_path_buf();
        }
        return asset_path;
    }
    library_path.to_path_buf()
}

fn select_primary_asset(assets: &[String]) -> Option<String> {
    let mut candidates: Vec<&String> = assets.iter().collect();
    candidates.sort_by_key(|path| asset_priority(path));
    candidates.first().map(|s| (*s).clone())
}

fn asset_priority(path: &str) -> u8 {
    let lower = path.to_lowercase();
    if lower.ends_with(".duf") {
        return 0;
    }
    if lower.ends_with(".dsf") {
        return 1;
    }
    if lower.ends_with(".dsa") || lower.ends_with(".dse") {
        return 2;
    }
    3
}

fn infer_content_type(raw_types: &[String], categories: &[String]) -> Option<String> {
    let mut inputs: Vec<String> = raw_types.iter().cloned().collect();
    inputs.extend(categories.iter().cloned());

    for value in inputs {
        let value = value.to_lowercase();
        if value.contains("character") || value.contains("figure") {
            return Some("character".to_string());
        }
        if value.contains("clothing") || value.contains("wardrobe") {
            return Some("clothing".to_string());
        }
        if value.contains("hair") {
            return Some("hair".to_string());
        }
        if value.contains("pose") {
            return Some("pose".to_string());
        }
        if value.contains("prop") {
            return Some("prop".to_string());
        }
        if value.contains("environment") || value.contains("scene") {
            return Some("environment".to_string());
        }
        if value.contains("light") {
            return Some("light".to_string());
        }
        if value.contains("material") || value.contains("shader") {
            return Some("material".to_string());
        }
        if value.contains("script") {
            return Some("script".to_string());
        }
        if value.contains("morph") {
            return Some("morph".to_string());
        }
        if value.contains("hdri") {
            return Some("hdri".to_string());
        }
    }

    None
}

fn select_and_cache_thumbnail(
    library_path: &Path,
    thumbnails_dir: &Path,
    support_file: &str,
    product: &CatalogProduct,
) -> Result<Option<PathBuf>, AppError> {
    let mut candidates: Vec<(i32, PathBuf)> = Vec::new();

    for asset in &product.support_assets {
        if let Some(candidate) = support_asset_candidate(library_path, asset) {
            let score = score_thumbnail_candidate(&candidate, asset);
            candidates.push((score, candidate));
        }
    }

    if candidates.is_empty() {
        for asset in &product.assets {
            if let Some(candidate) = asset_thumbnail_candidate(library_path, asset) {
                let score = score_thumbnail_candidate(&candidate, asset);
                candidates.push((score, candidate));
            }
        }
    }

    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    let source = match candidates.first() {
        Some((_, path)) => path,
        None => return Ok(None),
    };

    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png");
    let key = format!("{}|{}", library_path.display(), support_file);
    let hash = blake3::hash(key.as_bytes()).to_hex().to_string();
    let dest = thumbnails_dir.join(format!("{}.{}", hash, ext));

    if !dest.exists() {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let _ = fs::copy(source, &dest);
    }

    Ok(Some(dest))
}

fn support_asset_candidate(library_path: &Path, asset: &str) -> Option<PathBuf> {
    let lower = asset.to_lowercase();
    if !(lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg")) {
        return None;
    }
    if lower.ends_with(".tip.png") || lower.ends_with(".tip.jpg") || lower.ends_with(".tip.jpeg") {
        return None;
    }
    if lower.contains("/runtime/textures/") || lower.contains("/data/") {
        return None;
    }

    let relative = normalize_rel_path(asset);
    let candidate = library_path.join(relative);
    if candidate.exists() {
        Some(candidate)
    } else {
        None
    }
}

fn asset_thumbnail_candidate(library_path: &Path, asset: &str) -> Option<PathBuf> {
    let relative = normalize_rel_path(asset);
    let asset_path = library_path.join(relative);
    if !asset_path.exists() {
        return None;
    }

    let file_name = asset_path.file_name()?.to_string_lossy();
    let duffile_png = asset_path.with_file_name(format!("{}.png", file_name));
    if duffile_png.exists() {
        return Some(duffile_png);
    }

    let stem_png = asset_path.with_extension("png");
    if stem_png.exists() {
        return Some(stem_png);
    }

    None
}

fn score_thumbnail_candidate(path: &Path, original: &str) -> i32 {
    let mut score = 0;
    let lower = original.to_lowercase();

    if lower.contains("preview") || lower.contains("promo") || lower.contains("thumb") {
        score += 3;
    }
    if lower.contains("icon") || lower.contains("main") {
        score += 2;
    }
    if lower.contains("/people/") || lower.contains("/props/") || lower.contains("/environments/") {
        score += 2;
    }
    if lower.contains("/runtime/textures/") || lower.contains("/data/") {
        score -= 4;
    }
    if path.extension().and_then(|e| e.to_str()).map_or(false, |e| e.eq_ignore_ascii_case("png")) {
        score += 1;
    }

    score
}
