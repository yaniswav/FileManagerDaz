//! Library catalog indexing for DAZ products.
//!
//! Scans `Runtime/Support/*.dsx` metadata files and extracts product data:
//! name, vendor, content types, categories, assets, and support assets.

use crate::error::{AppError, AppResult};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use std::collections::HashSet;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Parsed metadata for a single product from a DSX file.
#[derive(Debug, Clone)]
pub struct CatalogProduct {
    pub name: String,
    pub product_token: Option<String>,
    pub global_id: Option<String>,
    pub store_id: Option<String>,
    pub artists: Vec<String>,
    pub assets: Vec<String>,
    pub support_assets: Vec<String>,
    pub categories: HashSet<String>,
    pub content_types: Vec<String>,
}

impl CatalogProduct {
    fn new(name: String) -> Self {
        Self {
            name,
            product_token: None,
            global_id: None,
            store_id: None,
            artists: Vec::new(),
            assets: Vec::new(),
            support_assets: Vec::new(),
            categories: HashSet::new(),
            content_types: Vec::new(),
        }
    }
}

/// Lists all DSX metadata files under a library's Runtime/Support folder.
pub fn list_support_metadata_files(library_path: &Path) -> AppResult<Vec<PathBuf>> {
    let support_dir = library_path.join("Runtime").join("Support");
    if !support_dir.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    for entry in WalkDir::new(&support_dir).min_depth(1).max_depth(6) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()).map_or(true, |e| !e.eq_ignore_ascii_case("dsx")) {
            continue;
        }

        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            if is_ignored_metadata_file(file_name) {
                continue;
            }
        }

        files.push(path.to_path_buf());
    }

    Ok(files)
}

/// Parses a DSX metadata file and returns all products it declares.
pub fn parse_daz_metadata_file(path: &Path) -> AppResult<Vec<CatalogProduct>> {
    let file = File::open(path)?;
    let mut reader = Reader::from_reader(BufReader::new(file));
    reader.trim_text(true);

    let mut buf = Vec::new();
    let mut products: Vec<CatalogProduct> = Vec::new();
    let mut current: Option<CatalogProduct> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                match e.name().as_ref() {
                    b"Product" => {
                        if let Some(product) = current.take() {
                            products.push(product);
                        }
                        if let Some(name) = attr_value(&e, b"VALUE") {
                            current = Some(CatalogProduct::new(name));
                        }
                    }
                    b"ProductToken" => {
                        if let (Some(product), Some(value)) = (current.as_mut(), attr_value(&e, b"VALUE")) {
                            product.product_token = Some(value);
                        }
                    }
                    b"GlobalID" => {
                        if let (Some(product), Some(value)) = (current.as_mut(), attr_value(&e, b"VALUE")) {
                            product.global_id = Some(value);
                        }
                    }
                    b"StoreID" => {
                        if let (Some(product), Some(value)) = (current.as_mut(), attr_value(&e, b"VALUE")) {
                            product.store_id = Some(value);
                        }
                    }
                    b"Artist" => {
                        if let (Some(product), Some(value)) = (current.as_mut(), attr_value(&e, b"VALUE")) {
                            if !value.trim().is_empty() {
                                product.artists.push(value);
                            }
                        }
                    }
                    b"Asset" => {
                        if let (Some(product), Some(value)) = (current.as_mut(), attr_value(&e, b"VALUE")) {
                            product.assets.push(value);
                        }
                    }
                    b"SupportAsset" => {
                        if let (Some(product), Some(value)) = (current.as_mut(), attr_value(&e, b"VALUE")) {
                            product.support_assets.push(value);
                        }
                    }
                    b"ContentType" => {
                        if let (Some(product), Some(value)) = (current.as_mut(), attr_value(&e, b"VALUE")) {
                            product.content_types.push(value);
                        }
                    }
                    b"Category" => {
                        if let (Some(product), Some(value)) = (current.as_mut(), attr_value(&e, b"VALUE")) {
                            if !value.trim().is_empty() {
                                product.categories.insert(value);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                if e.name().as_ref() == b"Product" {
                    if let Some(product) = current.take() {
                        products.push(product);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(AppError::Other(format!(
                    "Failed to parse metadata {}: {}",
                    path.display(),
                    e
                )));
            }
            _ => {}
        }

        buf.clear();
    }

    if let Some(product) = current.take() {
        products.push(product);
    }

    Ok(products)
}

fn is_ignored_metadata_file(name: &str) -> bool {
    name.eq_ignore_ascii_case("manifest.dsx")
        || name.eq_ignore_ascii_case("supplement.dsx")
        || name.eq_ignore_ascii_case("content.dsx")
}

fn attr_value(element: &BytesStart, key: &[u8]) -> Option<String> {
    for attr in element.attributes().flatten() {
        if attr.key.as_ref() == key {
            if let Ok(value) = attr.unescape_value() {
                return Some(value.into_owned());
            }
        }
    }
    None
}

/// Returns a relative path by trimming a leading slash/backslash if present.
pub fn normalize_rel_path(path: &str) -> PathBuf {
    let trimmed = path.trim_start_matches(&['/', '\\'][..]);
    PathBuf::from(trimmed)
}
