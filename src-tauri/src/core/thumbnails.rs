//! Thumbnail discovery for DAZ products.
//!
//! Finds the best thumbnail image for a product by scanning its root directory
//! with a priority-based scoring system.

use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Image extensions considered as thumbnails (case-insensitive match).
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg"];

/// Keywords that indicate a file is likely a promotional/preview image.
const THUMBNAIL_KEYWORDS: &[&str] = &[
    "icon", "thumb", "thumbnail", "cover", "promo", "preview", "main",
];

/// Returns true if the file extension is an image type we care about.
pub fn is_image_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTENSIONS.iter().any(|ext| e.eq_ignore_ascii_case(ext)))
        .unwrap_or(false)
}

/// Searches for the best thumbnail image for a product.
///
/// Priority order:
/// 1. Image with the exact same stem as the first `.duf` file (`.png`, `.tip.png`, `.jpg`)
/// 2. Image containing a keyword (`icon`, `thumb`, `cover`, `promo`, etc.)
///    or the product name in its filename, within the product root (depth ≤ 2)
/// 3. Any image file in the product root directory (depth ≤ 2)
///
/// Returns `None` if no image is found.
pub fn find_best_thumbnail(
    product_root: &Path,
    product_name: &str,
    duf_files: &[PathBuf],
) -> Option<PathBuf> {
    // --- Priority 1: exact stem match next to the first .duf ---
    if let Some(first_duf) = duf_files.first() {
        // <stem>.png
        let stem_png = first_duf.with_extension("png");
        if stem_png.exists() {
            return Some(stem_png);
        }
        // <stem>.tip.png  (DAZ convention for tooltip images)
        if let Some(name) = first_duf.file_name().and_then(|n| n.to_str()) {
            let tip_png = first_duf.with_file_name(format!("{}.tip.png", name));
            if tip_png.exists() {
                return Some(tip_png);
            }
        }
        // <stem>.jpg
        let stem_jpg = first_duf.with_extension("jpg");
        if stem_jpg.exists() {
            return Some(stem_jpg);
        }
    }

    // --- Collect all images in product root (depth ≤ 2) ---
    if !product_root.exists() {
        return None;
    }

    let images: Vec<PathBuf> = WalkDir::new(product_root)
        .max_depth(2)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && is_image_file(e.path()))
        // Exclude runtime texture directories — those are material maps, not thumbnails
        .filter(|e| {
            let p = e.path().to_string_lossy().to_lowercase();
            !p.contains("runtime") && !p.contains("textures") && !p.contains("data")
        })
        .map(|e| e.into_path())
        .collect();

    if images.is_empty() {
        // Fallback: also check the parent of product_root (for deeply nested products)
        if let Some(parent) = product_root.parent() {
            return find_image_in_dir(parent, product_name);
        }
        return None;
    }

    let name_lower = product_name.to_lowercase();
    // Extract the first word of the product name for fuzzy matching (e.g. "Amala" from "Amala G9")
    let name_first_word = name_lower.split_whitespace().next().unwrap_or(&name_lower);

    // --- Priority 2: keyword or product name match ---
    let mut best_keyword: Option<(i32, &PathBuf)> = None;

    for img in &images {
        let file_lower = img
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();

        let mut score = 0i32;

        // Keyword boost
        for kw in THUMBNAIL_KEYWORDS {
            if file_lower.contains(kw) {
                score += 10;
                break;
            }
        }

        // Product name boost (full name or first word)
        if file_lower.contains(&name_lower) {
            score += 8;
        } else if name_first_word.len() >= 3 && file_lower.contains(name_first_word) {
            score += 5;
        }

        // Prefer .png over .jpg
        if file_lower.ends_with(".png") && !file_lower.ends_with(".tip.png") {
            score += 1;
        }

        if score > 0 {
            if best_keyword.as_ref().map_or(true, |(s, _)| score > *s) {
                best_keyword = Some((score, img));
            }
        }
    }

    if let Some((_, img)) = best_keyword {
        return Some(img.clone());
    }

    // --- Priority 3: any image in the product root ---
    // Prefer images at root level (depth 0) over subdirectories
    let root_level = images.iter().find(|img| img.parent() == Some(product_root));
    if let Some(img) = root_level {
        return Some(img.clone());
    }

    // Last resort: first image found anywhere in the tree
    images.into_iter().next()
}

/// Searches a single directory (non-recursive) for an image matching the product name.
pub fn find_image_in_dir(dir: &Path, product_name: &str) -> Option<PathBuf> {
    let name_lower = product_name.to_lowercase();
    let read_dir = fs::read_dir(dir).ok()?;

    let mut fallback: Option<PathBuf> = None;

    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() || !is_image_file(&path) {
            continue;
        }
        let file_lower = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();

        if file_lower.contains(&name_lower) {
            return Some(path);
        }
        if fallback.is_none() {
            fallback = Some(path);
        }
    }

    fallback
}
