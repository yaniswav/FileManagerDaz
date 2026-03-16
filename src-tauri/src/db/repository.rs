//! Repository for CRUD operations on the database

use crate::db::models::{Collection, DuplicateGroup, LibraryProductInput, LibraryStats, NewProduct, Product, TypeCount, UpdateProduct, VendorCount};
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

        // Performance & safety pragmas
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -8000;
             PRAGMA busy_timeout = 5000;
             PRAGMA foreign_keys = ON;",
        )?;

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

            // Initialize product files table
            crate::db::product_files::init_product_files_table(conn)?;

            // Initialize collections tables
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS collections (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS collection_items (
                    collection_id INTEGER NOT NULL,
                    product_id INTEGER NOT NULL,
                    added_at TEXT NOT NULL,
                    PRIMARY KEY (collection_id, product_id),
                    FOREIGN KEY (collection_id) REFERENCES collections(id) ON DELETE CASCADE,
                    FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_collection_items_product ON collection_items(product_id);
                "#,
            )?;

            debug!("Database schema initialized");
            Ok(())
        })
    }

    /// Migrates the schema if necessary.
    /// All column additions run inside a single transaction (all-or-nothing).
    fn do_migrate(&self) -> AppResult<()> {
        self.with_connection(|conn| {
            let columns: Vec<String> = conn
                .prepare("PRAGMA table_info(products)")?
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()?;

            // Column migrations — iterated to avoid copy-paste
            let column_migrations: &[(&str, &str)] = &[
                ("origin",         "TEXT NOT NULL DEFAULT 'import'"),
                ("library_path",   "TEXT"),
                ("support_file",   "TEXT"),
                ("product_token",  "TEXT"),
                ("global_id",      "TEXT"),
                ("vendor",         "TEXT"),
                ("import_task_id", "TEXT"),
                ("content_type",   "TEXT"),
                ("categories",     "TEXT DEFAULT '[]'"),
                ("thumbnail_path", "TEXT"),
                ("notes",          "TEXT"),
            ];

            let pending: Vec<_> = column_migrations
                .iter()
                .filter(|(col, _)| !columns.iter().any(|c| c == col))
                .collect();

            if !pending.is_empty() {
                // Wrap all ALTER TABLEs in a transaction — prevents partial migration
                let tx = conn.unchecked_transaction()?;
                for (col, def) in &pending {
                    warn!("Migrating: adding column {col}");
                    tx.execute(
                        &format!("ALTER TABLE products ADD COLUMN {col} {def}"),
                        [],
                    )?;
                }
                tx.commit()?;
            }

            // Indexes — idempotent (CREATE IF NOT EXISTS), safe outside transaction
            conn.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_products_content_type ON products(content_type);
                 CREATE INDEX IF NOT EXISTS idx_products_origin ON products(origin);
                 CREATE INDEX IF NOT EXISTS idx_products_library_path ON products(library_path);
                 CREATE INDEX IF NOT EXISTS idx_products_support_file ON products(support_file);
                 CREATE INDEX IF NOT EXISTS idx_products_vendor ON products(vendor);
                 CREATE UNIQUE INDEX IF NOT EXISTS idx_products_import_task_id ON products(import_task_id);
                 CREATE UNIQUE INDEX IF NOT EXISTS idx_products_library_support ON products(library_path, support_file);",
            )?;

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
                INSERT INTO products (name, path, import_task_id, source_archive, content_type, global_id, vendor, installed_at, tags, files_count, total_size)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                "#,
                params![
                    product.name,
                    product.path,
                    product.import_task_id,
                    product.source_archive,
                    product.content_type,
                    product.global_id,
                    product.vendor,
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

    /// Batch update tags for multiple products in a single transaction.
    /// mode: "add" (merge), "remove" (subtract), or "replace" (overwrite).
    pub fn batch_update_tags(
        &self,
        ids: &[i64],
        tags: &[String],
        mode: &str,
    ) -> AppResult<usize> {
        if ids.is_empty() || (tags.is_empty() && mode != "replace") {
            return Ok(0);
        }
        self.with_connection(|conn| {
            let tx = conn.unchecked_transaction()?;

            // Pre-clean input tags
            let clean_tags: Vec<String> = tags
                .iter()
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect();

            let updated = match mode {
                "replace" => {
                    // All products get the same value — single UPDATE with IN clause
                    let new_tags = clean_tags.join(",");
                    let placeholders: Vec<String> = (0..ids.len())
                        .map(|i| format!("?{}", i + 2))
                        .collect();
                    let sql = format!(
                        "UPDATE products SET tags = ?1 WHERE id IN ({})",
                        placeholders.join(", ")
                    );
                    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> =
                        vec![Box::new(new_tags)];
                    for &id in ids {
                        params.push(Box::new(id));
                    }
                    tx.execute(
                        &sql,
                        rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
                    )?
                }
                _ => {
                    // "add" or "remove" — need per-product SELECT + UPDATE
                    let mut count = 0usize;
                    for &id in ids {
                        let new_tags = if mode == "remove" {
                            let current: String = tx
                                .query_row("SELECT COALESCE(tags, '') FROM products WHERE id = ?1", [id], |r| r.get(0))
                                .unwrap_or_default();
                            let remove_lower: Vec<String> = clean_tags.iter().map(|t| t.to_lowercase()).collect();
                            current
                                .split(',')
                                .map(|t| t.trim().to_string())
                                .filter(|t| !t.is_empty() && !remove_lower.contains(&t.to_lowercase()))
                                .collect::<Vec<_>>()
                                .join(",")
                        } else {
                            // "add" — merge without duplicates
                            let current: String = tx
                                .query_row("SELECT COALESCE(tags, '') FROM products WHERE id = ?1", [id], |r| r.get(0))
                                .unwrap_or_default();
                            let mut tag_set: Vec<String> = current
                                .split(',')
                                .map(|t| t.trim().to_string())
                                .filter(|t| !t.is_empty())
                                .collect();
                            for tag in &clean_tags {
                                if !tag_set.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                                    tag_set.push(tag.clone());
                                }
                            }
                            tag_set.join(",")
                        };

                        count += tx.execute(
                            "UPDATE products SET tags = ?1 WHERE id = ?2",
                            rusqlite::params![new_tags, id],
                        )?;
                    }
                    count
                }
            };

            tx.commit()?;
            info!("batch_update_tags: {} products updated (mode={}, {} tags)", updated, mode, clean_tags.len());
            Ok(updated)
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

    /// Lists all products.
    pub fn list_library_products(&self) -> AppResult<Vec<Product>> {
        self.with_connection(|conn| {
            let sql = format!(
                "SELECT {} FROM products ORDER BY name COLLATE NOCASE",
                PRODUCT_SELECT_FIELDS
            );
            let mut stmt = conn.prepare(&sql)?;

            let products = stmt
                .query_map([], Self::map_row)?
                .collect::<Result<Vec<_>, _>>()?;

            Ok(products)
        })
    }

    /// Searches products by text query.
    pub fn search_library_products(&self, query: &str) -> AppResult<Vec<Product>> {
        self.with_connection(|conn| {
            let pattern = format!("%{}%", query);
            let sql = format!(
                "SELECT {} FROM products WHERE (name LIKE ?1 OR tags LIKE ?1 OR content_type LIKE ?1 OR vendor LIKE ?1 OR categories LIKE ?1) ORDER BY name COLLATE NOCASE",
                PRODUCT_SELECT_FIELDS
            );
            let mut stmt = conn.prepare(&sql)?;

            let products = stmt
                .query_map([pattern], Self::map_row)?
                .collect::<Result<Vec<_>, _>>()?;

            Ok(products)
        })
    }

    /// Lists library products with server-side pagination and filtering.
    /// All filtering, sorting, and pagination is done in SQLite.
    pub fn list_library_products_paginated(
        &self,
        limit: i64,
        offset: i64,
        search_query: Option<&str>,
        library_filter: Option<&str>,
        category_filter: Option<&str>,
        type_filter: Option<&str>,
        vendor_filter: Option<&str>,
        sort_by: Option<&str>,
        collection_id: Option<i64>,
    ) -> AppResult<(Vec<Product>, i64)> {
        self.with_connection(|conn| {
            let mut conditions: Vec<String> = Vec::new();
            let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
            let mut param_idx = 1u32;

            // Optional JOIN for collection filtering
            let join_clause = if let Some(cid) = collection_id {
                conditions.push(format!("ci.collection_id = ?{}", param_idx));
                params.push(Box::new(cid));
                param_idx += 1;
                "INNER JOIN collection_items ci ON ci.product_id = products.id"
            } else {
                ""
            };

            if let Some(q) = search_query {
                let trimmed = q.trim();
                if !trimmed.is_empty() {
                    // Tokenized search: split query into words, each must match at least one field
                    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
                    for token in tokens {
                        let pattern = format!("%{}%", token);
                        conditions.push(format!(
                            "(name LIKE ?{i} OR tags LIKE ?{i} OR content_type LIKE ?{i} OR vendor LIKE ?{i} OR categories LIKE ?{i})",
                            i = param_idx
                        ));
                        params.push(Box::new(pattern));
                        param_idx += 1;
                    }
                }
            }

            if let Some(lib) = library_filter {
                if !lib.is_empty() {
                    conditions.push(format!("library_path = ?{}", param_idx));
                    params.push(Box::new(lib.to_string()));
                    param_idx += 1;
                }
            }

            if let Some(cat) = category_filter {
                if !cat.is_empty() {
                    let pattern = format!("%{}%", cat);
                    conditions.push(format!("categories LIKE ?{}", param_idx));
                    params.push(Box::new(pattern));
                    param_idx += 1;
                }
            }

            if let Some(ct) = type_filter {
                if !ct.is_empty() {
                    if ct == "unknown" {
                        conditions.push("(content_type IS NULL OR content_type = '')".to_string());
                    } else {
                        conditions.push(format!("content_type = ?{}", param_idx));
                        params.push(Box::new(ct.to_string()));
                        param_idx += 1;
                    }
                }
            }

            if let Some(v) = vendor_filter {
                if !v.is_empty() {
                    conditions.push(format!("vendor = ?{}", param_idx));
                    params.push(Box::new(v.to_string()));
                    param_idx += 1;
                }
            }

            let where_clause = if conditions.is_empty() {
                "1=1".to_string()
            } else {
                conditions.join(" AND ")
            };

            // Count total matching rows
            let count_sql = format!("SELECT COUNT(*) FROM products {} WHERE {}", join_clause, where_clause);
            let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
            let total: i64 = conn.query_row(&count_sql, param_refs.as_slice(), |row| row.get(0))?;

            // Determine ORDER BY
            let order = match sort_by {
                Some("date") => "installed_at DESC",
                Some("size") => "total_size DESC",
                _ => "name COLLATE NOCASE ASC",
            };

            // Fetch paginated rows
            let select_sql = format!(
                "SELECT {} FROM products {} WHERE {} ORDER BY {} LIMIT ?{} OFFSET ?{}",
                PRODUCT_SELECT_FIELDS, join_clause, where_clause, order, param_idx, param_idx + 1
            );

            params.push(Box::new(limit));
            params.push(Box::new(offset));

            let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
            let mut stmt = conn.prepare(&select_sql)?;
            let products = stmt
                .query_map(param_refs.as_slice(), Self::map_row)?
                .collect::<Result<Vec<_>, _>>()?;

            debug!(
                "Paginated library products: {} rows (total={}, offset={}, limit={})",
                products.len(), total, offset, limit
            );

            Ok((products, total))
        })
    }

    /// Returns a sorted list of distinct non-null vendor names.
    pub fn list_distinct_vendors(&self) -> AppResult<Vec<String>> {
        self.with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT vendor FROM products WHERE vendor IS NOT NULL AND vendor != '' ORDER BY vendor COLLATE NOCASE"
            )?;
            let vendors = stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .filter_map(|r| r.ok())
                .collect();
            Ok(vendors)
        })
    }

    /// Returns aggregate statistics for library products.
    pub fn get_library_stats(&self) -> AppResult<LibraryStats> {
        self.with_connection(|conn| {
            // Total count + total size in a single query
            let (total_products, total_size_bytes): (i64, i64) = conn.query_row(
                "SELECT COUNT(*), COALESCE(SUM(total_size), 0) FROM products",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;

            // Products by content type
            let mut stmt = conn.prepare(
                "SELECT COALESCE(NULLIF(content_type, ''), 'unknown') AS ct, COUNT(*) \
                 FROM products \
                 GROUP BY ct ORDER BY COUNT(*) DESC"
            )?;
            let products_by_type: Vec<TypeCount> = stmt
                .query_map([], |row| {
                    Ok(TypeCount {
                        content_type: row.get(0)?,
                        count: row.get(1)?,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();

            // Top 10 vendors
            let mut stmt = conn.prepare(
                "SELECT vendor, COUNT(*) as cnt FROM products \
                 WHERE vendor IS NOT NULL AND vendor != '' \
                 GROUP BY vendor ORDER BY cnt DESC LIMIT 10"
            )?;
            let top_vendors: Vec<VendorCount> = stmt
                .query_map([], |row| {
                    Ok(VendorCount {
                        vendor: row.get(0)?,
                        count: row.get(1)?,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();

            // Recent products (last 5 added)
            let mut stmt = conn.prepare(
                &format!(
                    "SELECT {} FROM products ORDER BY installed_at DESC LIMIT 5",
                    PRODUCT_SELECT_FIELDS
                )
            )?;
            let recent_products: Vec<Product> = stmt
                .query_map([], Self::map_row)?
                .filter_map(|r| r.ok())
                .collect();

            Ok(LibraryStats {
                total_products,
                total_size_bytes,
                products_by_type,
                top_vendors,
                recent_products,
            })
        })
    }

    /// Finds duplicate products (same name AND same vendor).
    /// Returns groups of duplicates as Vec<Vec<Product>>.
    pub fn find_duplicates(&self) -> AppResult<Vec<DuplicateGroup>> {
        self.with_connection(|conn| {
            // Find (name, vendor) pairs that appear more than once
            let mut stmt = conn.prepare(
                &format!(
                    "SELECT {} FROM products \
                     WHERE origin = 'library' AND (name, COALESCE(vendor, '')) IN ( \
                       SELECT name, COALESCE(vendor, '') FROM products \
                       WHERE origin = 'library' \
                       GROUP BY name, COALESCE(vendor, '') \
                       HAVING COUNT(*) > 1 \
                     ) \
                     ORDER BY name COLLATE NOCASE, vendor COLLATE NOCASE, installed_at",
                    PRODUCT_SELECT_FIELDS
                )
            )?;
            let all: Vec<Product> = stmt
                .query_map([], Self::map_row)?
                .filter_map(|r| r.ok())
                .collect();

            // Group by (name_lower, vendor_lower)
            let mut map: std::collections::BTreeMap<(String, String), Vec<Product>> =
                std::collections::BTreeMap::new();
            for p in all {
                let key = (
                    p.name.to_lowercase(),
                    p.vendor.as_deref().unwrap_or("").to_lowercase(),
                );
                map.entry(key).or_default().push(p);
            }

            let groups: Vec<DuplicateGroup> = map
                .into_values()
                .filter(|g| g.len() > 1)
                .map(|products| {
                    let name = products[0].name.clone();
                    let vendor = products[0].vendor.clone();
                    let count = products.len() as i64;
                    DuplicateGroup { name, vendor, count, products }
                })
                .collect();

            Ok(groups)
        })
    }

    // ========================================================================
    // Collections CRUD
    // ========================================================================

    /// Creates a new collection. Returns the new collection's ID.
    pub fn create_collection(&self, name: &str) -> AppResult<Collection> {
        self.with_connection(|conn| {
            let now = Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO collections (name, created_at, updated_at) VALUES (?1, ?2, ?3)",
                params![name.trim(), now, now],
            )?;
            let id = conn.last_insert_rowid();
            info!("Created collection '{}' (id={})", name, id);
            Ok(Collection {
                id,
                name: name.trim().to_string(),
                item_count: 0,
                created_at: now.clone(),
                updated_at: now,
            })
        })
    }

    /// Lists all collections with their item counts, ordered by name.
    pub fn list_collections(&self) -> AppResult<Vec<Collection>> {
        self.with_connection(|conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT c.id, c.name, c.created_at, c.updated_at,
                       COALESCE(cnt.n, 0) AS item_count
                FROM collections c
                LEFT JOIN (
                    SELECT collection_id, COUNT(*) AS n FROM collection_items GROUP BY collection_id
                ) cnt ON cnt.collection_id = c.id
                ORDER BY c.name COLLATE NOCASE
                "#,
            )?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(Collection {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        created_at: row.get(2)?,
                        updated_at: row.get(3)?,
                        item_count: row.get(4)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }

    /// Renames an existing collection.
    pub fn rename_collection(&self, id: i64, new_name: &str) -> AppResult<()> {
        self.with_connection(|conn| {
            let now = Utc::now().to_rfc3339();
            let rows = conn.execute(
                "UPDATE collections SET name = ?1, updated_at = ?2 WHERE id = ?3",
                params![new_name.trim(), now, id],
            )?;
            if rows == 0 {
                return Err(AppError::Database(format!("Collection {} not found", id)));
            }
            Ok(())
        })
    }

    /// Deletes a collection (cascade deletes items).
    pub fn delete_collection(&self, id: i64) -> AppResult<()> {
        self.with_connection(|conn| {
            let rows = conn.execute("DELETE FROM collections WHERE id = ?1", [id])?;
            if rows == 0 {
                return Err(AppError::Database(format!("Collection {} not found", id)));
            }
            info!("Deleted collection id={}", id);
            Ok(())
        })
    }

    /// Adds product IDs to a collection (bulk). Silently ignores duplicates.
    pub fn add_to_collection(&self, collection_id: i64, product_ids: &[i64]) -> AppResult<usize> {
        if product_ids.is_empty() {
            return Ok(0);
        }
        self.with_connection(|conn| {
            let now = Utc::now().to_rfc3339();
            let tx = conn.unchecked_transaction()?;
            let mut added = 0usize;
            for &pid in product_ids {
                let r = tx.execute(
                    "INSERT OR IGNORE INTO collection_items (collection_id, product_id, added_at) VALUES (?1, ?2, ?3)",
                    params![collection_id, pid, now],
                )?;
                added += r;
            }
            // Bump updated_at
            tx.execute(
                "UPDATE collections SET updated_at = ?1 WHERE id = ?2",
                params![now, collection_id],
            )?;
            tx.commit()?;
            info!("Added {} products to collection {}", added, collection_id);
            Ok(added)
        })
    }

    /// Removes product IDs from a collection.
    pub fn remove_from_collection(&self, collection_id: i64, product_ids: &[i64]) -> AppResult<usize> {
        if product_ids.is_empty() {
            return Ok(0);
        }
        self.with_connection(|conn| {
            let tx = conn.unchecked_transaction()?;
            let mut removed = 0usize;
            for &pid in product_ids {
                let r = tx.execute(
                    "DELETE FROM collection_items WHERE collection_id = ?1 AND product_id = ?2",
                    params![collection_id, pid],
                )?;
                removed += r;
            }
            let now = Utc::now().to_rfc3339();
            tx.execute(
                "UPDATE collections SET updated_at = ?1 WHERE id = ?2",
                params![now, collection_id],
            )?;
            tx.commit()?;
            info!("Removed {} products from collection {}", removed, collection_id);
            Ok(removed)
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

    #[test]
    fn test_list_library_products_paginated() {
        let db = Database::open_in_memory().unwrap();

        // Insert library products via upsert
        for i in 1..=10 {
            let input = LibraryProductInput {
                name: format!("Product {:02}", i),
                path: format!("/lib/product{}", i),
                library_path: "/lib".to_string(),
                support_file: format!("product{}.dsx", i),
                product_token: None,
                global_id: None,
                vendor: if i % 2 == 0 { Some("VendorA".to_string()) } else { None },
                categories: vec!["Default/General".to_string()],
                content_type: if i <= 5 { Some("Character".to_string()) } else { Some("Prop".to_string()) },
                installed_at: chrono::Utc::now().to_rfc3339(),
                thumbnail_path: None,
                files_count: i as i64 * 10,
                total_size: i as i64 * 1000,
            };
            db.upsert_library_product(&input).unwrap();
        }

        // Paginate: first page
        let (items, total) = db
            .list_library_products_paginated(5, 0, None, None, None, None, None, None, None)
            .unwrap();
        assert_eq!(total, 10);
        assert_eq!(items.len(), 5);

        // Second page
        let (items2, total2) = db
            .list_library_products_paginated(5, 5, None, None, None, None, None, None, None)
            .unwrap();
        assert_eq!(total2, 10);
        assert_eq!(items2.len(), 5);

        // No overlap
        let ids1: Vec<i64> = items.iter().map(|p| p.id).collect();
        let ids2: Vec<i64> = items2.iter().map(|p| p.id).collect();
        assert!(ids1.iter().all(|id| !ids2.contains(id)));

        // Filter by content type
        let (chars, char_total) = db
            .list_library_products_paginated(50, 0, None, None, None, Some("Character"), None, None, None)
            .unwrap();
        assert_eq!(char_total, 5);
        assert_eq!(chars.len(), 5);

        // Search query
        let (searched, search_total) = db
            .list_library_products_paginated(50, 0, Some("Product 01"), None, None, None, None, None, None)
            .unwrap();
        assert_eq!(search_total, 1);
        assert_eq!(searched.len(), 1);
        assert_eq!(searched[0].name, "Product 01");

        // Unknown type filter
        let (unknown, unknown_total) = db
            .list_library_products_paginated(50, 0, None, None, None, Some("unknown"), None, None, None)
            .unwrap();
        assert_eq!(unknown_total, 0);
        assert_eq!(unknown.len(), 0);
    }
}
