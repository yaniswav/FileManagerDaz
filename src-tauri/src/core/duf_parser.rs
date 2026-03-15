//! DAZ User File (.duf) dependency parser.
//!
//! A `.duf` file is JSON (optionally GZIP-compressed). This module reads a `.duf`,
//! decompresses it if necessary, walks the JSON tree, and extracts all referenced
//! file paths (textures, morphs, geometries, etc.).

use crate::error::AppResult;
use flate2::read::GzDecoder;
use serde_json::Value;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use tracing::{debug, warn};

/// Maximum file size (50 MB) for DUF parsing. Larger files are likely
/// scenes or HD morphs with huge geometry data — not worth parsing for deps.
const MAX_DUF_SIZE: u64 = 50 * 1024 * 1024;

/// Reads a `.duf` file (auto-detecting GZIP vs plain JSON) and returns all
/// referenced file paths found inside. Paths are normalized to forward slashes.
pub fn extract_duf_dependencies(path: &Path) -> AppResult<Vec<String>> {
    // Skip very large files to avoid stalling the scan
    if let Ok(meta) = path.metadata() {
        if meta.len() > MAX_DUF_SIZE {
            debug!(
                "Skipping oversized DUF ({:.1} MB): {}",
                meta.len() as f64 / 1_048_576.0,
                path.display()
            );
            return Ok(vec![]);
        }
    }

    let json = read_duf_json(path)?;
    let deps = collect_uri_references(&json);
    debug!(
        "Extracted {} dependencies from {}",
        deps.len(),
        path.display()
    );
    Ok(deps)
}

/// Reads a `.duf` file, auto-detecting GZIP compression.
/// Returns the parsed JSON value.
fn read_duf_json(path: &Path) -> AppResult<Value> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    // Peek at the first 2 bytes to detect GZIP magic number (0x1F 0x8B)
    let mut magic = [0u8; 2];
    reader.read_exact(&mut magic)?;

    // Re-open for full read (we consumed 2 bytes)
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let json: Value = if magic[0] == 0x1F && magic[1] == 0x8B {
        // GZIP-compressed
        let decoder = GzDecoder::new(reader);
        serde_json::from_reader(decoder).map_err(|e| {
            crate::error::AppError::Other(format!(
                "Failed to parse GZIP DUF {}: {}",
                path.display(),
                e
            ))
        })?
    } else {
        // Plain JSON
        serde_json::from_reader(reader).map_err(|e| {
            crate::error::AppError::Other(format!(
                "Failed to parse DUF JSON {}: {}",
                path.display(),
                e
            ))
        })?
    };

    Ok(json)
}

/// Recursively walks the JSON tree and collects all string values that look like
/// file references (URI paths to textures, morphs, geometries, etc.).
fn collect_uri_references(value: &Value) -> Vec<String> {
    let mut refs = HashSet::new();
    walk_json(value, &mut refs);
    let mut result: Vec<String> = refs.into_iter().collect();
    result.sort();
    result
}

/// Keys whose string values are known to contain file references in DAZ JSON.
const REF_KEYS: &[&str] = &[
    "url",
    "geometry",
    "morph",
    "map",
    "image",
    "value",
    "source",
    "parent",
    "inherits",
];

/// File extensions that indicate a referenced asset.
const ASSET_EXTENSIONS: &[&str] = &[
    ".duf", ".dsf", ".dsa", ".dse", ".dbz", ".dhdm", ".djl", ".dst",
    ".jpg", ".jpeg", ".png", ".tif", ".tiff", ".bmp", ".exr", ".hdr",
    ".obj", ".fbx",
];

