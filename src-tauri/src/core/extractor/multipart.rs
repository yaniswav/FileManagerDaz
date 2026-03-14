//! Multi-part archive detection and resolution
//!
//! Handles split archives in various formats:
//! - RAR: `file.part1.rar`, `file.part2.rar` or `file.rar`, `file.r00`, `file.r01`
//! - ZIP: `file.zip`, `file.z01`, `file.z02`
//!
//! For RAR: UnRAR handles multi-part natively when given the first part.
//! For ZIP: Parts must be concatenated in order before extraction.

use crate::error::{AppError, AppResult};
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use tracing::info;

/// Buffer size for concatenation I/O (256 KB)
const CONCAT_BUFFER_SIZE: usize = 256 * 1024;

/// Information about a multi-part archive
#[derive(Debug, Clone)]
pub struct MultiPartInfo {
    /// The first part (entry point for extraction)
    pub first_part: PathBuf,
    /// All parts in order (including first)
    pub all_parts: Vec<PathBuf>,
    /// The base archive format
    pub format: MultiPartFormat,
}

/// Multi-part archive format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultiPartFormat {
    /// RAR split: .part1.rar, .part2.rar (handled natively by unrar)
    RarPartN,
    /// RAR old-style split: .rar, .r00, .r01 (handled natively by unrar)
    RarOldStyle,
    /// ZIP split: .zip, .z01, .z02 (requires concatenation)
    ZipSplit,
}

/// Checks if a file is a secondary part of a multi-part archive.
///
/// Secondary parts should be skipped during batch processing since
/// only the first part needs to be processed.
pub fn is_secondary_part(path: &Path) -> bool {
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n.to_lowercase(),
        None => return false,
    };

    // RAR .partN.rar where N > 1
    if let Some(part_num) = extract_rar_part_number(&name) {
        return part_num > 1;
    }

    // RAR old-style: .r00, .r01, .r02, ... (these are always secondary, .rar is the first)
    if is_rar_old_style_part(&name) {
        return true;
    }

    // ZIP split: .z01, .z02, ... (these are secondary, .zip is the first)
    if is_zip_split_part(&name) {
        return true;
    }

    false
}

/// Detects if a file is part of a multi-part archive and returns info about it.
///
/// Returns `None` if the file is a regular (non-split) archive.
pub fn detect_multipart(path: &Path) -> Option<MultiPartInfo> {
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return None,
    };
    let name_lower = name.to_lowercase();
    let parent = path.parent()?;

    // Check RAR .partN.rar format
    if let Some(part_num) = extract_rar_part_number(&name_lower) {
        let base = extract_rar_part_base(&name_lower)?;
        let parts = find_rar_part_files(parent, &base);
        if parts.len() > 1 {
            return Some(MultiPartInfo {
                first_part: parts[0].clone(),
                all_parts: parts,
                format: MultiPartFormat::RarPartN,
            });
        }
        // Single .part1.rar with no other parts — treat as normal
        if part_num == 1 {
            return None;
        }
    }

    // Check RAR old-style: .rar + .r00, .r01, ...
    if name_lower.ends_with(".rar") || is_rar_old_style_part(&name_lower) {
        let base = if name_lower.ends_with(".rar") {
            name_lower.trim_end_matches(".rar").to_string()
        } else {
            // .r00 → get base name
            let stem = path.file_stem().and_then(|s| s.to_str())?;
            stem.to_lowercase()
                .rsplit_once('.')
                .map(|(base, _)| base.to_string())
                .unwrap_or_else(|| stem.to_lowercase())
        };

        let old_parts = find_rar_old_style_parts(parent, &base);
        if old_parts.len() > 1 {
            return Some(MultiPartInfo {
                first_part: old_parts[0].clone(),
                all_parts: old_parts,
                format: MultiPartFormat::RarOldStyle,
            });
        }
    }

    // Check ZIP split: .zip + .z01, .z02, ...
    if name_lower.ends_with(".zip") || is_zip_split_part(&name_lower) {
        let base = if name_lower.ends_with(".zip") {
            name_lower.trim_end_matches(".zip").to_string()
        } else {
            let stem = path.file_stem().and_then(|s| s.to_str())?;
            stem.to_lowercase()
        };

        let zip_parts = find_zip_split_parts(parent, &base);
        if zip_parts.len() > 1 {
            return Some(MultiPartInfo {
                first_part: zip_parts.last()?.clone(), // .zip is last in concat order but first to process
                all_parts: zip_parts,
                format: MultiPartFormat::ZipSplit,
            });
        }
    }

    None
}

