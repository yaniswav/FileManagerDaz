//! DAZ Studio content analysis module
//!
//! Analyzes files and directory structure to detect
//! DAZ content type and suggest metadata.

use crate::error::AppResult;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use tracing::{debug, info};
use walkdir::WalkDir;

// ============================================================================
// Constants: Standard DAZ structure
// ============================================================================

/// Recognized standard DAZ folders
pub const DAZ_CONTENT_FOLDERS: &[&str] = &[
    "data",
    "Runtime",
    "People",
    "Props",
    "Environments",
    "Lights",
    "Cameras",
    "Presets",
    "Scripts",
    "Shader Presets",
    "Render Presets",
    "Light Presets",
    "Camera Presets",
    "Scene Subsets",
    "Scenes",
    "aniBlocks",
    "Support",
    "Templates",
    "Documentation",
    "Textures",
    "Render Settings",
];

/// DAZ file extensions
pub const DAZ_FILE_EXTENSIONS: &[&str] = &[
    "duf",  // DAZ User File (scenes, presets, morphs...)
    "dsf",  // DAZ Scene File (geometry, morphs)
    "dsa",  // DAZ Script (legacy format)
    "dse",  // DAZ Script Encrypted
    "ds",   // DAZ Script
    "daz",  // Legacy DAZ file
    "dbz",  // DAZ Binary File
    "djl",  // DAZ JSON Lite
    "dhdm", // DAZ HD Morph
    "dst",  // DAZ Shader Template
    "tip",  // Tip of the Day
];

/// Common texture extensions
pub const TEXTURE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "tif", "tiff", "bmp", "exr", "hdr", "webp",
];

// ============================================================================
// Public types
// ============================================================================

/// Detected DAZ content type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentType {
    /// Character (Genesis, etc.)
    Character,
    /// Clothing/Accessory
    Clothing,
    /// Hair
    Hair,
    /// Prop (object)
    Prop,
    /// Environment/Scene
    Environment,
    /// Pose/Animation
    Pose,
    /// Lights
    Light,
    /// Materials/Shaders
    Material,
    /// Scripts
    Script,
    /// Morphs
    Morph,
    /// HDRI
    Hdri,
    /// Other/Mixed
    Other,
}

impl ContentType {
    /// Returns a readable name
    pub fn display_name(&self) -> &'static str {
        match self {
            ContentType::Character => "Character",
            ContentType::Clothing => "Clothing",
            ContentType::Hair => "Hair",
            ContentType::Prop => "Prop",
            ContentType::Environment => "Environment",
            ContentType::Pose => "Pose",
            ContentType::Light => "Light",
            ContentType::Material => "Material",
            ContentType::Script => "Script",
            ContentType::Morph => "Morph",
            ContentType::Hdri => "HDRI",
            ContentType::Other => "Other",
        }
    }
}

/// Analysis summary of DAZ content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisSummary {
    /// Is this valid DAZ content?
    pub is_daz_content: bool,
    /// Main detected content type
    pub content_type: ContentType,
    /// Detected standard DAZ folders
    pub daz_folders: Vec<String>,
    /// Optional wrapper folder (if content is wrapped)
    pub wrapper_folder: Option<String>,
    /// Number of DAZ files (.duf, .dsf, etc.)
    pub daz_file_count: usize,
    /// Number of textures
    pub texture_count: usize,
    /// Suggested tags for categorization
    pub suggested_tags: Vec<String>,
    /// Detected figures (Genesis 8, Genesis 9, etc.)
    pub detected_figures: Vec<String>,
    /// Possible warnings
    pub warnings: Vec<String>,
}

