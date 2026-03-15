//! Scene Analyzer — parses a `.duf` scene file, extracts all asset references,
//! and cross-references them against the product_files table to determine which
//! products are required and which assets are missing.

use crate::db::{InstalledAsset, RequiredProduct, SceneAnalysisReport};
use crate::error::AppResult;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info};

/// Decode percent-encoded characters (%20 → space, etc.) in a path string.
fn percent_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (
                char_to_hex(bytes[i + 1]),
                char_to_hex(bytes[i + 2]),
            ) {
                result.push((hi << 4 | lo) as char);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

fn char_to_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Analyzes a `.duf` scene file and cross-references its dependencies
/// against the installed product files in the database.
///
/// Also checks if files physically exist on disk at any library path.
pub fn analyze_scene(
    duf_path: &Path,
    conn: &Connection,
    library_paths: &[String],
) -> AppResult<SceneAnalysisReport> {
    let scene_name = duf_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown Scene")
        .to_string();

    info!("Analyzing scene: {} ({})", scene_name, duf_path.display());

    // 1. Extract all dependency paths from the DUF file
    let raw_deps = crate::core::duf_parser::extract_duf_dependencies(duf_path)?;

    // 2. Percent-decode and normalize paths
    let deps: Vec<String> = raw_deps
        .into_iter()
        .map(|p| percent_decode(&p))
        .collect();

    let total_dependencies = deps.len();

    if total_dependencies == 0 {
        return Ok(SceneAnalysisReport {
            scene_name,
            total_dependencies: 0,
            installed_count: 0,
            missing_count: 0,
            completion_pct: 100.0,
            installed_assets: Vec::new(),
            untracked_assets: Vec::new(),
            missing_assets: Vec::new(),
            required_products: Vec::new(),
        });
    }

    // 3. Batch-lookup in product_files (process in chunks of 500 for SQLite param limits)
    let mut db_matches: HashMap<String, (i64, String)> = HashMap::new(); // path -> (product_id, product_name)
    
    for chunk in deps.chunks(500) {
        let placeholders: Vec<String> = (0..chunk.len()).map(|i| format!("?{}", i + 1)).collect();
        let sql = format!(
            "SELECT pf.relative_path, pf.product_id, p.name
             FROM product_files pf
             JOIN products p ON p.id = pf.product_id
             WHERE pf.relative_path IN ({})",
            placeholders.join(", ")
        );

        let mut stmt = conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::types::ToSql> = chunk
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();

        let rows = stmt.query_map(params.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        for row in rows {
            let (path, pid, pname) = row?;
            db_matches.insert(path, (pid, pname));
        }
    }

    // 4. Classify each dependency
    let mut installed_assets: Vec<InstalledAsset> = Vec::new();
    let mut untracked_assets: Vec<String> = Vec::new();
    let mut missing_assets: Vec<String> = Vec::new();

    for dep in &deps {
        if let Some((product_id, product_name)) = db_matches.get(dep) {
            installed_assets.push(InstalledAsset {
                relative_path: dep.clone(),
                product_id: *product_id,
                product_name: product_name.clone(),
            });
        } else {
            // Not in DB — check if file exists on disk in any library
            let exists_on_disk = library_paths.iter().any(|lib| {
                Path::new(lib).join(dep).exists()
            });

            if exists_on_disk {
                untracked_assets.push(dep.clone());
            } else {
                missing_assets.push(dep.clone());
            }
        }
    }

    // 5. Build required products list (grouped + deduplicated)
    let mut product_map: HashMap<i64, (String, usize)> = HashMap::new();
    for asset in &installed_assets {
        let entry = product_map
            .entry(asset.product_id)
            .or_insert_with(|| (asset.product_name.clone(), 0));
        entry.1 += 1;
    }

    let mut required_products: Vec<RequiredProduct> = product_map
        .into_iter()
        .map(|(id, (name, count))| RequiredProduct {
            product_id: id,
            product_name: name,
            files_used: count,
        })
        .collect();
    required_products.sort_by(|a, b| b.files_used.cmp(&a.files_used));

    let installed_count = installed_assets.len() + untracked_assets.len();
    let missing_count = missing_assets.len();
    let completion_pct = if total_dependencies == 0 {
        100.0
    } else {
        (installed_count as f64 / total_dependencies as f64) * 100.0
    };

    info!(
        "Scene '{}': {} deps, {} installed ({} tracked, {} untracked), {} missing ({:.1}%)",
        scene_name,
        total_dependencies,
        installed_count,
        installed_assets.len(),
        untracked_assets.len(),
        missing_count,
        completion_pct
    );

    Ok(SceneAnalysisReport {
        scene_name,
        total_dependencies,
        installed_count,
        missing_count,
        completion_pct,
        installed_assets,
        untracked_assets,
        missing_assets,
        required_products,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percent_decode() {
        assert_eq!(percent_decode("hello%20world"), "hello world");
        assert_eq!(percent_decode("no%20escape%21"), "no escape!");
        assert_eq!(percent_decode("plain"), "plain");
        assert_eq!(percent_decode("%2Fdata%2Ftest"), "/data/test");
    }

    #[test]
    fn test_percent_decode_invalid() {
        // Incomplete percent encoding — kept as-is
        assert_eq!(percent_decode("test%2"), "test%2");
        assert_eq!(percent_decode("test%"), "test%");
        assert_eq!(percent_decode("test%GG"), "test%GG");
    }
}
