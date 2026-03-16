//! Orphan `.duf` scanner for DAZ content libraries.
//!
//! Scans DAZ library content directories for `.duf` files not tracked by any
//! existing product, groups them by resolved product identity, parses DUF
//! dependencies in parallel using Rayon, then writes to SQLite via MPSC.

use crate::core::duf_parser;
use crate::core::thumbnails::find_best_thumbnail;
use crate::db::Database;
use crate::error::AppError;
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use rusqlite::params;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use tracing::{debug, info, warn};
use walkdir::WalkDir;

/// Content directories to scan for orphan .duf files.
const ORPHAN_SCAN_DIRS: &[&str] = &["People", "Props", "Environments", "Lights", "Cameras"];

/// Technical/structural folder names to skip when resolving a product name.
/// Case-insensitive comparison is used.
const TECHNICAL_DIRS: &[&str] = &[
    // Rendering / shading
    "Materials", "Iray", "3delight", "Shaders", "Textures",
    // Morphs / anatomy
    "Morphs", "Expressions", "Anatomy", "Base",
    // Clothing / accessories
    "Wardrobe", "Accessories",
    // Poses / animations
    "Poses", "Action Based",
    // Asset types used as subfolders
    "Props", "Hair", "Presets", "Scripts", "Data",
    // Common product subfolders
    "Normal", "Addons", "Options", "Shine", "Shine Options",
    "Colors", "Styles", "Mat", "Mats", "Blush", "Makeups",
    // Scene / render
    "Lights", "Cameras", "Environments",
    // DAZ generation folders (act as categories, not product names)
    "People", "Genesis 3 Female", "Genesis 3 Male",
    "Genesis 8 Female", "Genesis 8 Male", "Genesis 8.1 Female", "Genesis 8.1 Male",
    "Genesis 9",
    // Clothing subfolder conventions
    "Clothing", "Footwear", "Full Body",
    // Misc structural
    "Characters", "Figures", "Support",
];

/// Resolved identity for a group of orphan .duf files.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct ResolvedProduct {
    /// Human-readable product name (e.g., "Amala G9")
    name: String,
    /// Vendor / parent folder name (e.g., "Characters")
    vendor: Option<String>,
    /// Absolute path to the resolved product root folder
    root_path: PathBuf,
}

/// Result of parallel DUF processing for a single product group.
struct ParsedOrphanProduct {
    identity: ResolvedProduct,
    content_type: Option<String>,
    installed_at: String,
    thumbnail_path: Option<PathBuf>,
    file_entries: Vec<(String, String)>,
    duf_count: usize,
    total_size: u64,
}

