//! Auto-import orchestrator
//!
//! Validates that a detected archive is a DAZ archive (not a random download)
//! and emits events so the frontend can decide whether to process it.

use crate::core::extractor::ArchiveFormat;
use serde::Serialize;
use std::io::Read;
use std::path::Path;
use tracing::{debug, info};

/// Known DAZ anchor folder names (case-sensitive, matching DAZ conventions)
const DAZ_INDICATORS: &[&str] = &[
    "Content/",
    "content/",
    "Runtime/",
    "runtime/",
    "data/",
    "People/",
    "Environments/",
    "Props/",
    "Light Presets/",
    "Render Presets/",
    "Shader Presets/",
    "Camera Presets/",
    "Pose Presets/",
    "Materials/",
    "Scripts/",
    "ReadMe's/",
    "Manifest.dsx",
    "Supplement.dsx",
];

/// Result of archive validation
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveValidation {
    /// Path to the archive
    pub path: String,
    /// File name
    pub file_name: String,
    /// Whether it looks like a DAZ archive
    pub is_daz_archive: bool,
    /// Detected archive format
    pub format: Option<String>,
    /// Reason for the decision
    pub reason: String,
    /// File size in bytes
    pub size_bytes: u64,
}

/// Validates whether an archive file appears to be a DAZ content archive.
///
/// Checks by listing entries (for ZIP/7z) or using heuristics for RAR.
/// Returns a validation result the frontend can use to decide what to do.
pub fn validate_archive(path: &Path) -> ArchiveValidation {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    let format = ArchiveFormat::from_extension(path);
    let format_str = format.map(|f| f.extension().to_string());

    if format.is_none() {
        return ArchiveValidation {
            path: path.to_string_lossy().to_string(),
            file_name,
            is_daz_archive: false,
            format: format_str,
            reason: "Not a supported archive format".into(),
            size_bytes,
        };
    }

    // Try to peek inside the archive to check for DAZ content
    let (is_daz, reason) = match format.unwrap() {
        ArchiveFormat::Zip => check_zip_for_daz(path),
        ArchiveFormat::SevenZip => check_7z_for_daz(path),
        ArchiveFormat::Rar => check_rar_for_daz(path),
    };

    info!(
        "Archive validation: {} -> is_daz={}, reason={}",
        file_name, is_daz, reason
    );

    ArchiveValidation {
        path: path.to_string_lossy().to_string(),
        file_name,
        is_daz_archive: is_daz,
        format: format_str,
        reason,
        size_bytes,
    }
}

/// Check a ZIP archive for DAZ content indicators
fn check_zip_for_daz(path: &Path) -> (bool, String) {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => return (false, format!("Cannot open: {}", e)),
    };

    let mut archive = match zip::ZipArchive::new(file) {
        Ok(a) => a,
        Err(e) => return (false, format!("Invalid ZIP: {}", e)),
    };

    // Check first 200 entries for DAZ indicators
    let check_count = archive.len().min(200);
    for i in 0..check_count {
        if let Ok(entry) = archive.by_index(i) {
            let name = entry.name();
            for indicator in DAZ_INDICATORS {
                if name.starts_with(indicator) || name.contains(&format!("/{}", indicator)) {
                    return (true, format!("Found DAZ indicator: {}", indicator));
                }
            }
        }
    }

    (false, "No DAZ content indicators found".into())
}

/// Check a 7z archive for DAZ content indicators
fn check_7z_for_daz(path: &Path) -> (bool, String) {
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => return (false, format!("Cannot open: {}", e)),
    };

    let len = match file.metadata() {
        Ok(m) => m.len(),
        Err(e) => return (false, format!("Cannot read metadata: {}", e)),
    };

    let archive = match sevenz_rust::Archive::read(&mut file, len, &[]) {
        Ok(a) => a,
        Err(e) => return (false, format!("Invalid 7z: {}", e)),
    };

    for entry in archive.files.iter().take(200) {
        if entry.has_stream {
            let name = &entry.name;
            for indicator in DAZ_INDICATORS {
                if name.starts_with(indicator) || name.contains(&format!("/{}", indicator)) {
                    return (true, format!("Found DAZ indicator: {}", indicator));
                }
            }
        }
    }

    (false, "No DAZ content indicators found".into())
}

/// Check a RAR archive for DAZ content indicators.
/// Uses magic bytes check + filename heuristics since we can't easily
/// list RAR contents without the full unrar extraction flow.
fn check_rar_for_daz(path: &Path) -> (bool, String) {
    // First verify it's actually a RAR file (magic bytes)
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => return (false, format!("Cannot open: {}", e)),
    };

    let mut magic = [0u8; 7];
    if file.read_exact(&mut magic).is_err() {
        return (false, "File too small to be RAR".into());
    }

    // RAR4: "Rar!\x1a\x07\x00", RAR5: "Rar!\x1a\x07\x01"
    if &magic[..4] != b"Rar!" {
        return (false, "Not a valid RAR file (bad magic bytes)".into());
    }

    // For RAR, we can't easily list entries without unrar.
    // Use filename heuristics: DAZ products typically have specific naming patterns.
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();

    // Common DAZ vendor/product naming patterns
    let daz_name_hints = [
        "genesis", "g9", "g8", "g3", "dforce", "d-force", "daz3d", "daz_", "iray",
        "hd morph", "character", "hair", "clothing", "outfit", "pose", "scene",
        "texture", "material", "shader", "light", "hdri", "environment", "prop",
    ];

    for hint in &daz_name_hints {
        if file_name.contains(hint) {
            return (
                true,
                format!("RAR filename contains DAZ indicator: {}", hint),
            );
        }
    }

    // If we can't determine, assume it might be DAZ (user put it in the watch folder)
    debug!(
        "RAR file {} has no obvious DAZ indicators in name, treating as potential DAZ archive",
        file_name
    );
    (
        true,
        "RAR file in watched folder — assumed DAZ content".into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_validate_non_archive() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("readme.txt");
        std::fs::write(&path, "hello").unwrap();
        let result = validate_archive(&path);
        assert!(!result.is_daz_archive);
        assert_eq!(result.reason, "Not a supported archive format");
    }

    #[test]
    fn test_validate_zip_with_daz_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.zip");
        let file = std::fs::File::create(&path).unwrap();
        let mut zip_writer = zip::ZipWriter::new(file);
        zip_writer
            .start_file("Content/data/test.dsf", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip_writer.write_all(b"test").unwrap();
        zip_writer.finish().unwrap();

        let result = validate_archive(&path);
        assert!(result.is_daz_archive);
        assert!(result.reason.contains("Content/"));
    }

    #[test]
    fn test_validate_zip_without_daz_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.zip");
        let file = std::fs::File::create(&path).unwrap();
        let mut zip_writer = zip::ZipWriter::new(file);
        zip_writer
            .start_file("src/main.rs", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip_writer.write_all(b"fn main() {}").unwrap();
        zip_writer.finish().unwrap();

        let result = validate_archive(&path);
        assert!(!result.is_daz_archive);
    }
}