fn walk_json(value: &Value, refs: &mut HashSet<String>) {
    match value {
        Value::String(s) => {
            if let Some(path) = extract_path_from_uri(s) {
                refs.insert(path);
            }
        }
        Value::Object(map) => {
            for (key, val) in map {
                let key_lower = key.to_lowercase();
                match val {
                    Value::String(s) => {
                        // For known reference keys, always try to extract
                        if REF_KEYS.contains(&key_lower.as_str()) {
                            if let Some(path) = extract_path_from_uri(s) {
                                refs.insert(path);
                            }
                        } else if looks_like_file_path(s) {
                            if let Some(path) = extract_path_from_uri(s) {
                                refs.insert(path);
                            }
                        }
                    }
                    _ => walk_json(val, refs),
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                walk_json(item, refs);
            }
        }
        _ => {}
    }
}

/// Checks if a string looks like a file path (has a known extension).
fn looks_like_file_path(s: &str) -> bool {
    let lower = s.to_lowercase();
    ASSET_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
}

/// Extracts and normalizes a file path from a DAZ URI string.
///
/// DAZ URIs can be:
/// - `/data/DAZ 3D/Genesis 9/Base/Genesis9.dsf` (absolute library-relative)
/// - `/data/DAZ 3D/Genesis 9/Base/Genesis9.dsf#geometry` (with fragment)
/// - `name:/data/something.dsf` (with scheme)
///
/// Returns the normalized path (forward slashes, no leading slash, no fragment/query).
fn extract_path_from_uri(uri: &str) -> Option<String> {
    let s = uri.trim();
    if s.is_empty() {
        return None;
    }

    // Strip scheme (e.g., "name:" prefix)
    let path_part = if let Some(colon_pos) = s.find(":/") {
        &s[colon_pos + 1..]
    } else {
        s
    };

    // Strip fragment (#geometry, #material, etc.)
    let path_part = path_part.split('#').next().unwrap_or(path_part);
    // Strip query string (unlikely but defensive)
    let path_part = path_part.split('?').next().unwrap_or(path_part);

    let path_part = path_part.trim();
    if path_part.is_empty() {
        return None;
    }

    // Must look like a file path with a known extension
    let lower = path_part.to_lowercase();
    if !ASSET_EXTENSIONS.iter().any(|ext| lower.ends_with(ext)) {
        return None;
    }

    // Normalize: forward slashes, strip leading slash
    let normalized = path_part.replace('\\', "/");
    let normalized = normalized.trim_start_matches('/');

    if normalized.is_empty() {
        return None;
    }

    Some(normalized.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_extract_path_from_uri_basic() {
        assert_eq!(
            extract_path_from_uri("/data/test.dsf"),
            Some("data/test.dsf".to_string())
        );
    }

    #[test]
    fn test_extract_path_from_uri_with_fragment() {
        assert_eq!(
            extract_path_from_uri("/data/test.dsf#geometry"),
            Some("data/test.dsf".to_string())
        );
    }

    #[test]
    fn test_extract_path_from_uri_with_scheme() {
        assert_eq!(
            extract_path_from_uri("name:/data/test.dsf#morph"),
            Some("data/test.dsf".to_string())
        );
    }

    #[test]
    fn test_extract_path_from_uri_rejects_non_asset() {
        assert_eq!(extract_path_from_uri("some random string"), None);
        assert_eq!(extract_path_from_uri(""), None);
    }

    #[test]
    fn test_looks_like_file_path() {
        assert!(looks_like_file_path("/data/test.dsf"));
        assert!(looks_like_file_path("Runtime/Textures/skin.jpg"));
        assert!(!looks_like_file_path("just a name"));
    }

    #[test]
    fn test_collect_uri_references() {
        let json: Value = serde_json::json!({
            "geometry_library": [{
                "id": "geo1",
                "source": "/data/genesis9.dsf#geometry"
            }],
            "modifier_library": [{
                "morph": {
                    "url": "/data/morph.dsf#morph"
                }
            }],
            "material_library": [{
                "extra": [{
                    "map": [{
                        "image": "/Runtime/Textures/skin.jpg"
                    }]
                }]
            }]
        });

        let refs = collect_uri_references(&json);
        assert!(refs.contains(&"data/genesis9.dsf".to_string()));
        assert!(refs.contains(&"data/morph.dsf".to_string()));
        assert!(refs.contains(&"Runtime/Textures/skin.jpg".to_string()));
    }

    #[test]
    fn test_read_plain_json_duf() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.duf");
        let mut f = File::create(&path).unwrap();
        let json = serde_json::json!({
            "asset_info": { "id": "/test.duf" },
            "scene": {
                "nodes": [{
                    "url": "/data/DAZ 3D/Genesis 9/Base/Genesis9.dsf#Genesis9"
                }]
            }
        });
        f.write_all(serde_json::to_string(&json).unwrap().as_bytes())
            .unwrap();

        let deps = extract_duf_dependencies(&path).unwrap();
        assert!(deps.contains(&"data/DAZ 3D/Genesis 9/Base/Genesis9.dsf".to_string()));
    }

    #[test]
    fn test_read_gzip_duf() {
        use flate2::write::GzEncoder;
        use flate2::Compression;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("compressed.duf");

        let json = serde_json::json!({
            "scene": {
                "materials": [{
                    "extra": [{
                        "type": "studio/material/daz_brick",
                        "map": [{ "image": "/Runtime/Textures/skin.png" }]
                    }]
                }]
            }
        });

        let f = File::create(&path).unwrap();
        let mut encoder = GzEncoder::new(f, Compression::default());
        encoder
            .write_all(serde_json::to_string(&json).unwrap().as_bytes())
            .unwrap();
        encoder.finish().unwrap();

        let deps = extract_duf_dependencies(&path).unwrap();
        assert!(deps.contains(&"Runtime/Textures/skin.png".to_string()));
    }
}