/// Scans key content folders for `.duf` files not tracked by any existing product.
/// Groups them by resolved product identity, parses DUF dependencies in parallel
/// using Rayon, then writes to SQLite on a single consumer thread via MPSC.
///
/// `emit_progress` receives (phase: f32, message: &str) for progress reporting.
pub fn scan_orphan_dufs(
    db: &Database,
    library_path: &Path,
    num_threads: usize,
    emit_progress: &(dyn Fn(f32, &str) + Sync),
) -> Result<usize, AppError> {
    let lib_name = library_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("library");

    // Collect all file paths already tracked in product_files
    let tracked_paths: HashSet<String> = db.with_connection(|conn| {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT pf.relative_path FROM product_files pf"
        )?;
        let paths = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect::<HashSet<_>>();
        Ok(paths)
    })?;

    // Group orphan .duf files by their resolved product identity
    emit_progress(0.50, &format!("[{}] Collecting orphan .duf files…", lib_name));
    let mut orphan_groups: HashMap<ResolvedProduct, Vec<PathBuf>> = HashMap::new();

    for scan_dir_name in ORPHAN_SCAN_DIRS {
        let scan_dir = library_path.join(scan_dir_name);
        if !scan_dir.exists() {
            continue;
        }

        for entry in WalkDir::new(&scan_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default();

            if !ext.eq_ignore_ascii_case("duf") {
                continue;
            }

            // Compute library-relative path
            let rel_path = path
                .strip_prefix(library_path)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");

            // Skip if already tracked
            if tracked_paths.contains(&rel_path) {
                continue;
            }

            // Resolve the product identity by walking up past technical dirs
            let parent = path.parent().unwrap_or(path);
            let identity = resolve_product_identity(parent, library_path);

            orphan_groups
                .entry(identity)
                .or_default()
                .push(path.to_path_buf());
        }
    }

    if orphan_groups.is_empty() {
        return Ok(0);
    }

    let group_total = orphan_groups.len();
    info!(
        "Found {} product groups with untracked .duf files in {}",
        group_total, lib_name
    );

    // === Parallel DUF parsing (Rayon) → MPSC → single-threaded DB writer ===

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads)
        .thread_name(|i| format!("duf-parser-{}", i))
        .build()
        .or_else(|e| {
            warn!("Custom thread pool failed ({e}), falling back to single thread");
            rayon::ThreadPoolBuilder::new().num_threads(1).build()
        })
        .map_err(|e| AppError::Internal(format!("Cannot create thread pool: {e}")))?;

    info!("Orphan scan using {} threads", num_threads);

    // Collect groups into a vec for parallel iteration
    let groups_vec: Vec<(ResolvedProduct, Vec<PathBuf>)> = orphan_groups.into_iter().collect();

    // Channel: parallel parsers → single DB writer
    let (tx, rx) = mpsc::channel::<ParsedOrphanProduct>();

    // Progress counter (shared across Rayon threads)
    let parsed_count = std::sync::atomic::AtomicUsize::new(0);

    // Spawn Rayon parallel parsing in a scoped block
    let library_path_owned = library_path.to_path_buf();

    std::thread::scope(|s| {
        // Producer thread: runs the Rayon pool
        let producer = s.spawn(|| {
            pool.install(|| {
                groups_vec.par_iter().for_each(|(identity, duf_files)| {
                    let result = parse_orphan_group(&library_path_owned, identity, duf_files);

                    // Track progress
                    let done = parsed_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    if done % 20 == 0 || done == group_total {
                        let phase = 0.55 + (done as f32 / group_total as f32) * 0.35;
                        emit_progress(
                            phase,
                            &format!("[{}] Parsing: {}/{} products", lib_name, done, group_total),
                        );
                    }

                    let _ = tx.send(result);
                });
            });
            drop(tx); // Close channel when all producers are done
        });

        // Consumer: single-threaded DB writes
        let mut created = 0usize;
        let mut batch_count = 0usize;

        for parsed in rx.iter() {
            batch_count += 1;
            match write_orphan_product(db, &library_path_owned, &parsed) {
                Ok(_) => created += 1,
                Err(e) => {
                    warn!("Failed to write orphan product '{}': {}", parsed.identity.name, e);
                }
            }

            // Emit DB-write progress every 50 products
            if batch_count % 50 == 0 {
                let phase = 0.90 + (batch_count as f32 / group_total as f32) * 0.10;
                emit_progress(phase, &format!("[{}] Writing: {}/{}", lib_name, batch_count, group_total));
            }
        }

        // Wait for producer to finish
        let _ = producer.join();

        info!(
            "Pass 2 complete: created/merged {} products from {} groups in {}",
            created, group_total, lib_name
        );

        Ok(created)
    })
}

/// Walks UP from `start_dir` towards `library_path`, skipping technical folders,
/// to find the true product root directory.
fn resolve_product_identity(
    start_dir: &Path,
    library_path: &Path,
) -> ResolvedProduct {
    let technical_set: HashSet<String> = TECHNICAL_DIRS
        .iter()
        .map(|s| s.to_lowercase())
        .collect();

    let scan_root_set: HashSet<String> = ORPHAN_SCAN_DIRS
        .iter()
        .map(|s| s.to_lowercase())
        .collect();

    let mut current = start_dir;

    loop {
        if current == library_path || !current.starts_with(library_path) {
            break;
        }

        let folder_name = current
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        let folder_lower = folder_name.to_lowercase();

        if scan_root_set.contains(&folder_lower) {
            break;
        }

        if !technical_set.contains(&folder_lower) && !folder_name.is_empty() {
            let vendor = current
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .filter(|v| {
                    let v_lower = v.to_lowercase();
                    !scan_root_set.contains(&v_lower) && !technical_set.contains(&v_lower)
                })
                .map(|s| s.to_string());

            return ResolvedProduct {
                name: folder_name.to_string(),
                vendor,
                root_path: current.to_path_buf(),
            };
        }

        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }

    // Fallback: walk DOWN from library_path
    if let Ok(rel) = start_dir.strip_prefix(library_path) {
        for component in rel.components() {
            let comp_str = component.as_os_str().to_str().unwrap_or("");
            let comp_lower = comp_str.to_lowercase();
            if !comp_str.is_empty()
                && !technical_set.contains(&comp_lower)
                && !scan_root_set.contains(&comp_lower)
            {
                return ResolvedProduct {
                    name: comp_str.to_string(),
                    vendor: None,
                    root_path: library_path.join(component),
                };
            }
        }
    }

    // Ultimate fallback
    let name = start_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown")
        .to_string();

    ResolvedProduct {
        name,
        vendor: None,
        root_path: start_dir.to_path_buf(),
    }
}

