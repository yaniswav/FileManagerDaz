//! Installed bundles management
//!
//! Allows detecting if an archive has already been installed by calculating
//! a hash of its content and verifying file integrity.

use crate::error::{AppError, AppResult};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

// ============================================================================
// Types
// ============================================================================

/// Information about an installed bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledBundle {
    pub id: i64,
    /// Blake3 hash of the source archive
    pub archive_hash: String,
    /// Original path of the archive
    pub archive_path: PathBuf,
    /// Archive name (without path)
    pub archive_name: String,
    /// Installation date (timestamp)
    pub installed_at: i64,
    /// Number of installed files
    pub file_count: usize,
    /// Total size in bytes
    pub total_size: u64,
    /// Installation destination path
    pub destination_path: PathBuf,
}

/// Integrity check result
#[derive(Debug, Clone, Serialize)]
pub struct IntegrityCheckResult {
    /// Verified bundle
    pub bundle_id: i64,
    /// Number of files found
    pub files_found: usize,
    /// Number of expected files
    pub files_expected: usize,
    /// Missing files
    pub missing_files: Vec<PathBuf>,
    /// Bundle is valid
    pub is_valid: bool,
}

/// Pre-installation check result
#[derive(Debug, Clone, Serialize)]
pub struct PreInstallCheck {
    /// Already installed?
    pub already_installed: bool,
    /// Existing bundle details (if installed)
    pub existing_bundle: Option<InstalledBundle>,
}

// ============================================================================
// Bundles table management
// ============================================================================

/// Creates the bundles table if it doesn't exist
pub fn init_bundles_table(conn: &Connection) -> AppResult<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS bundles (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            archive_hash TEXT NOT NULL UNIQUE,
            archive_path TEXT NOT NULL,
            archive_name TEXT NOT NULL,
            installed_at INTEGER NOT NULL,
            file_count INTEGER NOT NULL,
            total_size INTEGER NOT NULL,
            destination_path TEXT NOT NULL
        )",
        [],
    )?;

    // Index for fast hash lookup
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_bundles_hash ON bundles(archive_hash)",
        [],
    )?;

    info!("Bundles table initialized");
    Ok(())
}

/// Checks if an archive is already installed by its hash
pub fn check_bundle_by_hash(conn: &Connection, hash: &str) -> AppResult<PreInstallCheck> {
    let mut stmt = conn.prepare(
        "SELECT id, archive_hash, archive_path, archive_name, installed_at, 
                file_count, total_size, destination_path 
         FROM bundles WHERE archive_hash = ?1",
    )?;

    let bundle = stmt
        .query_row(params![hash], |row| {
            Ok(InstalledBundle {
                id: row.get(0)?,
                archive_hash: row.get(1)?,
                archive_path: PathBuf::from(row.get::<_, String>(2)?),
                archive_name: row.get(3)?,
                installed_at: row.get(4)?,
                file_count: row.get::<_, i64>(5)? as usize,
                total_size: row.get::<_, i64>(6)? as u64,
                destination_path: PathBuf::from(row.get::<_, String>(7)?),
            })
        })
        .optional()?;

    Ok(PreInstallCheck {
        already_installed: bundle.is_some(),
        existing_bundle: bundle,
    })
}

/// Registers a newly installed bundle
pub fn register_bundle(
    conn: &Connection,
    archive_hash: &str,
    archive_path: &Path,
    file_count: usize,
    total_size: u64,
    destination_path: &Path,
) -> AppResult<i64> {
    let archive_name = archive_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let now = chrono::Utc::now().timestamp();

    conn.execute(
        "INSERT INTO bundles (archive_hash, archive_path, archive_name, installed_at, 
                              file_count, total_size, destination_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            archive_hash,
            archive_path.to_string_lossy().to_string(),
            archive_name,
            now,
            file_count as i64,
            total_size as i64,
            destination_path.to_string_lossy().to_string(),
        ],
    )?;

    let id = conn.last_insert_rowid();
    info!(
        "Registered bundle {} with hash {}",
        id,
        &archive_hash[..16.min(archive_hash.len())]
    );
    Ok(id)
}

