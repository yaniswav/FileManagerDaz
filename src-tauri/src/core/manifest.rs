//! Parser for DAZ Manifest.dsx and Supplement.dsx files.
//!
//! - **Manifest.dsx**: lists all files belonging to a product (GlobalID + file paths)
//! - **Supplement.dsx**: product metadata (name, install types, tags)
//!
//! These files are embedded in DAZ archives and installed alongside the content.

use crate::error::AppResult;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use tracing::{debug, warn};
use walkdir::WalkDir;

/// File entry from a Manifest.dsx
#[derive(Debug, Clone)]
pub struct ManifestFile {
    /// Relative path (stripped of leading "Content/" prefix)
    pub relative_path: String,
    /// Target (usually "Content")
    pub target: String,
    /// Action (usually "Install")
    pub action: String,
}

/// Parsed data from a Manifest.dsx file
#[derive(Debug, Clone)]
pub struct ManifestData {
    /// DAZ global product ID (UUID)
    pub global_id: Option<String>,
    /// All files listed in the manifest
    pub files: Vec<ManifestFile>,
}

/// Parsed data from a Supplement.dsx file
#[derive(Debug, Clone)]
pub struct SupplementData {
    /// Product display name
    pub product_name: Option<String>,
    /// Install types (e.g., "Content")
    pub install_types: Option<String>,
    /// Product tags (e.g., "DAZStudio4_5")
    pub product_tags: Option<String>,
}

/// Combined metadata from both manifest files
#[derive(Debug, Clone, Default)]
pub struct ProductManifest {
    pub global_id: Option<String>,
    pub product_name: Option<String>,
    pub install_types: Option<String>,
    pub product_tags: Option<String>,
    pub files: Vec<ManifestFile>,
}

/// Extract an attribute value from an XML element by attribute name.
fn attr_value(e: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> Option<String> {
    e.attributes()
        .filter_map(|a| a.ok())
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| String::from_utf8(a.value.to_vec()).ok())
}

/// Strip the "Content/" prefix from a manifest file path.
/// DAZ manifests store paths as "Content/data/..." but in the library they're just "data/..."
fn strip_content_prefix(path: &str) -> String {
    if let Some(stripped) = path.strip_prefix("Content/") {
        stripped.to_string()
    } else if let Some(stripped) = path.strip_prefix("Content\\") {
        stripped.to_string()
    } else {
        path.to_string()
    }
}

/// Parse a Manifest.dsx file.
///
/// Format:
/// ```xml
/// <DAZInstallManifest VERSION="0.1">
///   <GlobalID VALUE="uuid-here"/>
///   <File TARGET="Content" ACTION="Install" VALUE="Content/path/to/file.dsf"/>
/// </DAZInstallManifest>
/// ```
pub fn parse_manifest(path: &Path) -> AppResult<ManifestData> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut xml = Reader::from_reader(reader);
    xml.trim_text(true);

    let mut global_id = None;
    let mut files = Vec::new();
    let mut buf = Vec::new();

    loop {
        match xml.read_event_into(&mut buf) {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                match e.name().as_ref() {
                    b"GlobalID" => {
                        global_id = attr_value(e, b"VALUE");
                    }
                    b"File" => {
                        let target = attr_value(e, b"TARGET").unwrap_or_default();
                        let action = attr_value(e, b"ACTION").unwrap_or_default();
                        if let Some(value) = attr_value(e, b"VALUE") {
                            files.push(ManifestFile {
                                relative_path: strip_content_prefix(&value),
                                target,
                                action,
                            });
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                warn!("XML parse error in {}: {}", path.display(), e);
                break;
            }
            _ => {}
        }
        buf.clear();
    }

    debug!(
        "Parsed manifest {}: global_id={:?}, {} files",
        path.display(),
        global_id,
        files.len()
    );

    Ok(ManifestData { global_id, files })
}

/// Parse a Supplement.dsx file.
///
/// Format:
/// ```xml
/// <ProductSupplement VERSION="0.1">
///   <ProductName VALUE="dForce Heavy Leather Dress for Genesis 9"/>
///   <InstallTypes VALUE="Content"/>
///   <ProductTags VALUE="DAZStudio4_5"/>
/// </ProductSupplement>
/// ```
pub fn parse_supplement(path: &Path) -> AppResult<SupplementData> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut xml = Reader::from_reader(reader);
    xml.trim_text(true);

    let mut data = SupplementData {
        product_name: None,
        install_types: None,
        product_tags: None,
    };
    let mut buf = Vec::new();

    loop {
        match xml.read_event_into(&mut buf) {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                match e.name().as_ref() {
                    b"ProductName" => data.product_name = attr_value(e, b"VALUE"),
                    b"InstallTypes" => data.install_types = attr_value(e, b"VALUE"),
                    b"ProductTags" => data.product_tags = attr_value(e, b"VALUE"),
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                warn!("XML parse error in {}: {}", path.display(), e);
                break;
            }
            _ => {}
        }
        buf.clear();
    }

    debug!("Parsed supplement {}: name={:?}", path.display(), data.product_name);
    Ok(data)
}

