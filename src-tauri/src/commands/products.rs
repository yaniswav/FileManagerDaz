//! Tauri commands for product management

use crate::commands::TaskPayload;
use crate::config::SettingsState;
use blake3;
use crate::core::catalog::{list_support_metadata_files, normalize_rel_path, parse_daz_metadata_file, CatalogProduct};
use crate::db::{Database, DuplicateGroup, IntegrityReport, LibraryProductInput, LibraryStats, NewProduct, Product, SceneAnalysisReport, UninstallReport, UpdateProduct};
use crate::error::{ApiResponse, AppError};
use chrono::{DateTime, Utc};
use rusqlite::params;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{Emitter, State};
use tokio::task::spawn_blocking;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

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

    /// Initializes the database at the given path
    pub fn init_with_path(&self, db_path: &std::path::Path) -> Result<(), AppError> {
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

/// Batch add/remove/replace tags on multiple products at once.
/// mode: "add" (default), "remove", or "replace".
#[tauri::command]
pub fn batch_update_tags(
    ids: Vec<i64>,
    tags: Vec<String>,
    mode: Option<String>,
    db_state: State<DbState>,
) -> ApiResponse<usize> {
    let m = mode.as_deref().unwrap_or("add");
    info!("batch_update_tags: {} products, mode={}, {} tags", ids.len(), m, tags.len());
    with_db(&db_state, |db| db.batch_update_tags(&ids, &tags, m))
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

/// Paginated response for library products.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedProducts {
    pub items: Vec<Product>,
    pub total: i64,
}

/// Lists library products with server-side pagination and filtering.
#[tauri::command]
pub fn list_library_products_paginated(
    limit: Option<i64>,
    offset: Option<i64>,
    search_query: Option<String>,
    library_filter: Option<String>,
    category_filter: Option<String>,
    type_filter: Option<String>,
    vendor_filter: Option<String>,
    sort_by: Option<String>,
    collection_id: Option<i64>,
    db_state: State<DbState>,
) -> ApiResponse<PaginatedProducts> {
    let limit = limit.unwrap_or(50).min(200).max(1);
    let offset = offset.unwrap_or(0).max(0);

    with_db(&db_state, |db| {
        let (items, total) = db.list_library_products_paginated(
            limit,
            offset,
            search_query.as_deref(),
            library_filter.as_deref(),
            category_filter.as_deref(),
            type_filter.as_deref(),
            vendor_filter.as_deref(),
            sort_by.as_deref(),
            collection_id,
        )?;
        Ok(PaginatedProducts { items, total })
    })
}

/// Returns a sorted list of distinct vendor names across all library products.
#[tauri::command]
pub fn list_product_vendors(db_state: State<DbState>) -> ApiResponse<Vec<String>> {
    with_db(&db_state, |db| db.list_distinct_vendors())
}

/// Returns aggregate statistics for the library dashboard.
#[tauri::command]
pub fn get_library_stats(db_state: State<DbState>) -> ApiResponse<LibraryStats> {
    with_db(&db_state, |db| db.get_library_stats())
}

/// Finds duplicate products (same name + vendor) in the library.
#[tauri::command]
pub fn find_duplicates(db_state: State<DbState>) -> ApiResponse<Vec<DuplicateGroup>> {
    with_db(&db_state, |db| db.find_duplicates())
}

/// Scans DAZ libraries and rebuilds the product catalog from metadata.
/// Returns immediately; the actual scan runs in a background task.
/// Emits standardized `app-task-start`, `app-task-progress`, and `app-task-end` events.
#[tauri::command]
pub async fn scan_library_products(
    library_path: Option<String>,
    resource_profile: Option<String>,
    settings: State<'_, SettingsState>,
    app: tauri::AppHandle,
) -> Result<ApiResponse<usize>, String> {
    let (db_path, thumbnails_dir, libraries) = match settings.read() {
        Ok(settings) => {
            let libs = match library_path {
                Some(path) => vec![PathBuf::from(path)],
                None => settings.daz_libraries.clone(),
            };
            (settings.database_path.clone(), settings.thumbnails_dir.clone(), libs)
        }
        Err(e) => {
            return Ok(ApiResponse::error(AppError::Config(format!(
                "Cannot read settings: {}",
                e
            ))))
        }
    };

    let task_id = Uuid::new_v4().to_string();

    // Emit task-start
    let _ = app.emit("app-task-start", TaskPayload {
        id: task_id.clone(),
        task_type: "scan".into(),
        message: "Scanning libraries…".into(),
        progress: None,
        status: "running".into(),
    });

    // Fire-and-forget: spawn a detached background task so the scan
    // continues even if the user navigates away from the Products tab.
    let task_id_inner = task_id.clone();
    let app_inner = app.clone();
    let profile = ResourceProfile::from_str_opt(resource_profile.as_deref());
    tauri::async_runtime::spawn(async move {
        let app_progress = app_inner.clone();
        let tid = task_id_inner.clone();
        let lib_count = libraries.len();

        let result = spawn_blocking(move || {
            let db = Database::open(&db_path)?;
            let mut total = 0usize;

            let ctx = ScanContext {
                app: app_progress,
                task_id: tid,
                lib_count,
                resource_profile: profile,
            };

            for (i, library) in libraries.iter().enumerate() {
                total += scan_library(&db, library, &thumbnails_dir, &ctx, i)?;
            }

            Ok::<_, AppError>(total)
        })
        .await;

        match result {
            Ok(Ok(count)) => {
                info!("Library scan complete: {} products indexed", count);
                let _ = app_inner.emit("app-task-end", TaskPayload {
                    id: task_id_inner,
                    task_type: "scan".into(),
                    message: format!("Scan complete — {} products indexed", count),
                    progress: Some(1.0),
                    status: "success".into(),
                });
            }
            Ok(Err(err)) => {
                error!("Library scan failed: {}", err);
                let _ = app_inner.emit("app-task-end", TaskPayload {
                    id: task_id_inner,
                    task_type: "scan".into(),
                    message: format!("Scan failed: {}", err),
                    progress: None,
                    status: "error".into(),
                });
            }
            Err(join_err) => {
                error!("Library scan task panicked: {}", join_err);
                let _ = app_inner.emit("app-task-end", TaskPayload {
                    id: task_id_inner,
                    task_type: "scan".into(),
                    message: format!("Scan crashed: {}", join_err),
                    progress: None,
                    status: "error".into(),
                });
            }
        }
    });

    // Return immediately — the scan runs in the background
    Ok(ApiResponse::success(0))
}

// ============================================================================
// Helper
// ============================================================================

/// Context passed through the scan pipeline for progress reporting.
struct ScanContext {
    app: tauri::AppHandle,
    task_id: String,
    lib_count: usize,
    resource_profile: ResourceProfile,
}

/// Controls how many CPU threads the scan uses.
#[derive(Debug, Clone, Copy)]
enum ResourceProfile {
    /// 1–2 threads — keeps the PC totally free
    Low,
    /// Smart scaling based on core count (default)
    Normal,
    /// Use (almost) all cores — maximum speed
    Max,
}

impl ResourceProfile {
    fn from_str_opt(s: Option<&str>) -> Self {
        match s.map(|v| v.to_lowercase()).as_deref() {
            Some("low") => Self::Low,
            Some("max") => Self::Max,
            _ => Self::Normal,
        }
    }

    /// Returns the number of Rayon threads to use.
    fn thread_count(self) -> usize {
        let cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);

        match self {
            Self::Low => cpus.min(2).max(1),
            Self::Normal => {
                if cpus <= 4 {
                    (cpus.saturating_sub(1)).max(2)
                } else {
                    (cpus.saturating_sub(2)).max(2)
                }
            }
            Self::Max => (cpus.saturating_sub(1)).max(1),
        }
    }
}