/// Pure function: parses a group of .duf files into a ready-to-insert product.
/// Does heavy I/O (GZIP decompression, JSON parsing, image search) — safe for parallel execution.
fn parse_orphan_group(
    library_path: &Path,
    identity: &ResolvedProduct,
    duf_files: &[PathBuf],
) -> ParsedOrphanProduct {
    let folder_lower = identity.root_path.to_string_lossy().to_lowercase();
    let content_type = if folder_lower.contains("people") || folder_lower.contains("character") {
        Some("character".to_string())
    } else if folder_lower.contains("prop") {
        Some("prop".to_string())
    } else if folder_lower.contains("environment") {
        Some("environment".to_string())
    } else if folder_lower.contains("light") {
        Some("light".to_string())
    } else if folder_lower.contains("hair") {
        Some("hair".to_string())
    } else if folder_lower.contains("pose") {
        Some("pose".to_string())
    } else {
        None
    };

    let installed_at = duf_files
        .iter()
        .filter_map(|p| p.metadata().ok())
        .filter_map(|m| m.modified().ok())
        .max()
        .map(|mtime| DateTime::<Utc>::from(mtime).to_rfc3339())
        .unwrap_or_else(|| Utc::now().to_rfc3339());

    let thumbnail_path = find_best_thumbnail(&identity.root_path, &identity.name, duf_files);

    let mut file_entries: Vec<(String, String)> = Vec::new();
    let mut total_size: u64 = 0;

    for duf_path in duf_files {
        total_size += fs::metadata(duf_path).map(|m| m.len()).unwrap_or(0);
        let rel = duf_path
            .strip_prefix(library_path)
            .unwrap_or(duf_path)
            .to_string_lossy()
            .replace('\\', "/");
        file_entries.push((rel, "Content".to_string()));
    }

    for duf_path in duf_files {
        match duf_parser::extract_duf_dependencies(duf_path) {
            Ok(deps) => {
                for dep in &deps {
                    let dep_path = library_path.join(dep.replace('/', "\\"));
                    total_size += fs::metadata(&dep_path).map(|m| m.len()).unwrap_or(0);
                }
                for dep in deps {
                    file_entries.push((dep, "Content".to_string()));
                }
            }
            Err(e) => {
                warn!(
                    "Failed to parse DUF dependencies from {}: {}",
                    duf_path.display(),
                    e
                );
            }
        }
    }

    file_entries.sort();
    file_entries.dedup();

    ParsedOrphanProduct {
        identity: identity.clone(),
        content_type,
        installed_at,
        thumbnail_path,
        file_entries,
        duf_count: duf_files.len(),
        total_size,
    }
}

/// Writes a single parsed orphan product to the database (upsert + file entries)
/// within an atomic transaction. Must be called from a single thread.
fn write_orphan_product(
    db: &Database,
    library_path: &Path,
    parsed: &ParsedOrphanProduct,
) -> Result<(), AppError> {
    let library_path_str = library_path.to_string_lossy().to_string();

    db.with_connection(|conn| {
        let tx = conn.unchecked_transaction()?;

        let existing_id: Option<i64> = match tx.query_row(
            "SELECT id FROM products WHERE origin = 'library' AND name = ?1 AND library_path = ?2",
            params![parsed.identity.name, library_path_str],
            |row| row.get(0),
        ) {
            Ok(id) => Some(id),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(e.into()),
        };

        let product_id = if let Some(id) = existing_id {
            debug!(
                "Merging orphan files into existing product '{}' (id={})",
                parsed.identity.name, id
            );
            id
        } else {
            let rel_folder = parsed.identity
                .root_path
                .strip_prefix(library_path)
                .unwrap_or(&parsed.identity.root_path)
                .to_string_lossy()
                .replace('\\', "/");
            let support_file = format!("__custom__/{}", rel_folder);

            tx.execute(
                r#"
                INSERT INTO products (
                    name, path, origin, library_path, support_file,
                    vendor, content_type, categories, thumbnail_path,
                    installed_at, tags, files_count, total_size
                )
                VALUES (?1, ?2, 'library', ?3, ?4, ?5, ?6, '[]', ?7, ?8, '', ?9, ?10)
                "#,
                params![
                    parsed.identity.name,
                    parsed.identity.root_path.to_string_lossy().to_string(),
                    library_path_str,
                    support_file,
                    parsed.identity.vendor,
                    parsed.content_type,
                    parsed.thumbnail_path.as_ref().map(|p| p.to_string_lossy().replace('\\', "/")),
                    parsed.installed_at,
                    parsed.duf_count as i64,
                    parsed.total_size as i64,
                ],
            )?;
            let new_id = tx.last_insert_rowid();
            info!(
                "Created custom product '{}' (id={}) from {} orphan .duf files",
                parsed.identity.name, new_id, parsed.duf_count
            );
            new_id
        };

        // Insert product files within the same transaction
        if !parsed.file_entries.is_empty() {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO product_files (product_id, relative_path, target) VALUES (?1, ?2, ?3)",
            )?;
            let mut count = 0;
            for (path, target) in &parsed.file_entries {
                count += stmt.execute(params![product_id, path, target])?;
            }
            drop(stmt);

            debug!(
                "Stored {} file entries for product '{}' (id={})",
                count, parsed.identity.name, product_id
            );
        }

        tx.commit()?;
        Ok(())
    })
}
