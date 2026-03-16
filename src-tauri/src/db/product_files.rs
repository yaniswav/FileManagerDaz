//! Product files tracking — stores per-product file inventories from Manifest.dsx.
//!
//! Enables:
//! - Clean uninstall (know exactly which files to remove)
//! - Conflict detection (two products writing to the same path)
//! - File integrity checks

use crate::error::AppResult;
use rusqlite::Connection;
use tracing::debug;

/// A file belonging to a product (from Manifest.dsx).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductFile {
    pub id: i64,
    pub product_id: i64,
    /// Library-relative path (e.g., "data/lilflame/LF_HeavyLeather_Dress/...")
    pub relative_path: String,
    /// Installation target (usually "Content")
    pub target: String,
}

/// Conflict: two products claim the same file path
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileConflict {
    /// The conflicting file path
    pub relative_path: String,
    /// Product ID that already owns this file
    pub existing_product_id: i64,
    /// Name of the existing product
    pub existing_product_name: String,
}

/// Initialize the product_files table.
pub fn init_product_files_table(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS product_files (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            product_id INTEGER NOT NULL,
            relative_path TEXT NOT NULL,
            target TEXT NOT NULL DEFAULT 'Content',
            FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE CASCADE,
            UNIQUE(product_id, relative_path)
        );
        CREATE INDEX IF NOT EXISTS idx_product_files_product
            ON product_files(product_id);
        CREATE INDEX IF NOT EXISTS idx_product_files_path
            ON product_files(relative_path);
        "#,
    )?;
    debug!("Product files table initialized");
    Ok(())
}

/// Insert file entries for a product (from parsed Manifest.dsx).
/// Skips duplicates silently (INSERT OR IGNORE).
pub fn insert_product_files(
    conn: &Connection,
    product_id: i64,
    files: &[(String, String)], // (relative_path, target)
) -> AppResult<usize> {
    let mut stmt = conn.prepare(
        "INSERT OR IGNORE INTO product_files (product_id, relative_path, target) VALUES (?1, ?2, ?3)",
    )?;

    let mut count = 0;
    for (path, target) in files {
        count += stmt.execute(rusqlite::params![product_id, path, target])?;
    }

    debug!("Inserted {} file records for product {}", count, product_id);
    Ok(count)
}