impl ScanContext {
    /// Emit a progress update. `lib_idx` is the current library index.
    /// `phase` is 0.0..1.0 within the current library.
    fn emit_progress(&self, lib_idx: usize, phase: f32, message: &str) {
        let lib_base = lib_idx as f32 / self.lib_count as f32;
        let lib_span = 1.0 / self.lib_count as f32;
        let overall = lib_base + lib_span * phase.clamp(0.0, 1.0);

        let _ = self.app.emit("app-task-progress", TaskPayload {
            id: self.task_id.clone(),
            task_type: "scan".into(),
            message: message.to_string(),
            progress: Some(overall.min(0.99)),
            status: "running".into(),
        });
    }
}

/// Executes an operation on the database
pub(crate) fn with_db<T, F>(db_state: &State<DbState>, f: F) -> ApiResponse<T>
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

fn scan_library(db: &Database, library_path: &Path, thumbnails_dir: &Path, ctx: &ScanContext, lib_idx: usize) -> Result<usize, AppError> {
    if !library_path.exists() {
        return Ok(0);
    }

    let lib_name = library_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("library");

    fs::create_dir_all(thumbnails_dir)?;

    // === Pass 1: Index products from DSX metadata files (fault-tolerant) ===
    ctx.emit_progress(lib_idx, 0.0, &format!("[{}] Listing metadata…", lib_name));
    let metadata_files = list_support_metadata_files(library_path)?;
    let meta_total = metadata_files.len();
    let mut indexed = 0usize;

    for (mi, metadata_path) in metadata_files.iter().enumerate() {
        // Throttle: emit progress every 50 files
        if mi % 50 == 0 {
            let phase = if meta_total > 0 { mi as f32 / meta_total as f32 * 0.5 } else { 0.0 };
            ctx.emit_progress(lib_idx, phase, &format!("[{}] Pass 1: {}/{} metadata", lib_name, mi, meta_total));
        }

        let products = match parse_daz_metadata_file(metadata_path) {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    "Skipping malformed metadata file {}: {}",
                    metadata_path.display(),
                    e
                );
                continue;
            }
        };
        if products.is_empty() {
            continue;
        }

        // DSX files usually contain a single product; index the first one.
        let Some(product) = products.first().cloned() else { continue; };

        let support_file = metadata_path
            .strip_prefix(library_path)
            .unwrap_or(metadata_path)
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
            thumbnail_path: thumbnail_path.map(|p| p.to_string_lossy().replace('\\', "/")),
            files_count: 0,
            total_size: 0,
        };

        db.upsert_library_product(&input)?;
        indexed += 1;
    }

    info!(
        "Pass 1 complete: indexed {} products from DSX metadata in {}",
        indexed,
        library_path.display()
    );

    // === Pass 2: Scan for orphan .duf files not tracked by any product ===
    ctx.emit_progress(lib_idx, 0.5, &format!("[{}] Pass 2: scanning orphans…", lib_name));
    let num_threads = ctx.resource_profile.thread_count();
    let orphan_count = crate::core::orphan_scanner::scan_orphan_dufs(
        db,
        library_path,
        num_threads,
        &|phase, message| ctx.emit_progress(lib_idx, phase, message),
    )?;
    if orphan_count > 0 {
        info!(
            "Pass 2 complete: created {} custom products from orphan .duf files in {}",
            orphan_count,
            library_path.display()
        );
    }

    Ok(indexed + orphan_count)
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