/// Lists all installed bundles
pub fn list_bundles(conn: &Connection) -> AppResult<Vec<InstalledBundle>> {
    let mut stmt = conn.prepare(
        "SELECT id, archive_hash, archive_path, archive_name, installed_at, 
                file_count, total_size, destination_path 
         FROM bundles ORDER BY installed_at DESC",
    )?;

    let bundles = stmt
        .query_map([], |row| {
            Ok(InstalledBundle {
                id: row.get(0)?,
                archive_hash: row.get(1)?,
                archive_path: PathBuf::from(row.get::<_, String>(2)?),
                archive_name: row.get(3)?,
                installed_at: row.get(4)?,
                file_count: row.get::<_, i64>(5)? as usize,
                total_size: row.get::<_, i64>(6)? as u64,
                destination_path: PathBuf::from(row.get::<_, String>(7)?),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(bundles)
}

/// Removes a bundle from the database (does not delete files)
pub fn remove_bundle(conn: &Connection, bundle_id: i64) -> AppResult<()> {
    let affected = conn.execute("DELETE FROM bundles WHERE id = ?1", params![bundle_id])?;

    if affected == 0 {
        warn!("No bundle found with id {}", bundle_id);
    } else {
        info!("Removed bundle {} from database", bundle_id);
    }

    Ok(())
}

/// Gets a bundle by its ID
pub fn get_bundle(conn: &Connection, bundle_id: i64) -> AppResult<Option<InstalledBundle>> {
    let mut stmt = conn.prepare(
        "SELECT id, archive_hash, archive_path, archive_name, installed_at, 
                file_count, total_size, destination_path 
         FROM bundles WHERE id = ?1",
    )?;

    let bundle = stmt
        .query_row(params![bundle_id], |row| {
            Ok(InstalledBundle {
                id: row.get(0)?,
                archive_hash: row.get(1)?,
                archive_path: PathBuf::from(row.get::<_, String>(2)?),
                archive_name: row.get(3)?,
                installed_at: row.get(4)?,
                file_count: row.get::<_, i64>(5)? as usize,
                total_size: row.get::<_, i64>(6)? as u64,
                destination_path: PathBuf::from(row.get::<_, String>(7)?),
            })
        })
        .optional()?;

    Ok(bundle)
}

// ============================================================================
// Archive hashing
// ============================================================================

/// Computes the blake3 hash of an archive file using streaming
///
/// Uses a 64KB buffer for performance on large files.
pub fn compute_archive_hash(archive_path: &Path) -> AppResult<String> {
    use std::fs::File;
    use std::io::Read;

    debug!("Computing hash for: {:?}", archive_path);

    if !archive_path.exists() {
        return Err(AppError::NotFound(archive_path.to_path_buf()));
    }

    let mut file = File::open(archive_path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; 64 * 1024]; // 64KB buffer

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let hash = hasher.finalize();
    let hash_hex = hash.to_hex().to_string();

    debug!("Hash computed: {}", &hash_hex[..16]);
    Ok(hash_hex)
}

// ============================================================================
// Integrity verification (simplified)
// ============================================================================

/// Verifies that the bundle destination still exists
///
/// Note: Simplified verification - we only check that the destination folder exists.
/// A complete verification would require storing the file list.
pub fn verify_bundle_integrity(
    conn: &Connection,
    bundle_id: i64,
) -> AppResult<IntegrityCheckResult> {
    let bundle = get_bundle(conn, bundle_id)?
        .ok_or_else(|| AppError::NotFound(PathBuf::from(format!("bundle:{}", bundle_id))))?;

    let destination_exists = bundle.destination_path.exists();

    // Simplified counting: count files in the destination folder
    let files_found = if destination_exists {
        count_files_in_directory(&bundle.destination_path).unwrap_or(0)
    } else {
        0
    };

    let is_valid = destination_exists && files_found > 0;

    Ok(IntegrityCheckResult {
        bundle_id,
        files_found,
        files_expected: bundle.file_count,
        missing_files: if destination_exists {
            vec![]
        } else {
            vec![bundle.destination_path.clone()]
        },
        is_valid,
    })
}

/// Counts files in a folder (recursive)
fn count_files_in_directory(dir: &Path) -> AppResult<usize> {
    let mut count = 0;

    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            count += 1;
        }
    }

    Ok(count)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_db() -> (TempDir, Connection) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();
        init_bundles_table(&conn).unwrap();
        (temp_dir, conn)
    }

    #[test]
    fn test_register_and_check_bundle() {
        let (_temp_dir, conn) = setup_test_db();
        let dest = PathBuf::from("/fake/dest");

        // Enregistrer un bundle
        let id = register_bundle(
            &conn,
            "abc123hash",
            Path::new("C:/Downloads/MyBundle.zip"),
            42,
            1024000,
            &dest,
        )
        .unwrap();

        assert!(id > 0);

        // Verify it is found by hash
        let check = check_bundle_by_hash(&conn, "abc123hash").unwrap();
        assert!(check.already_installed);
        assert_eq!(check.existing_bundle.unwrap().file_count, 42);

        // Verify that another hash is not found
        let check2 = check_bundle_by_hash(&conn, "other_hash").unwrap();
        assert!(!check2.already_installed);
    }

    #[test]
    fn test_compute_hash() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.zip");
        fs::write(&file_path, b"test content for hashing").unwrap();

        let hash = compute_archive_hash(&file_path).unwrap();

        // The blake3 hash is 64 hex characters
        assert_eq!(hash.len(), 64);

        // The same file should give the same hash
        let hash2 = compute_archive_hash(&file_path).unwrap();
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_list_bundles() {
        let (_temp_dir, conn) = setup_test_db();
        let dest = PathBuf::from("/fake/dest");

        register_bundle(&conn, "hash1", Path::new("bundle1.zip"), 10, 1000, &dest).unwrap();
        register_bundle(&conn, "hash2", Path::new("bundle2.zip"), 20, 2000, &dest).unwrap();

        let bundles = list_bundles(&conn).unwrap();
        assert_eq!(bundles.len(), 2);
    }
}