/// Get all files for a product.
pub fn get_product_files(conn: &Connection, product_id: i64) -> AppResult<Vec<ProductFile>> {
    let mut stmt = conn.prepare(
        "SELECT id, product_id, relative_path, target FROM product_files WHERE product_id = ?1 ORDER BY relative_path",
    )?;

    let files = stmt
        .query_map(rusqlite::params![product_id], |row| {
            Ok(ProductFile {
                id: row.get(0)?,
                product_id: row.get(1)?,
                relative_path: row.get(2)?,
                target: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(files)
}

/// Check for conflicts: files that are already owned by another product.
/// Returns a list of conflicts for the given file paths.
pub fn check_file_conflicts(
    conn: &Connection,
    file_paths: &[String],
    exclude_product_id: Option<i64>,
) -> AppResult<Vec<FileConflict>> {
    if file_paths.is_empty() {
        return Ok(Vec::new());
    }

    // Build a parameterized query for the file paths
    let n_paths = file_paths.len();
    let placeholders: Vec<String> = (0..n_paths).map(|i| format!("?{}", i + 1)).collect();
    let exclude_clause = if exclude_product_id.is_some() {
        format!(" AND pf.product_id != ?{}", n_paths + 1)
    } else {
        String::new()
    };

    let sql = format!(
        "SELECT DISTINCT pf.relative_path, pf.product_id, p.name
         FROM product_files pf
         JOIN products p ON p.id = pf.product_id
         WHERE pf.relative_path IN ({}){}
         ORDER BY pf.relative_path",
        placeholders.join(", "),
        exclude_clause
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = file_paths
        .iter()
        .map(|s| Box::new(s.clone()) as Box<dyn rusqlite::types::ToSql>)
        .collect();
    if let Some(pid) = exclude_product_id {
        params.push(Box::new(pid));
    }

    let conflicts = stmt
        .query_map(rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())), |row| {
            Ok(FileConflict {
                relative_path: row.get(0)?,
                existing_product_id: row.get(1)?,
                existing_product_name: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(conflicts)
}

/// Insert file entries for a product within an explicit transaction.
/// More efficient for large batches (thousands of files).
pub fn insert_product_files_batch(
    conn: &Connection,
    product_id: i64,
    files: &[(String, String)], // (relative_path, target)
) -> AppResult<usize> {
    let tx = conn.unchecked_transaction()?;

    let mut stmt = tx.prepare(
        "INSERT OR IGNORE INTO product_files (product_id, relative_path, target) VALUES (?1, ?2, ?3)",
    )?;

    let mut count = 0;
    for (path, target) in files {
        count += stmt.execute(rusqlite::params![product_id, path, target])?;
    }

    drop(stmt);
    tx.commit()?;

    debug!("Batch-inserted {} file records for product {}", count, product_id);
    Ok(count)
}

/// Delete all file records for a product (used when uninstalling).
pub fn delete_product_files(conn: &Connection, product_id: i64) -> AppResult<usize> {
    let count = conn.execute(
        "DELETE FROM product_files WHERE product_id = ?1",
        rusqlite::params![product_id],
    )?;
    debug!("Deleted {} file records for product {}", count, product_id);
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE products (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                path TEXT NOT NULL,
                installed_at TEXT NOT NULL DEFAULT ''
            );
            "#,
        )
        .unwrap();
        init_product_files_table(&conn).unwrap();
        conn
    }

    fn insert_product(conn: &Connection, name: &str) -> i64 {
        conn.execute(
            "INSERT INTO products (name, path) VALUES (?1, ?2)",
            rusqlite::params![name, "/test"],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn test_insert_and_get_files() {
        let conn = setup_db();
        let pid = insert_product(&conn, "Test Product");

        let files = vec![
            ("data/test.dsf".to_string(), "Content".to_string()),
            ("Runtime/Textures/t.jpg".to_string(), "Content".to_string()),
        ];
        let count = insert_product_files(&conn, pid, &files).unwrap();
        assert_eq!(count, 2);

        let result = get_product_files(&conn, pid).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].relative_path, "Runtime/Textures/t.jpg"); // sorted
        assert_eq!(result[1].relative_path, "data/test.dsf");
    }

    #[test]
    fn test_insert_duplicates_ignored() {
        let conn = setup_db();
        let pid = insert_product(&conn, "Test");

        let files = vec![("data/test.dsf".to_string(), "Content".to_string())];
        insert_product_files(&conn, pid, &files).unwrap();
        let count = insert_product_files(&conn, pid, &files).unwrap();
        assert_eq!(count, 0); // duplicate ignored

        let result = get_product_files(&conn, pid).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_check_conflicts() {
        let conn = setup_db();
        let pid1 = insert_product(&conn, "Product A");
        let pid2 = insert_product(&conn, "Product B");

        let files_a = vec![
            ("data/shared.dsf".to_string(), "Content".to_string()),
            ("data/unique_a.dsf".to_string(), "Content".to_string()),
        ];
        insert_product_files(&conn, pid1, &files_a).unwrap();

        let files_b = vec!["data/shared.dsf".to_string(), "data/new.dsf".to_string()];
        let conflicts = check_file_conflicts(&conn, &files_b, Some(pid2)).unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].relative_path, "data/shared.dsf");
        assert_eq!(conflicts[0].existing_product_name, "Product A");
    }

    #[test]
    fn test_delete_product_files() {
        let conn = setup_db();
        let pid = insert_product(&conn, "Test");

        let files = vec![
            ("a.dsf".to_string(), "Content".to_string()),
            ("b.dsf".to_string(), "Content".to_string()),
        ];
        insert_product_files(&conn, pid, &files).unwrap();

        let deleted = delete_product_files(&conn, pid).unwrap();
        assert_eq!(deleted, 2);

        let result = get_product_files(&conn, pid).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_cascade_on_product_delete() {
        let conn = setup_db();
        // Enable foreign keys
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

        let pid = insert_product(&conn, "Test");
        let files = vec![("test.dsf".to_string(), "Content".to_string())];
        insert_product_files(&conn, pid, &files).unwrap();

        conn.execute("DELETE FROM products WHERE id = ?1", rusqlite::params![pid])
            .unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM product_files", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