// ============================================================================
// Uninstaller & Integrity
// ============================================================================

/// Uninstalls a product: deletes files from disk + removes from DB.
/// With `dry_run = true`, only reports what *would* be deleted.
#[tauri::command]
pub fn uninstall_product(
    id: i64,
    dry_run: bool,
    db_state: State<DbState>,
) -> ApiResponse<UninstallReport> {
    info!("uninstall_product: id={}, dry_run={}", id, dry_run);

    let guard = match db_state.0.lock() {
        Ok(g) => g,
        Err(_) => return ApiResponse::error_msg("DB_ERROR", "Database lock poisoned"),
    };
    let db = match guard.as_ref() {
        Some(db) => db,
        None => return ApiResponse::error_msg("DB_ERROR", "Database not initialized"),
    };

    // 1. Get product
    let product = match db.get_product(id) {
        Ok(Some(p)) => p,
        Ok(None) => return ApiResponse::error_msg("NOT_FOUND", "Product not found"),
        Err(e) => return ApiResponse::error(e),
    };

    // 2. Get tracked file list
    let tracked_files = match db.with_connection(|conn| {
        crate::db::product_files::get_product_files(conn, id)
    }) {
        Ok(f) => f,
        Err(e) => return ApiResponse::error(e),
    };

    // 3. Resolve absolute paths from library_path + relative_path
    let library_path = product.library_path.as_deref().unwrap_or("");

    let mut files_found = 0usize;
    let mut files_deleted = 0usize;
    let mut files_missing = 0usize;
    let mut bytes_freed = 0u64;
    let mut errors: Vec<String> = Vec::new();
    let mut dirs_to_check: std::collections::BTreeSet<PathBuf> = std::collections::BTreeSet::new();

    for pf in &tracked_files {
        let abs = Path::new(library_path).join(&pf.relative_path);
        if abs.exists() {
            files_found += 1;
            let size = std::fs::metadata(&abs).map(|m| m.len()).unwrap_or(0);
            bytes_freed += size;

            if !dry_run {
                match std::fs::remove_file(&abs) {
                    Ok(_) => {
                        files_deleted += 1;
                        // Track parent dir for empty-dir cleanup
                        if let Some(parent) = abs.parent() {
                            dirs_to_check.insert(parent.to_path_buf());
                        }
                    }
                    Err(e) => {
                        errors.push(format!("{}: {}", abs.display(), e));
                    }
                }
            }
        } else {
            files_missing += 1;
        }
    }

    // 4. Clean up empty directories (bottom-up)
    if !dry_run {
        // Sort deepest first
        let mut dirs: Vec<PathBuf> = dirs_to_check.into_iter().collect();
        dirs.sort_by(|a, b| b.components().count().cmp(&a.components().count()));

        let lib_root = Path::new(library_path);
        for dir in dirs {
            // Walk upward until library root, removing empty dirs
            let mut current = dir;
            while current.starts_with(lib_root) && current != lib_root {
                if current.is_dir() {
                    match std::fs::read_dir(&current) {
                        Ok(mut entries) => {
                            if entries.next().is_none() {
                                let _ = std::fs::remove_dir(&current);
                            } else {
                                break; // not empty, stop climbing
                            }
                        }
                        Err(_) => break,
                    }
                }
                match current.parent() {
                    Some(p) => current = p.to_path_buf(),
                    None => break,
                }
            }
        }

        // 5. Delete from database (CASCADE handles product_files, collection_items)
        if let Err(e) = db.with_connection(|conn| {
            conn.execute("DELETE FROM products WHERE id = ?1", [id])?;
            Ok(())
        }) {
            errors.push(format!("DB cleanup: {}", e));
        }

        info!(
            "Uninstalled product '{}' (id={}): {} files deleted, {} bytes freed, {} errors",
            product.name, id, files_deleted, bytes_freed, errors.len()
        );
    }

    ApiResponse::success(UninstallReport {
        product_id: id,
        product_name: product.name,
        files_found,
        files_deleted,
        files_missing,
        bytes_freed,
        errors,
        dry_run,
    })
}