impl Default for AnalysisSummary {
    fn default() -> Self {
        Self {
            is_daz_content: false,
            content_type: ContentType::Other,
            daz_folders: Vec::new(),
            wrapper_folder: None,
            daz_file_count: 0,
            texture_count: 0,
            suggested_tags: Vec::new(),
            detected_figures: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

// ============================================================================
// Main function
// ============================================================================

/// Analyzes a directory to detect DAZ content
pub fn analyze_content(path: &Path) -> AppResult<AnalysisSummary> {
    info!("Analyzing content: {:?}", path);

    let mut summary = AnalysisSummary::default();

    // Create set of lowercase DAZ folder names
    let daz_folders_lower: HashSet<String> = DAZ_CONTENT_FOLDERS
        .iter()
        .map(|s| s.to_lowercase())
        .collect();

    // Analyze first level entries
    let root_entries = analyze_root_level(path, &daz_folders_lower)?;
    summary.daz_folders = root_entries.daz_folders;
    summary.wrapper_folder = root_entries.wrapper_folder.clone();
    summary.is_daz_content = !summary.daz_folders.is_empty();

    // Determine effective path (with or without wrapper)
    let effective_path = if let Some(ref wrapper) = root_entries.wrapper_folder {
        path.join(wrapper)
    } else {
        path.to_path_buf()
    };

    // Scan files
    let file_stats = scan_files(&effective_path)?;
    summary.daz_file_count = file_stats.daz_files;
    summary.texture_count = file_stats.textures;

    // Deduce content type BEFORE moving figures
    summary.content_type = detect_content_type(&summary.daz_folders, &file_stats);

    // Now we can move figures
    summary.detected_figures = file_stats.figures;

    // Generate suggested tags
    summary.suggested_tags = generate_tags(&summary);

    // Checks and warnings
    if summary.daz_file_count == 0 && summary.texture_count > 0 {
        summary
            .warnings
            .push("Textures only - no DAZ files".to_string());
    }
    if summary.wrapper_folder.is_some() {
        debug!("Wrapper folder detected: {:?}", summary.wrapper_folder);
    }

    info!(
        "Analysis complete: is_daz={}, type={:?}, files={}, textures={}",
        summary.is_daz_content, summary.content_type, summary.daz_file_count, summary.texture_count
    );

    Ok(summary)
}

// ============================================================================
// Root level analysis
// ============================================================================

struct RootAnalysis {
    daz_folders: Vec<String>,
    wrapper_folder: Option<String>,
}

fn analyze_root_level(path: &Path, daz_folders_set: &HashSet<String>) -> AppResult<RootAnalysis> {
    let mut daz_folders = Vec::new();
    let mut all_entries = Vec::new();

    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                all_entries.push(name.to_string());

                if daz_folders_set.contains(&name.to_lowercase()) {
                    daz_folders.push(name.to_string());
                }
            }
        }
    }

    // If no DAZ folders but only one subfolder, it might be a wrapper
    let wrapper_folder = if daz_folders.is_empty() && all_entries.len() == 1 {
        let wrapper_path = path.join(&all_entries[0]);

        // Check if wrapper contains DAZ folders
        let mut has_daz = false;
        if let Ok(entries) = std::fs::read_dir(&wrapper_path) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    if let Some(name) = entry.file_name().to_str() {
                        if daz_folders_set.contains(&name.to_lowercase()) {
                            daz_folders.push(name.to_string());
                            has_daz = true;
                        }
                    }
                }
            }
        }

        if has_daz {
            Some(all_entries[0].clone())
        } else {
            None
        }
    } else {
        None
    };

    Ok(RootAnalysis {
        daz_folders,
        wrapper_folder,
    })
}

// ============================================================================
// File scanning
// ============================================================================

struct FileStats {
    daz_files: usize,
    textures: usize,
    figures: Vec<String>,
    has_poses: bool,
    has_morphs: bool,
    has_materials: bool,
}

fn scan_files(path: &Path) -> AppResult<FileStats> {
    let mut stats = FileStats {
        daz_files: 0,
        textures: 0,
        figures: Vec::new(),
        has_poses: false,
        has_morphs: false,
        has_materials: false,
    };

    let daz_ext_set: HashSet<&str> = DAZ_FILE_EXTENSIONS.iter().copied().collect();
    let tex_ext_set: HashSet<&str> = TEXTURE_EXTENSIONS.iter().copied().collect();

    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }

        let file_path = entry.path();
        let ext = file_path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        if daz_ext_set.contains(ext.as_str()) {
            stats.daz_files += 1;

            // Detect type by path
            let path_str = file_path.to_string_lossy().to_lowercase();

            if path_str.contains("pose") {
                stats.has_poses = true;
            }
            if path_str.contains("morph") || ext == "dhdm" {
                stats.has_morphs = true;
            }
            if path_str.contains("material") || path_str.contains("shader") {
                stats.has_materials = true;
            }

            // Detect figures
            detect_figure(&path_str, &mut stats.figures);
        } else if tex_ext_set.contains(ext.as_str()) {
            stats.textures += 1;
        }
    }

    // Deduplicate figures
    stats.figures.sort();
    stats.figures.dedup();

    Ok(stats)
}

fn detect_figure(path_lower: &str, figures: &mut Vec<String>) {
    let figure_patterns = [
        ("genesis 9", "Genesis 9"),
        ("genesis9", "Genesis 9"),
        ("g9", "Genesis 9"),
        ("genesis 8.1", "Genesis 8.1"),
        ("genesis8.1", "Genesis 8.1"),
        ("g8.1", "Genesis 8.1"),
        ("genesis 8", "Genesis 8"),
        ("genesis8", "Genesis 8"),
        ("g8", "Genesis 8"),
        ("genesis 3", "Genesis 3"),
        ("genesis3", "Genesis 3"),
        ("g3", "Genesis 3"),
        ("genesis 2", "Genesis 2"),
        ("genesis2", "Genesis 2"),
        ("victoria 8", "Victoria 8"),
        ("michael 8", "Michael 8"),
    ];

    for (pattern, name) in figure_patterns {
        if path_lower.contains(pattern) && !figures.contains(&name.to_string()) {
            figures.push(name.to_string());
        }
    }
}

