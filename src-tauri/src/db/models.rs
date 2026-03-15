//! Data models for the SQLite database

use serde::{Deserialize, Serialize};

/// Installed DAZ product
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    /// Unique ID (auto-incremented)
    pub id: i64,
    /// Product name
    pub name: String,
    /// Installation path
    pub path: String,
    /// Product origin ("import" or "library")
    pub origin: Option<String>,
    /// Library root path (for library catalog entries)
    pub library_path: Option<String>,
    /// Runtime/Support metadata file (relative to library)
    pub support_file: Option<String>,
    /// Product token/SKU (from DSX metadata)
    pub product_token: Option<String>,
    /// Global ID (from DSX metadata)
    pub global_id: Option<String>,
    /// Vendor or artist name
    pub vendor: Option<String>,
    /// Source archive path (optional)
    pub source_archive: Option<String>,
    /// Content type (Character, Prop, etc.)
    pub content_type: Option<String>,
    /// DAZ category hierarchy paths
    pub categories: Vec<String>,
    /// Cached thumbnail path
    pub thumbnail_path: Option<String>,
    /// Installation date (ISO 8601)
    pub installed_at: String,
    /// Tags (comma-separated)
    pub tags: String,
    /// User notes
    pub notes: Option<String>,
    /// Number of installed files
    pub files_count: i64,
    /// Total size (bytes)
    pub total_size: i64,
}

/// New product to insert (without ID)
#[derive(Debug, Clone, Deserialize)]
pub struct NewProduct {
    pub name: String,
    pub path: String,
    /// Optional link to an import task ID (for idempotency/backfill).
    pub import_task_id: Option<String>,
    pub source_archive: Option<String>,
    pub content_type: Option<String>,
    /// DAZ global product ID (from Manifest.dsx)
    pub global_id: Option<String>,
    /// Vendor/artist name (from DSX metadata)
    pub vendor: Option<String>,
    /// Optional installed date override (ISO 8601).
    pub installed_at: Option<String>,
    pub tags: String,
    pub files_count: i64,
    pub total_size: i64,
}

impl NewProduct {
    pub fn new(name: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            import_task_id: None,
            source_archive: None,
            content_type: None,
            global_id: None,
            vendor: None,
            installed_at: None,
            tags: String::new(),
            files_count: 0,
            total_size: 0,
        }
    }

    pub fn with_import_task_id(mut self, import_task_id: impl Into<String>) -> Self {
        self.import_task_id = Some(import_task_id.into());
        self
    }

    pub fn with_tags(mut self, tags: impl Into<String>) -> Self {
        self.tags = tags.into();
        self
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source_archive = Some(source.into());
        self
    }

    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    pub fn with_installed_at(mut self, installed_at: impl Into<String>) -> Self {
        self.installed_at = Some(installed_at.into());
        self
    }

    pub fn with_stats(mut self, files_count: i64, total_size: i64) -> Self {
        self.files_count = files_count;
        self.total_size = total_size;
        self
    }

    pub fn with_global_id(mut self, global_id: impl Into<String>) -> Self {
        self.global_id = Some(global_id.into());
        self
    }

    pub fn with_vendor(mut self, vendor: impl Into<String>) -> Self {
        self.vendor = Some(vendor.into());
        self
    }
}

/// Data for product update
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateProduct {
    pub name: Option<String>,
    pub tags: Option<String>,
    pub content_type: Option<String>,
    pub notes: Option<String>,
}

/// Input data for indexing a product from a library metadata file.
#[derive(Debug, Clone)]
pub struct LibraryProductInput {
    pub name: String,
    pub path: String,
    pub library_path: String,
    pub support_file: String,
    pub product_token: Option<String>,
    pub global_id: Option<String>,
    pub vendor: Option<String>,
    pub categories: Vec<String>,
    pub content_type: Option<String>,
    pub installed_at: String,
    pub thumbnail_path: Option<String>,
    pub files_count: i64,
    pub total_size: i64,
}

// ============================================================================
// Library Stats
// ============================================================================

/// Aggregate statistics for the library dashboard.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStats {
    pub total_products: i64,
    pub total_size_bytes: i64,
    pub products_by_type: Vec<TypeCount>,
    pub top_vendors: Vec<VendorCount>,
    pub recent_products: Vec<Product>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeCount {
    pub content_type: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VendorCount {
    pub vendor: String,
    pub count: i64,
}

/// A group of duplicate products (same name + vendor).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    pub name: String,
    pub vendor: Option<String>,
    pub count: i64,
    pub products: Vec<Product>,
}

// ============================================================================
// Collections
// ============================================================================

/// A user-defined collection (playlist) of products.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub item_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

// ============================================================================
// Maintenance / Uninstaller
// ============================================================================

/// Report returned by uninstall_product (dry-run or real).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallReport {
    pub product_id: i64,
    pub product_name: String,
    pub files_found: usize,
    pub files_deleted: usize,
    pub files_missing: usize,
    pub bytes_freed: u64,
    pub errors: Vec<String>,
    pub dry_run: bool,
}

/// Result of a product integrity check.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityReport {
    pub product_id: i64,
    pub total_files: usize,
    pub files_present: usize,
    pub files_missing: usize,
    /// 0.0 .. 100.0
    pub integrity_pct: f64,
    pub missing_paths: Vec<String>,
}

// ============================================================================
// Scene Analysis
// ============================================================================

/// An installed asset found in the scene, grouped by product.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledAsset {
    pub relative_path: String,
    pub product_id: i64,
    pub product_name: String,
}

/// Result of analyzing a .duf scene file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneAnalysisReport {
    pub scene_name: String,
    pub total_dependencies: usize,
    pub installed_count: usize,
    pub missing_count: usize,
    /// 0.0 .. 100.0
    pub completion_pct: f64,
    /// Assets matched to installed products (grouped by product).
    pub installed_assets: Vec<InstalledAsset>,
    /// File paths that exist on disk but are not tracked by any product.
    pub untracked_assets: Vec<String>,
    /// File paths that are completely missing (not on disk, not in DB).
    pub missing_assets: Vec<String>,
    /// Products required by this scene (deduplicated).
    pub required_products: Vec<RequiredProduct>,
}

/// A product required by the scene.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequiredProduct {
    pub product_id: i64,
    pub product_name: String,
    pub files_used: usize,
}