/// Reassembles ZIP split parts into a single ZIP file for extraction.
///
/// ZIP split format: data is stored in .z01, .z02, ..., .z99, then .zip (last).
/// Concatenation order: .z01 + .z02 + ... + .zip → complete archive.
///
/// Returns the path to the reassembled ZIP file (in temp dir).
pub fn reassemble_zip_split(info: &MultiPartInfo, dest_dir: &Path) -> AppResult<PathBuf> {
    info!(
        "Reassembling {} ZIP split parts into single archive",
        info.all_parts.len()
    );

    let reassembled_name = info
        .first_part
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("reassembled");
    let reassembled_path = dest_dir.join(format!("{}_reassembled.zip", reassembled_name));

    fs::create_dir_all(dest_dir)?;

    let outfile = File::create(&reassembled_path).map_err(|e| {
        AppError::ZipError(format!("Cannot create reassembled ZIP: {}", e))
    })?;
    let mut writer = BufWriter::with_capacity(CONCAT_BUFFER_SIZE, outfile);

    // Concatenate all parts in order (.z01, .z02, ..., .zip)
    for part_path in &info.all_parts {
        info!("  Appending: {:?}", part_path.file_name().unwrap_or_default());

        let infile = File::open(part_path).map_err(|e| {
            AppError::ZipError(format!(
                "Cannot open split part {}: {}",
                part_path.display(),
                e
            ))
        })?;
        let mut reader = BufReader::with_capacity(CONCAT_BUFFER_SIZE, infile);

        io::copy(&mut reader, &mut writer).map_err(|e| {
            AppError::ZipError(format!(
                "Error concatenating part {}: {}",
                part_path.display(),
                e
            ))
        })?;
    }

    writer.flush().map_err(|e| {
        AppError::ZipError(format!("Error flushing reassembled ZIP: {}", e))
    })?;

    let metadata = fs::metadata(&reassembled_path)?;
    info!(
        "ZIP reassembly complete: {} bytes",
        metadata.len()
    );

    Ok(reassembled_path)
}

// =============================================================================
// RAR partN detection helpers
// =============================================================================

/// Extracts the part number from a `.partN.rar` filename.
/// Returns None if not a partN.rar pattern.
fn extract_rar_part_number(name_lower: &str) -> Option<u32> {
    // Match: something.part1.rar, something.part02.rar, etc.
    if !name_lower.ends_with(".rar") {
        return None;
    }
    let without_rar = name_lower.trim_end_matches(".rar");
    let (_, part_str) = without_rar.rsplit_once(".part")?;
    part_str.parse::<u32>().ok()
}

/// Extracts the base name before `.partN.rar`.
fn extract_rar_part_base(name_lower: &str) -> Option<String> {
    let without_rar = name_lower.trim_end_matches(".rar");
    let (base, _) = without_rar.rsplit_once(".part")?;
    Some(base.to_string())
}

/// Finds all `.partN.rar` files for a given base name, sorted by part number.
fn find_rar_part_files(dir: &Path, base_lower: &str) -> Vec<PathBuf> {
    let mut parts: Vec<(u32, PathBuf)> = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                let name_lower = name.to_lowercase();
                if let Some(file_base) = extract_rar_part_base(&name_lower) {
                    if file_base == base_lower {
                        if let Some(num) = extract_rar_part_number(&name_lower) {
                            parts.push((num, path));
                        }
                    }
                }
            }
        }
    }

    parts.sort_by_key(|(num, _)| *num);
    parts.into_iter().map(|(_, path)| path).collect()
}

// =============================================================================
// RAR old-style detection helpers
// =============================================================================

/// Checks if a filename is a RAR old-style split part (.r00, .r01, ..., .r99).
fn is_rar_old_style_part(name_lower: &str) -> bool {
    if name_lower.len() < 4 {
        return false;
    }
    let ext = &name_lower[name_lower.len() - 4..];
    if !ext.starts_with(".r") {
        return false;
    }
    let digits = &ext[2..];
    digits.len() == 2 && digits.chars().all(|c| c.is_ascii_digit())
}

/// Finds all old-style RAR parts (.rar, .r00, .r01, ...) sorted in order.
fn find_rar_old_style_parts(dir: &Path, base_lower: &str) -> Vec<PathBuf> {
    let mut rar_main: Option<PathBuf> = None;
    let mut r_parts: Vec<(u32, PathBuf)> = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                let name_lower_full = name.to_lowercase();

                // Check for .rar file with matching base
                if name_lower_full == format!("{}.rar", base_lower) {
                    rar_main = Some(path);
                    continue;
                }

                // Check for .rNN files with matching base
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if stem.to_lowercase() == base_lower && is_rar_old_style_part(&name_lower_full)
                    {
                        let ext = &name_lower_full[name_lower_full.len() - 2..];
                        if let Ok(num) = ext.parse::<u32>() {
                            r_parts.push((num, path));
                        }
                    }
                }
            }
        }
    }

    // Order: .rar first, then .r00, .r01, .r02, ...
    let mut result = Vec::new();
    if let Some(main) = rar_main {
        result.push(main);
    }
    r_parts.sort_by_key(|(num, _)| *num);
    result.extend(r_parts.into_iter().map(|(_, path)| path));

    result
}

// =============================================================================
// ZIP split detection helpers
// =============================================================================