/// Find and parse all Manifest.dsx and Supplement.dsx files in a directory tree.
/// Returns a combined ProductManifest with deduplicated file entries.
pub fn parse_product_manifests(dir: &Path) -> AppResult<ProductManifest> {
    let mut result = ProductManifest::default();
    let mut seen_paths = std::collections::HashSet::new();

    for entry in WalkDir::new(dir).max_depth(10).into_iter().filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy();
        let path = entry.path();

        if name == "Manifest.dsx" {
            match parse_manifest(path) {
                Ok(manifest) => {
                    if result.global_id.is_none() {
                        result.global_id = manifest.global_id;
                    }
                    for file in manifest.files {
                        if seen_paths.insert(file.relative_path.clone()) {
                            result.files.push(file);
                        }
                    }
                }
                Err(e) => warn!("Failed to parse {}: {}", path.display(), e),
            }
        } else if name == "Supplement.dsx" {
            match parse_supplement(path) {
                Ok(supplement) => {
                    if result.product_name.is_none() {
                        result.product_name = supplement.product_name;
                    }
                    if result.install_types.is_none() {
                        result.install_types = supplement.install_types;
                    }
                    if result.product_tags.is_none() {
                        result.product_tags = supplement.product_tags;
                    }
                }
                Err(e) => warn!("Failed to parse {}: {}", path.display(), e),
            }
        }
    }

    debug!(
        "Product manifests from {}: global_id={:?}, name={:?}, {} files",
        dir.display(),
        result.global_id,
        result.product_name,
        result.files.len()
    );

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_manifest(dir: &Path, content: &str) -> std::path::PathBuf {
        let path = dir.join("Manifest.dsx");
        let mut f = File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    fn write_supplement(dir: &Path, content: &str) -> std::path::PathBuf {
        let path = dir.join("Supplement.dsx");
        let mut f = File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn test_parse_manifest_basic() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_manifest(
            dir.path(),
            r#"<DAZInstallManifest VERSION="0.1">
                <GlobalID VALUE="abc-123"/>
                <File TARGET="Content" ACTION="Install" VALUE="Content/data/test.dsf"/>
                <File TARGET="Content" ACTION="Install" VALUE="Content/Runtime/Textures/t.jpg"/>
            </DAZInstallManifest>"#,
        );

        let result = parse_manifest(&path).unwrap();
        assert_eq!(result.global_id.as_deref(), Some("abc-123"));
        assert_eq!(result.files.len(), 2);
        assert_eq!(result.files[0].relative_path, "data/test.dsf");
        assert_eq!(result.files[0].target, "Content");
        assert_eq!(result.files[1].relative_path, "Runtime/Textures/t.jpg");
    }

    #[test]
    fn test_parse_manifest_no_global_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_manifest(
            dir.path(),
            r#"<DAZInstallManifest VERSION="0.1">
                <File TARGET="Content" ACTION="Install" VALUE="Content/data/test.dsf"/>
            </DAZInstallManifest>"#,
        );

        let result = parse_manifest(&path).unwrap();
        assert!(result.global_id.is_none());
        assert_eq!(result.files.len(), 1);
    }

    #[test]
    fn test_parse_manifest_strips_content_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_manifest(
            dir.path(),
            r#"<DAZInstallManifest VERSION="0.1">
                <File TARGET="Content" ACTION="Install" VALUE="Content/People/Genesis 9/test.duf"/>
                <File TARGET="Content" ACTION="Install" VALUE="data/no_prefix.dsf"/>
            </DAZInstallManifest>"#,
        );

        let result = parse_manifest(&path).unwrap();
        assert_eq!(result.files[0].relative_path, "People/Genesis 9/test.duf");
        assert_eq!(result.files[1].relative_path, "data/no_prefix.dsf");
    }

    #[test]
    fn test_parse_supplement_basic() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_supplement(
            dir.path(),
            r#"<ProductSupplement VERSION="0.1">
                <ProductName VALUE="My Cool Product"/>
                <InstallTypes VALUE="Content"/>
                <ProductTags VALUE="DAZStudio4_5"/>
            </ProductSupplement>"#,
        );

        let result = parse_supplement(&path).unwrap();
        assert_eq!(result.product_name.as_deref(), Some("My Cool Product"));
        assert_eq!(result.install_types.as_deref(), Some("Content"));
        assert_eq!(result.product_tags.as_deref(), Some("DAZStudio4_5"));
    }

    #[test]
    fn test_parse_product_manifests_combined() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            dir.path(),
            r#"<DAZInstallManifest VERSION="0.1">
                <GlobalID VALUE="uuid-test"/>
                <File TARGET="Content" ACTION="Install" VALUE="Content/data/file1.dsf"/>
            </DAZInstallManifest>"#,
        );
        write_supplement(
            dir.path(),
            r#"<ProductSupplement VERSION="0.1">
                <ProductName VALUE="Test Product"/>
            </ProductSupplement>"#,
        );

        let result = parse_product_manifests(dir.path()).unwrap();
        assert_eq!(result.global_id.as_deref(), Some("uuid-test"));
        assert_eq!(result.product_name.as_deref(), Some("Test Product"));
        assert_eq!(result.files.len(), 1);
    }

    #[test]
    fn test_parse_product_manifests_deduplicates() {
        let dir = tempfile::tempdir().unwrap();
        // Two manifests with overlapping files
        write_manifest(
            dir.path(),
            r#"<DAZInstallManifest VERSION="0.1">
                <File TARGET="Content" ACTION="Install" VALUE="Content/data/shared.dsf"/>
                <File TARGET="Content" ACTION="Install" VALUE="Content/data/unique1.dsf"/>
            </DAZInstallManifest>"#,
        );
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        write_manifest(
            &sub,
            r#"<DAZInstallManifest VERSION="0.1">
                <File TARGET="Content" ACTION="Install" VALUE="Content/data/shared.dsf"/>
                <File TARGET="Content" ACTION="Install" VALUE="Content/data/unique2.dsf"/>
            </DAZInstallManifest>"#,
        );

        let result = parse_product_manifests(dir.path()).unwrap();
        assert_eq!(result.files.len(), 3); // shared + unique1 + unique2
    }

    #[test]
    fn test_strip_content_prefix() {
        assert_eq!(strip_content_prefix("Content/data/test.dsf"), "data/test.dsf");
        assert_eq!(strip_content_prefix("Content\\data\\test.dsf"), "data\\test.dsf");
        assert_eq!(strip_content_prefix("data/test.dsf"), "data/test.dsf");
        assert_eq!(strip_content_prefix(""), "");
    }
}
