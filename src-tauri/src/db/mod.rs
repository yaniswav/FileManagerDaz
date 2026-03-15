//! # Database Layer
//!
//! SQLite database access for FileManagerDaz.
//!
//! ## Modules
//!
//! - [`bundles`]: Installed bundle tracking (archive hashes, file lists)
//! - [`import_tasks`]: Import task persistence for retry/resume
//! - [`models`]: Data models for products
//! - [`repository`]: Product catalog CRUD operations
//!
//! ## Databases
//!
//! The application uses two separate SQLite databases:
//! - **Products database**: Product catalog (`products.db`)
//! - **Import tasks database**: Import history (`import_tasks.db`)

pub mod bundles;
pub mod import_tasks;
pub mod models;
pub mod product_files;
pub mod repository;

pub use models::{Collection, DuplicateGroup, InstalledAsset, IntegrityReport, LibraryProductInput, LibraryStats, NewProduct, Product, RequiredProduct, SceneAnalysisReport, TypeCount, UninstallReport, UpdateProduct, VendorCount};
pub use repository::Database;