/// Checks the integrity of a product's files on disk.
#[tauri::command]
pub fn check_product_integrity(
    id: i64,
    db_state: State<DbState>,
) -> ApiResponse<IntegrityReport> {
    let guard = match db_state.0.lock() {
        Ok(g) => g,
        Err(_) => return ApiResponse::error_msg("DB_ERROR", "Database lock poisoned"),
    };
    let db = match guard.as_ref() {
        Some(db) => db,
        None => return ApiResponse::error_msg("DB_ERROR", "Database not initialized"),
    };

    let product = match db.get_product(id) {
        Ok(Some(p)) => p,
        Ok(None) => return ApiResponse::error_msg("NOT_FOUND", "Product not found"),
        Err(e) => return ApiResponse::error(e),
    };

    let tracked_files = match db.with_connection(|conn| {
        crate::db::product_files::get_product_files(conn, id)
    }) {
        Ok(f) => f,
        Err(e) => return ApiResponse::error(e),
    };

    let library_path = product.library_path.as_deref().unwrap_or("");
    let total_files = tracked_files.len();
    let mut files_present = 0usize;
    let mut missing_paths: Vec<String> = Vec::new();

    for pf in &tracked_files {
        let abs = Path::new(library_path).join(&pf.relative_path);
        if abs.exists() {
            files_present += 1;
        } else {
            missing_paths.push(pf.relative_path.clone());
        }
    }

    let files_missing = total_files - files_present;
    let integrity_pct = if total_files == 0 {
        100.0
    } else {
        (files_present as f64 / total_files as f64) * 100.0
    };

    ApiResponse::success(IntegrityReport {
        product_id: id,
        total_files,
        files_present,
        files_missing,
        integrity_pct,
        missing_paths,
    })
}

