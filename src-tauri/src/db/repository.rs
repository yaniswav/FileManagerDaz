//! Repository for CRUD operations on the database

use crate::db::models::{LibraryProductInput, NewProduct, Product, UpdateProduct};
use crate::error::{AppError, AppResult};
use chrono::Utc;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;
use tracing::{debug, info, warn};
use serde_json;

const PRODUCT_SELECT_FIELDS: &str = "id, name, path, origin, library_path, support_file, product_token, global_id, vendor, source_archive, content_type, categories, thumbnail_path, installed_at, tags, notes, files_count, total_size";

/// Database repository
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Opens or creates the database
    pub fn open(db_path: &Path) -> AppResult<Self> {
        info!("Opening database: {:?}", db_path);

        // Create parent directory if needed
        if let Some(parent) = db_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let conn = Connection::open(db_path)?;
        let db = Self {
            conn: Mutex::new(conn),
        };

        db.do_initialize()?;
        db.do_migrate()?;
        Ok(db)
    }

    /// Opens an in-memory database (for tests)
    #[allow(dead_code)]
    pub fn open_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.do_initialize()?;
        Ok(db)
    }

    // ========================================================================
    // Centralized connection access
    // ========================================================================

    /// Executes a closure with access to the DB connection.
    /// Properly handles poisoned lock without panic.
    pub fn with_connection<T, F>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce(&Connection) -> AppResult<T>,
    {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::Database("Database lock poisoned".into()))?;
        f(&conn)
    }

    // ========================================================================
    // Initialization and migration (internal)
    // ========================================================================

    /// Initializes the database schema
    fn do_initialize(&self) -> AppResult<()> {
        self.with_connection(|conn| {
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS products (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL,
                    path TEXT NOT NULL,
                    origin TEXT NOT NULL DEFAULT 'import',
                    library_path TEXT,
                    support_file TEXT,
                    product_token TEXT,
                    global_id TEXT,
                    vendor TEXT,
                    import_task_id TEXT,
                    source_archive TEXT,
                    content_type TEXT,
                    categories TEXT DEFAULT '[]',
                    thumbnail_path TEXT,
                    installed_at TEXT NOT NULL,
                    tags TEXT DEFAULT '',
                    notes TEXT,
                    files_count INTEGER DEFAULT 0,
                    total_size INTEGER DEFAULT 0
                );

                CREATE INDEX IF NOT EXISTS idx_products_name ON products(name);
                CREATE INDEX IF NOT EXISTS idx_products_path ON products(path);
                "#,
            )?;

            // Initialize bundles table
            crate::db::bundles::init_bundles_table(conn)?;

            debug!("Database schema initialized");
            Ok(())
        })
    }

    /// Migrates the schema if necessary
    fn do_migrate(&self) -> AppResult<()> {
        self.with_connection(|conn| {
            // Check if new columns exist
            let mut columns: Vec<String> = conn
                .prepare("PRAGMA table_info(products)")?
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()?;
            let mut refresh_columns = false;

            // Add origin if missing
            if !columns.iter().any(|c| c == "origin") {
                warn!("Migrating database: adding origin column");
                conn.execute(
                    "ALTER TABLE products ADD COLUMN origin TEXT NOT NULL DEFAULT 'import'",
                    [],
                )?;
                refresh_columns = true;
            }

            // Add library_path if missing
            if !columns.iter().any(|c| c == "library_path") {
                warn!("Migrating database: adding library_path column");
                conn.execute("ALTER TABLE products ADD COLUMN library_path TEXT", [])?;
                refresh_columns = true;
            }

            // Add support_file if missing
            if !columns.iter().any(|c| c == "support_file") {
                warn!("Migrating database: adding support_file column");
                conn.execute("ALTER TABLE products ADD COLUMN support_file TEXT", [])?;
                refresh_columns = true;
            }

            // Add product_token if missing
            if !columns.iter().any(|c| c == "product_token") {
                warn!("Migrating database: adding product_token column");
                conn.execute("ALTER TABLE products ADD COLUMN product_token TEXT", [])?;
                refresh_columns = true;
            }

            // Add global_id if missing
            if !columns.iter().any(|c| c == "global_id") {
                warn!("Migrating database: adding global_id column");
                conn.execute("ALTER TABLE products ADD COLUMN global_id TEXT", [])?;
                refresh_columns = true;
            }

            // Add vendor if missing
            if !columns.iter().any(|c| c == "vendor") {
                warn!("Migrating database: adding vendor column");
                conn.execute("ALTER TABLE products ADD COLUMN vendor TEXT", [])?;
                refresh_columns = true;
            }

            // Add import_task_id if missing
            if !columns.iter().any(|c| c == "import_task_id") {
                warn!("Migrating database: adding import_task_id column");
                conn.execute("ALTER TABLE products ADD COLUMN import_task_id TEXT", [])?;
                refresh_columns = true;
            }

            // Add content_type if missing
            if !columns.iter().any(|c| c == "content_type") {
                warn!("Migrating database: adding content_type column");
                conn.execute("ALTER TABLE products ADD COLUMN content_type TEXT", [])?;
                refresh_columns = true;
            }

            // Add categories if missing
            if !columns.iter().any(|c| c == "categories") {
                warn!("Migrating database: adding categories column");
                conn.execute(
                    "ALTER TABLE products ADD COLUMN categories TEXT DEFAULT '[]'",
                    [],
                )?;
                refresh_columns = true;
            }

            // Add thumbnail_path if missing
            if !columns.iter().any(|c| c == "thumbnail_path") {
                warn!("Migrating database: adding thumbnail_path column");
                conn.execute("ALTER TABLE products ADD COLUMN thumbnail_path TEXT", [])?;
                refresh_columns = true;
            }

            // Add notes if missing
            if !columns.iter().any(|c| c == "notes") {
                warn!("Migrating database: adding notes column");
                conn.execute("ALTER TABLE products ADD COLUMN notes TEXT", [])?;
                refresh_columns = true;
            }

            if refresh_columns {
                columns = conn
                    .prepare("PRAGMA table_info(products)")?
                    .query_map([], |row| row.get::<_, String>(1))?
                    .collect::<Result<Vec<_>, _>>()?;
            }

            // Create index on content_type
            if columns.iter().any(|c| c == "content_type") {
                conn.execute(
                    "CREATE INDEX IF NOT EXISTS idx_products_content_type ON products(content_type)",
                    [],
                )?;
            }

            // Create index on origin
            if columns.iter().any(|c| c == "origin") {
                conn.execute(
                    "CREATE INDEX IF NOT EXISTS idx_products_origin ON products(origin)",
                    [],
                )?;
            }

            // Create index on library_path
            if columns.iter().any(|c| c == "library_path") {
                conn.execute(
                    "CREATE INDEX IF NOT EXISTS idx_products_library_path ON products(library_path)",
                    [],
                )?;
            }

            // Create index on support_file
            if columns.iter().any(|c| c == "support_file") {
                conn.execute(
                    "CREATE INDEX IF NOT EXISTS idx_products_support_file ON products(support_file)",
                    [],
                )?;
            }

            // Create unique index for idempotent inserts from import tasks
            if columns.iter().any(|c| c == "import_task_id") {
                conn.execute(
                    "CREATE UNIQUE INDEX IF NOT EXISTS idx_products_import_task_id ON products(import_task_id)",
                    [],
                )?;
            } else {
                warn!("Skipping import_task_id index (column missing after migration)");
            }

            // Create unique index for library products (one entry per library + support file)
            if columns.iter().any(|c| c == "library_path") && columns.iter().any(|c| c == "support_file") {
                conn.execute(
                    "CREATE UNIQUE INDEX IF NOT EXISTS idx_products_library_support ON products(library_path, support_file)",
                    [],
                )?;
            } else {
                warn!("Skipping library_support index (columns missing after migration)");
            }

            debug!("Database migration completed");
            Ok(())
        })
    }

    // ========================================================================
    // CRUD Operations
    // ========================================================================

    /// Adds a new product
    pub fn add_product(&self, product: &NewProduct) -> AppResult<i64> {
        self.with_connection(|conn| {
            let installed_at = product
                .installed_at
                .clone()
                .unwrap_or_else(|| Utc::now().to_rfc3339());

            conn.execute(
                r#"
                INSERT INTO products (name, path, import_task_id, source_archive, content_type, installed_at, tags, files_count, total_size)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                "#,
                params![
                    product.name,
                    product.path,
                    product.import_task_id,
                    product.source_archive,
                    product.content_type,
                    installed_at,
                    product.tags,
                    product.files_count,
                    product.total_size,
                ],
            )?;

            let id = conn.last_insert_rowid();
            info!("Added product: {} (id={})", product.name, id);
            Ok(id)
        })
    }

    /// Updates an existing product
    pub fn update_product(&self, id: i64, update: &UpdateProduct) -> AppResult<bool> {
        self.with_connection(|conn| {
            // Dynamically build the UPDATE query
            let mut updates = Vec::new();
            let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

            if let Some(ref name) = update.name {
                updates.push("name = ?");
                values.push(Box::new(name.clone()));
            }
            if let Some(ref tags) = update.tags {
                updates.push("tags = ?");
                values.push(Box::new(tags.clone()));
            }
            if let Some(ref content_type) = update.content_type {
                updates.push("content_type = ?");
                values.push(Box::new(content_type.clone()));
            }
            if let Some(ref notes) = update.notes {
                updates.push("notes = ?");
                values.push(Box::new(notes.clone()));
            }

            if updates.is_empty() {
                return Ok(false);
            }

            // Add ID at the end
            values.push(Box::new(id));

            let sql = format!("UPDATE products SET {} WHERE id = ?", updates.join(", "));

            let params: Vec<&dyn rusqlite::ToSql> = values.iter().map(|v| v.as_ref()).collect();
            let rows = conn.execute(&sql, params.as_slice())?;

            info!(
                "Updated product id={}: {} fields modified",
                id,
                updates.len()
            );
            Ok(rows > 0)
        })
    }

    /// Lists all products
    pub fn list_products(&self) -> AppResult<Vec<Product>> {
        self.with_connection(|conn| {
            let sql = format!(
                "SELECT {} FROM products ORDER BY installed_at DESC",
                PRODUCT_SELECT_FIELDS
            );
            let mut stmt = conn.prepare(&sql)?;

            let products = stmt
                .query_map([], Self::map_row)?
                .collect::<Result<Vec<_>, _>>()?;

            debug!("Listed {} products", products.len());
            Ok(products)
        })
    }

    /// Gets a product by ID
    pub fn get_product(&self, id: i64) -> AppResult<Option<Product>> {
        self.with_connection(|conn| {
            let sql = format!(
                "SELECT {} FROM products WHERE id = ?1",
                PRODUCT_SELECT_FIELDS
            );
            let mut stmt = conn.prepare(&sql)?;

            let product = stmt.query_row([id], Self::map_row).optional()?;

            Ok(product)
        })
    }

    /// Gets a product by linked import task ID (if present).
    pub fn get_product_by_import_task_id(
        &self,
        import_task_id: &str,
    ) -> AppResult<Option<Product>> {
        self.with_connection(|conn| {
            let sql = format!(
                "SELECT {} FROM products WHERE import_task_id = ?1",
                PRODUCT_SELECT_FIELDS
            );
            let mut stmt = conn.prepare(&sql)?;

            let product = stmt.query_row([import_task_id], Self::map_row).optional()?;
            Ok(product)
        })
    }

    /// Deletes a product by ID.
    pub fn delete_product(&self, id: i64) -> AppResult<bool> {
        self.with_connection(|conn| {
            let rows_affected = conn.execute("DELETE FROM products WHERE id = ?1", [id])?;
            info!("Deleted product id={}: {}", id, rows_affected > 0);
            Ok(rows_affected > 0)
        })
    }

    /// Searches products by name or tags
    pub fn search_products(&self, query: &str) -> AppResult<Vec<Product>> {
        self.with_connection(|conn| {
            let pattern = format!("%{}%", query);
            let sql = format!(
                "SELECT {} FROM products WHERE name LIKE ?1 OR tags LIKE ?1 OR content_type LIKE ?1 OR vendor LIKE ?1 ORDER BY installed_at DESC",
                PRODUCT_SELECT_FIELDS
            );
            let mut stmt = conn.prepare(&sql)?;

            let products = stmt
                .query_map([pattern], Self::map_row)?
                .collect::<Result<Vec<_>, _>>()?;

            debug!("Search '{}' found {} products", query, products.len());
            Ok(products)
        })
    }

    /// Lists products indexed from DAZ libraries.
    pub fn list_library_products(&self) -> AppResult<Vec<Product>> {
        self.with_connection(|conn| {
            let sql = format!(
                "SELECT {} FROM products WHERE origin = 'library' ORDER BY name COLLATE NOCASE",
                PRODUCT_SELECT_FIELDS
            );
            let mut stmt = conn.prepare(&sql)?;

            let products = stmt
                .query_map([], Self::map_row)?
                .collect::<Result<Vec<_>, _>>()?;

            Ok(products)
        })
    }

    /// Searches products indexed from DAZ libraries.
    pub fn search_library_products(&self, query: &str) -> AppResult<Vec<Product>> {
        self.with_connection(|conn| {
            let pattern = format!("%{}%", query);
            let sql = format!(
                "SELECT {} FROM products WHERE origin = 'library' AND (name LIKE ?1 OR tags LIKE ?1 OR content_type LIKE ?1 OR vendor LIKE ?1 OR categories LIKE ?1) ORDER BY name COLLATE NOCASE",
                PRODUCT_SELECT_FIELDS
            );
            let mut stmt = conn.prepare(&sql)?;

            let products = stmt
                .query_map([pattern], Self::map_row)?
                .collect::<Result<Vec<_>, _>>()?;

            Ok(products)
        })
    }

    /// Inserts or updates a library product, preserving user tags/notes when present.
    pub fn upsert_library_product(&self, input: &LibraryProductInput) -> AppResult<i64> {
        self.with_connection(|conn| {
            let existing = conn
                .prepare(
                    "SELECT id, tags, notes FROM products WHERE origin = 'library' AND library_path = ?1 AND support_file = ?2",
                )?
                .query_row(
                    params![input.library_path, input.support_file],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?)),
                )
                .optional()?;

            let categories_json = serde_json::to_string(&input.categories).unwrap_or_else(|_| "[]".to_string());

            if let Some((id, existing_tags, existing_notes)) = existing {
                conn.execute(
                    r#"
                    UPDATE products
                    SET name = ?1,
                        path = ?2,
                        library_path = ?3,
                        support_file = ?4,
                        product_token = ?5,
                        global_id = ?6,
                        vendor = ?7,
                        content_type = ?8,
                        categories = ?9,
                        thumbnail_path = ?10,
                        installed_at = ?11,
                        tags = ?12,
                        notes = ?13,
                        files_count = ?14,
                        total_size = ?15,
                        origin = 'library',
                        source_archive = NULL,
                        import_task_id = NULL
                    WHERE id = ?16
                    "#,
                    params![
                        input.name,
                        input.path,
                        input.library_path,
                        input.support_file,
                        input.product_token,
                        input.global_id,
                        input.vendor,
                        input.content_type,
                        categories_json,
                        input.thumbnail_path,
                        input.installed_at,
                        existing_tags,
                        existing_notes,
                        input.files_count,
                        input.total_size,
                        id,
                    ],
                )?;
                Ok(id)
            } else {
                conn.execute(
                    r#"
                    INSERT INTO products (
                        name, path, origin, library_path, support_file, product_token,
                        global_id, vendor, content_type, categories, thumbnail_path,
                        installed_at, tags, notes, files_count, total_size
                    )
                    VALUES (?1, ?2, 'library', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, '', NULL, ?12, ?13)
                    "#,
                    params![
                        input.name,
                        input.path,
                        input.library_path,
                        input.support_file,
                        input.product_token,
                        input.global_id,
                        input.vendor,
                        input.content_type,
                        categories_json,
                        input.thumbnail_path,
                        input.installed_at,
                        input.files_count,
                        input.total_size,
                    ],
                )?;
                Ok(conn.last_insert_rowid())
            }
        })
    }

    /// Counts the number of products
    #[allow(dead_code)]
    pub fn count_products(&self) -> AppResult<i64> {
        self.with_connection(|conn| {
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM products", [], |row| row.get(0))?;
            Ok(count)
        })
    }

    /// Maps a SQL row to a Product
    fn map_row(row: &rusqlite::Row) -> Result<Product, rusqlite::Error> {
        let categories_json: Option<String> = row.get(11)?;
        let categories: Vec<String> = categories_json
            .as_deref()
            .and_then(|value| serde_json::from_str(value).ok())
            .unwrap_or_default();

        Ok(Product {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            origin: row.get(3)?,
            library_path: row.get(4)?,
            support_file: row.get(5)?,
            product_token: row.get(6)?,
            global_id: row.get(7)?,
            vendor: row.get(8)?,
            source_archive: row.get(9)?,
            content_type: row.get(10)?,
            categories,
            thumbnail_path: row.get(12)?,
            installed_at: row.get(13)?,
            tags: row.get(14)?,
            notes: row.get(15)?,
            files_count: row.get(16)?,
            total_size: row.get(17)?,
        })
    }
}