/// Checks if a filename is a ZIP split part (.z01, .z02, ..., .z99).
fn is_zip_split_part(name_lower: &str) -> bool {
    if name_lower.len() < 4 {
        return false;
    }
    let ext = &name_lower[name_lower.len() - 4..];
    if !ext.starts_with(".z") {
        return false;
    }
    let digits = &ext[2..];
    digits.len() == 2 && digits.chars().all(|c| c.is_ascii_digit())
}

/// Finds all ZIP split parts (.z01, .z02, ..., .zip) in concatenation order.
/// Order: .z01, .z02, ..., .zip (the .zip file comes last in binary order).
fn find_zip_split_parts(dir: &Path, base_lower: &str) -> Vec<PathBuf> {
    let mut z_parts: Vec<(u32, PathBuf)> = Vec::new();
    let mut zip_main: Option<PathBuf> = None;

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                let name_lower_full = name.to_lowercase();

                // Check for .zip with matching base
                if name_lower_full == format!("{}.zip", base_lower) {
                    zip_main = Some(path);
                    continue;
                }

                // Check for .zNN with matching base
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if stem.to_lowercase() == base_lower && is_zip_split_part(&name_lower_full) {
                        let ext = &name_lower_full[name_lower_full.len() - 2..];
                        if let Ok(num) = ext.parse::<u32>() {
                            z_parts.push((num, path));
                        }
                    }
                }
            }
        }
    }

    // Concatenation order: .z01, .z02, ..., .zip
    z_parts.sort_by_key(|(num, _)| *num);
    let mut result: Vec<PathBuf> = z_parts.into_iter().map(|(_, path)| path).collect();
    if let Some(main) = zip_main {
        result.push(main);
    }

    result
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_is_secondary_part_rar_partn() {
        assert!(!is_secondary_part(Path::new("archive.part1.rar")));
        assert!(is_secondary_part(Path::new("archive.part2.rar")));
        assert!(is_secondary_part(Path::new("archive.part3.rar")));
        assert!(is_secondary_part(Path::new("Archive.Part02.Rar")));
        assert!(!is_secondary_part(Path::new("Archive.Part01.Rar")));
    }

    #[test]
    fn test_is_secondary_part_rar_old() {
        assert!(!is_secondary_part(Path::new("archive.rar")));
        assert!(is_secondary_part(Path::new("archive.r00")));
        assert!(is_secondary_part(Path::new("archive.r01")));
        assert!(is_secondary_part(Path::new("Archive.R05")));
    }

    #[test]
    fn test_is_secondary_part_zip_split() {
        assert!(!is_secondary_part(Path::new("archive.zip")));
        assert!(is_secondary_part(Path::new("archive.z01")));
        assert!(is_secondary_part(Path::new("archive.z02")));
        assert!(is_secondary_part(Path::new("Archive.Z10")));
    }

    #[test]
    fn test_is_secondary_part_normal_archives() {
        assert!(!is_secondary_part(Path::new("archive.zip")));
        assert!(!is_secondary_part(Path::new("archive.rar")));
        assert!(!is_secondary_part(Path::new("archive.7z")));
        assert!(!is_secondary_part(Path::new("readme.txt")));
    }

    #[test]
    fn test_extract_rar_part_number() {
        assert_eq!(extract_rar_part_number("file.part1.rar"), Some(1));
        assert_eq!(extract_rar_part_number("file.part02.rar"), Some(2));
        assert_eq!(extract_rar_part_number("file.part10.rar"), Some(10));
        assert_eq!(extract_rar_part_number("file.rar"), None);
        assert_eq!(extract_rar_part_number("file.zip"), None);
    }

    #[test]
    fn test_is_zip_split_part() {
        assert!(is_zip_split_part("file.z01"));
        assert!(is_zip_split_part("file.z99"));
        assert!(!is_zip_split_part("file.zip"));
        assert!(!is_zip_split_part("file.txt"));
    }

    #[test]
    fn test_detect_multipart_rar_partn() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("archive.part1.rar"), b"fake").unwrap();
        fs::write(temp.path().join("archive.part2.rar"), b"fake").unwrap();
        fs::write(temp.path().join("archive.part3.rar"), b"fake").unwrap();

        let info = detect_multipart(&temp.path().join("archive.part1.rar"));
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.all_parts.len(), 3);
        assert_eq!(info.format, MultiPartFormat::RarPartN);
        assert!(info.first_part.to_string_lossy().contains("part1"));
    }

    #[test]
    fn test_detect_multipart_zip_split() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("archive.z01"), b"fake").unwrap();
        fs::write(temp.path().join("archive.z02"), b"fake").unwrap();
        fs::write(temp.path().join("archive.zip"), b"fake").unwrap();

        let info = detect_multipart(&temp.path().join("archive.zip"));
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.all_parts.len(), 3);
        assert_eq!(info.format, MultiPartFormat::ZipSplit);
    }

    #[test]
    fn test_detect_multipart_single_archive() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("single.zip"), b"fake").unwrap();

        let info = detect_multipart(&temp.path().join("single.zip"));
        assert!(info.is_none());
    }
}
