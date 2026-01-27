//! DAZ content anchor detection system
//!
//! This module implements intelligent detection of DAZ content within arbitrary folder structures.
//! The core concept is that DAZ content always contains specific "anchor" folders (data/, Runtime/, etc.)
//! regardless of how vendors structure their archives.
//!
//! # Strategy
//!
//! 1. Scan recursively for any folders matching DAZ_ANCHORS
//! 2. For each anchor level found, treat it as a valid DAZ root
//! 3. Copy only from those anchor points to the library
//! 4. Handle multiple products in one archive
//! 5. Support loose files (.duf, .dsf) without folder structure

use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

// ============================================================================
// Constants
// ============================================================================

/// Standard DAZ anchor folders
/// These are the "magic" folders that DAZ Studio recognizes
pub const DAZ_ANCHORS: &[&str] = &[
    // Primary anchors (most common)
    "data",
    "Runtime",
    "People",
    // Secondary anchors (content-specific)
    "Environments",
    "Props",
    "Lights",
    "Cameras",
    "Poses",
    "Morphs",
    "Shaders",
    "Materials",
    "Textures",
    // Asset-specific anchors
    "Hairs",
    "Figures",
    "Scripts",
    "Presets",
    "Plugins",
    // Metadata/Support
    "Documentation",
    "ReadMe",
    // Additional Studio folders
    "General",
    "Light Presets",
    "Camera Presets",
    "Render Presets",
    "Render Settings",
    "Shader Presets",
    "Templates",
    "aniBlocks",
];

/// DAZ file extensions for loose file detection
const DAZ_EXTENSIONS: &[&str] = &[
    // DAZ Studio files
    ".duf", ".dsf", ".dse", ".dsa", ".ds", ".dsx", ".dbz", // 3D model files
    ".obj", ".dae", ".fbx", // Texture files (only consider at root level)
    ".png", ".jpg", ".jpeg", ".tif", ".tiff", ".exr", ".hdr", // Poser files
    ".cr2", ".pz2", ".pp2", ".fc2", ".hd2", ".hr2", ".lt2", ".cm2",
];

/// Files/folders to always skip
const SYSTEM_JUNK: &[&str] = &[
    ".DS_Store",
    "Thumbs.db",
    "desktop.ini",
    "__MACOSX",
    ".git",
    ".gitignore",
    "__pycache__",
    ".vscode",
    ".idea",
    "node_modules",
];

// ============================================================================
// Types
// ============================================================================

/// Represents a detected anchor point in the file system
#[derive(Debug, Clone, Serialize)]
pub struct AnchorPoint {
    /// Path to the anchor level (parent of anchor folders)
    pub path: PathBuf,

    /// Detected anchor folders at this level
    pub anchors: Vec<String>,

    /// Depth from the extraction root
    pub depth: usize,

    /// Total files under this anchor
    pub file_count: usize,

    /// Total size in bytes
    pub total_size: u64,
}

/// Result of anchor detection
#[derive(Debug, Clone, Serialize)]
pub struct AnchorDetectionResult {
    /// All detected anchor points
    pub anchor_points: Vec<AnchorPoint>,

    /// Loose files found (no anchors but valid DAZ files)
    pub has_loose_files: bool,

    /// Path containing loose files (if any)
    pub loose_files_path: Option<PathBuf>,

    /// Total scan depth
    pub max_depth: usize,
}

// ============================================================================
// Public API
// ============================================================================

/// Detects all DAZ anchor points in a directory tree
///
/// # Algorithm
///
/// 1. Recursively scan the directory tree
/// 2. At each level, check if any subfolder is a DAZ anchor
/// 3. If found, record this as an anchor point
/// 4. Continue scanning deeper (supports nested products)
/// 5. If no anchors found, check for loose DAZ files
///
/// # Returns
///
/// `AnchorDetectionResult` containing all detected anchor points
pub fn detect_anchors(root: &Path) -> AppResult<AnchorDetectionResult> {
    info!("Detecting DAZ anchors in: {:?}", root);

    if !root.exists() {
        return Err(AppError::NotFound(root.to_path_buf()));
    }

    let mut result = AnchorDetectionResult {
        anchor_points: Vec::new(),
        has_loose_files: false,
        loose_files_path: None,
        max_depth: 0,
    };

    // Scan for anchors recursively
    scan_for_anchors_recursive(root, root, 0, &mut result)?;

    // If no anchors found, check for loose files
    if result.anchor_points.is_empty() {
        debug!("No anchors found, checking for loose files...");
        if has_loose_daz_files(root) {
            info!("Found loose DAZ files at root");
            result.has_loose_files = true;
            result.loose_files_path = Some(root.to_path_buf());
        }
    }

    info!(
        "Anchor detection complete: {} anchor points, loose files: {}",
        result.anchor_points.len(),
        result.has_loose_files
    );

    Ok(result)
}