// Trait for rusqlite::OptionalExtension
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_list_products() {
        let db = Database::open_in_memory().unwrap();

        let new_product = NewProduct::new("Test Character", "/path/to/character")
            .with_tags("Character,Female")
            .with_content_type("Character")
            .with_stats(150, 1024000);

        let id = db.add_product(&new_product).unwrap();
        assert!(id > 0);

        let products = db.list_products().unwrap();
        assert_eq!(products.len(), 1);
        assert_eq!(products[0].name, "Test Character");
        assert_eq!(products[0].tags, "Character,Female");
        assert_eq!(products[0].content_type, Some("Character".to_string()));
    }

    #[test]
    fn test_update_product() {
        let db = Database::open_in_memory().unwrap();

        let new_product = NewProduct::new("Original Name", "/path");
        let id = db.add_product(&new_product).unwrap();

        let update = UpdateProduct {
            name: Some("Updated Name".to_string()),
            tags: Some("NewTag".to_string()),
            content_type: None,
            notes: Some("Test notes".to_string()),
        };

        assert!(db.update_product(id, &update).unwrap());

        let product = db.get_product(id).unwrap().unwrap();
        assert_eq!(product.name, "Updated Name");
        assert_eq!(product.tags, "NewTag");
        assert_eq!(product.notes, Some("Test notes".to_string()));
    }

    #[test]
    fn test_get_product() {
        let db = Database::open_in_memory().unwrap();

        let new_product = NewProduct::new("Test Prop", "/path/to/prop");
        let id = db.add_product(&new_product).unwrap();

        let product = db.get_product(id).unwrap();
        assert!(product.is_some());
        assert_eq!(product.unwrap().name, "Test Prop");

        let not_found = db.get_product(9999).unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn test_delete_product() {
        let db = Database::open_in_memory().unwrap();

        let new_product = NewProduct::new("To Delete", "/path");
        let id = db.add_product(&new_product).unwrap();

        assert!(db.delete_product(id).unwrap());
        assert!(db.get_product(id).unwrap().is_none());
    }

    #[test]
    fn test_search_products() {
        let db = Database::open_in_memory().unwrap();

        db.add_product(
            &NewProduct::new("Victoria 8", "/v8")
                .with_tags("Character,Female")
                .with_content_type("Character"),
        )
        .unwrap();
        db.add_product(
            &NewProduct::new("Michael 8", "/m8")
                .with_tags("Character,Male")
                .with_content_type("Character"),
        )
        .unwrap();
        db.add_product(
            &NewProduct::new("Sports Car", "/car")
                .with_tags("Vehicle")
                .with_content_type("Prop"),
        )
        .unwrap();

        let females = db.search_products("Female").unwrap();
        assert_eq!(females.len(), 1);
        assert_eq!(females[0].name, "Victoria 8");

        let characters = db.search_products("Character").unwrap();
        assert_eq!(characters.len(), 2);

        let props = db.search_products("Prop").unwrap();
        assert_eq!(props.len(), 1);
    }

    #[test]
    fn test_count_products() {
        let db = Database::open_in_memory().unwrap();

        assert_eq!(db.count_products().unwrap(), 0);

        db.add_product(&NewProduct::new("P1", "/p1")).unwrap();
        db.add_product(&NewProduct::new("P2", "/p2")).unwrap();

        assert_eq!(db.count_products().unwrap(), 2);
    }
}
