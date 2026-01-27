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