/// Checks if a filename should be skipped (system junk)
pub fn is_system_junk(name: &str) -> bool {
    SYSTEM_JUNK
        .iter()
        .any(|junk| name.eq_ignore_ascii_case(junk))
}

/// Checks if a file has a DAZ-related extension
pub fn is_daz_file(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext_lower = format!(".{}", ext.to_lowercase());
        DAZ_EXTENSIONS.contains(&ext_lower.as_str())
    } else {
        false
    }
}

// ============================================================================
// Internal implementation
// ============================================================================

/// Recursive scanner for anchor points
fn scan_for_anchors_recursive(
    current: &Path,
    root: &Path,
    depth: usize,
    result: &mut AnchorDetectionResult,
) -> AppResult<()> {
    // Update max depth
    if depth > result.max_depth {
        result.max_depth = depth;
    }

    // Safety limit
    if depth > 20 {
        warn!("Max depth exceeded at {:?}", current);
        return Ok(());
    }

    // Read directory entries
    let entries = match fs::read_dir(current) {
        Ok(e) => e,
        Err(e) => {
            warn!("Cannot read directory {:?}: {}", current, e);
            return Ok(());
        }
    };

    // Collect subdirectories
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let entry_path = entry.path();
        let entry_name = entry.file_name();
        let name_str = entry_name.to_string_lossy();

        // Skip system junk
        if is_system_junk(&name_str) {
            continue;
        }

        if entry_path.is_dir() {
            subdirs.push((entry_path, name_str.to_string()));
        }
    }

    // Check if any subdir is an anchor
    let mut found_anchors = Vec::new();
    for (_, name) in &subdirs {
        if DAZ_ANCHORS.contains(&name.as_str()) {
            found_anchors.push(name.clone());
        }
    }

    // If we found anchors at this level, record it
    if !found_anchors.is_empty() {
        debug!(
            "Found {} anchors at {:?} (depth {}): {:?}",
            found_anchors.len(),
            current,
            depth,
            found_anchors
        );

        // Count files and size under this anchor point
        let (file_count, total_size) = count_content(current);

        result.anchor_points.push(AnchorPoint {
            path: current.to_path_buf(),
            anchors: found_anchors.clone(),
            depth,
            file_count,
            total_size,
        });

        // Don't scan deeper from this level - we treat this as a product root
        // But we still process subdirectories to find nested products
    }

    // Recursively scan all subdirectories
    for (subdir_path, _) in subdirs {
        scan_for_anchors_recursive(&subdir_path, root, depth + 1, result)?;
    }

    Ok(())
}

/// Checks if a directory contains loose DAZ files (no folder structure)
fn has_loose_daz_files(dir: &Path) -> bool {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && is_daz_file(&path) {
                return true;
            }
        }
    }
    false
}

/// Counts files and total size recursively
fn count_content(dir: &Path) -> (usize, u64) {
    let mut file_count = 0;
    let mut total_size = 0u64;

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_file() {
                file_count += 1;
                if let Ok(meta) = fs::metadata(&path) {
                    total_size += meta.len();
                }
            } else if path.is_dir() {
                let (sub_count, sub_size) = count_content(&path);
                file_count += sub_count;
                total_size += sub_size;
            }
        }
    }

    (file_count, total_size)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_system_junk() {
        assert!(is_system_junk(".DS_Store"));
        assert!(is_system_junk("Thumbs.db"));
        assert!(is_system_junk("__MACOSX"));
        assert!(!is_system_junk("data"));
        assert!(!is_system_junk("MyProduct"));
    }

    #[test]
    fn test_is_daz_file() {
        assert!(is_daz_file(Path::new("character.duf")));
        assert!(is_daz_file(Path::new("morph.dsf")));
        assert!(is_daz_file(Path::new("model.obj")));
        assert!(!is_daz_file(Path::new("readme.txt")));
        // Textures are considered DAZ-related to keep them during imports
        assert!(is_daz_file(Path::new("promo.png")));
    }
}