// ============================================================================
// Content type detection
// ============================================================================

fn detect_content_type(daz_folders: &[String], stats: &FileStats) -> ContentType {
    let folders_lower: Vec<String> = daz_folders.iter().map(|s| s.to_lowercase()).collect();

    // Priority: specific folders
    if folders_lower.contains(&"people".to_string()) {
        // People can contain poses or characters
        if stats.has_poses {
            return ContentType::Pose;
        }
        return ContentType::Character;
    }

    if folders_lower.iter().any(|f| f.contains("hair")) {
        return ContentType::Hair;
    }

    if folders_lower.contains(&"environments".to_string()) {
        return ContentType::Environment;
    }

    if folders_lower.contains(&"props".to_string()) {
        return ContentType::Prop;
    }

    if folders_lower.contains(&"lights".to_string()) {
        return ContentType::Light;
    }

    if folders_lower.contains(&"scripts".to_string()) {
        return ContentType::Script;
    }

    // By file content
    if stats.has_morphs {
        return ContentType::Morph;
    }

    if stats.has_materials {
        return ContentType::Material;
    }

    if stats.has_poses {
        return ContentType::Pose;
    }

    ContentType::Other
}

// ============================================================================
// Tag generation
// ============================================================================

fn generate_tags(summary: &AnalysisSummary) -> Vec<String> {
    let mut tags = Vec::new();

    // Main tag based on type
    tags.push(summary.content_type.display_name().to_string());

    // Tags based on figures
    for figure in &summary.detected_figures {
        tags.push(figure.clone());
    }

    // Tags based on folders
    for folder in &summary.daz_folders {
        let folder_lower = folder.to_lowercase();
        match folder_lower.as_str() {
            "runtime" => {} // Too generic
            "data" => {}    // Too generic
            "presets" => tags.push("Preset".to_string()),
            "scripts" => tags.push("Script".to_string()),
            _ => {}
        }
    }

    // Deduplicate
    tags.sort();
    tags.dedup();
    tags
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_daz_structure(dir: &Path) {
        fs::create_dir_all(dir.join("data")).unwrap();
        fs::create_dir_all(dir.join("Runtime/textures")).unwrap();
        fs::create_dir_all(dir.join("People/Genesis 9/Characters")).unwrap();

        fs::write(dir.join("People/Genesis 9/Characters/Test.duf"), "{}").unwrap();
        fs::write(dir.join("Runtime/textures/skin.jpg"), "fake").unwrap();
    }

    fn create_wrapped_structure(dir: &Path) {
        let wrapper = dir.join("MyProduct");
        fs::create_dir_all(wrapper.join("data")).unwrap();
        fs::create_dir_all(wrapper.join("Runtime")).unwrap();
        fs::write(wrapper.join("data/test.duf"), "{}").unwrap();
    }

    #[test]
    fn test_analyze_daz_content() {
        let temp_dir = TempDir::new().unwrap();
        create_daz_structure(temp_dir.path());

        let result = analyze_content(temp_dir.path()).unwrap();

        assert!(result.is_daz_content);
        assert!(result
            .daz_folders
            .iter()
            .any(|f| f.to_lowercase() == "data"));
        assert!(result
            .daz_folders
            .iter()
            .any(|f| f.to_lowercase() == "runtime"));
        assert!(result
            .daz_folders
            .iter()
            .any(|f| f.to_lowercase() == "people"));
        assert_eq!(result.content_type, ContentType::Character);
        assert!(result.detected_figures.contains(&"Genesis 9".to_string()));
    }

    #[test]
    fn test_analyze_wrapped_content() {
        let temp_dir = TempDir::new().unwrap();
        create_wrapped_structure(temp_dir.path());

        let result = analyze_content(temp_dir.path()).unwrap();

        assert!(result.is_daz_content);
        assert_eq!(result.wrapper_folder, Some("MyProduct".to_string()));
    }

    #[test]
    fn test_content_type_detection() {
        assert_eq!(
            detect_content_type(
                &["People".to_string()],
                &FileStats {
                    daz_files: 1,
                    textures: 0,
                    figures: vec![],
                    has_poses: false,
                    has_morphs: false,
                    has_materials: false,
                }
            ),
            ContentType::Character
        );

        assert_eq!(
            detect_content_type(
                &["Props".to_string()],
                &FileStats {
                    daz_files: 1,
                    textures: 0,
                    figures: vec![],
                    has_poses: false,
                    has_morphs: false,
                    has_materials: false,
                }
            ),
            ContentType::Prop
        );
    }
}