// ============================================================================
// Scene Analyzer
// ============================================================================

/// Analyzes a `.duf` scene file and identifies which products it requires.
#[tauri::command]
pub fn analyze_scene(
    file_path: String,
    db_state: State<DbState>,
    settings: State<'_, SettingsState>,
) -> ApiResponse<SceneAnalysisReport> {
    info!("analyze_scene: {}", file_path);

    let duf_path = Path::new(&file_path);
    if !duf_path.exists() {
        return ApiResponse::error_msg("NOT_FOUND", "Scene file not found");
    }

    let guard = match db_state.0.lock() {
        Ok(g) => g,
        Err(_) => return ApiResponse::error_msg("DB_ERROR", "Database lock poisoned"),
    };
    let db = match guard.as_ref() {
        Some(db) => db,
        None => return ApiResponse::error_msg("DB_ERROR", "Database not initialized"),
    };

    // Gather all library paths from settings
    let library_paths: Vec<String> = match settings.read() {
        Ok(settings) => settings.daz_libraries.iter()
            .filter_map(|p| p.to_str().map(|s| s.to_string()))
            .collect(),
        Err(_) => Vec::new(),
    };

    match db.with_connection(|conn| {
        crate::core::scene_analyzer::analyze_scene(duf_path, conn, &library_paths)
    }) {
        Ok(report) => ApiResponse::success(report),
        Err(e) => ApiResponse::error(e),
    }
}

// ============================================================================
// Product Files (Manifest tracking)
// ============================================================================

/// Get all tracked files for a product (from Manifest.dsx).
#[tauri::command]
pub fn get_product_files(
    id: i64,
    db_state: State<DbState>,
) -> ApiResponse<Vec<crate::db::product_files::ProductFile>> {
    let guard = match db_state.0.lock() {
        Ok(g) => g,
        Err(_) => return ApiResponse::error_msg("DB_ERROR", "Database lock poisoned"),
    };
    let db = match guard.as_ref() {
        Some(db) => db,
        None => return ApiResponse::error_msg("DB_ERROR", "Database not initialized"),
    };

    match db.with_connection(|conn| crate::db::product_files::get_product_files(conn, id)) {
        Ok(files) => ApiResponse::success(files),
        Err(e) => ApiResponse::error_msg("QUERY_ERROR", &e.to_string()),
    }
}

/// Check if installing files would conflict with existing products.
#[tauri::command]
pub fn check_file_conflicts(
    file_paths: Vec<String>,
    exclude_product_id: Option<i64>,
    db_state: State<DbState>,
) -> ApiResponse<Vec<crate::db::product_files::FileConflict>> {
    let guard = match db_state.0.lock() {
        Ok(g) => g,
        Err(_) => return ApiResponse::error_msg("DB_ERROR", "Database lock poisoned"),
    };
    let db = match guard.as_ref() {
        Some(db) => db,
        None => return ApiResponse::error_msg("DB_ERROR", "Database not initialized"),
    };

    match db.with_connection(|conn| {
        crate::db::product_files::check_file_conflicts(conn, &file_paths, exclude_product_id)
    }) {
        Ok(conflicts) => ApiResponse::success(conflicts),
        Err(e) => ApiResponse::error_msg("QUERY_ERROR", &e.to_string()),
    }
}
