//! # Bulk Download Engine
//!
//! Downloads DAZ content from Google Drive and MediaFire links.
//! Supports parallel downloads, resume, progress tracking, and
//! automatic integration with the import pipeline.
//!
//! ## Architecture
//!
//! - `mod.rs`: Public types, URL parsing, orchestrator
//! - `http.rs`: HTTP download engine with resume (Range headers)
//! - `gdrive.rs`: Google Drive URL resolution
//! - `mediafire.rs`: MediaFire page scraping → direct download link

mod gdrive;
mod http;
mod mediafire;

use crate::error::{AppError, AppResult};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, Semaphore};
use tracing::{error, info, warn};

// Re-exports
pub use http::download_file;

// ============================================================================
// Types
// ============================================================================

/// Supported download services
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadService {
    GoogleDrive,
    MediaFire,
}

impl std::fmt::Display for DownloadService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GoogleDrive => write!(f, "gdrive"),
            Self::MediaFire => write!(f, "mediafire"),
        }
    }
}

/// A parsed download link
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadLink {
    /// Original URL
    pub url: String,
    /// Detected service
    pub service: DownloadService,
    /// Google Drive file ID (if applicable)
    pub gdrive_id: Option<String>,
}

/// Status of a single download
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadStatus {
    Pending,
    Downloading { progress_bytes: u64, total_bytes: Option<u64> },
    Completed { file_name: String, file_size: u64, duration_secs: f64 },
    Failed { error: String },
    Skipped { reason: String },
}

/// Result of a single download
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResult {
    pub index: usize,
    pub url: String,
    pub service: DownloadService,
    pub status: DownloadStatus,
}

/// Progress event emitted during downloads
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgressEvent {
    /// Index in the batch
    pub index: usize,
    /// Total number of downloads
    pub total: usize,
    /// File being downloaded
    pub file_name: Option<String>,
    /// Current status
    pub status: DownloadStatus,
}

/// Summary of a batch download
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadSummary {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub skipped: usize,
    pub total_bytes: u64,
    pub total_duration_secs: f64,
    pub results: Vec<DownloadResult>,
}

/// Options for a batch download
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadOptions {
    /// Destination folder for downloaded files
    pub dest_dir: String,
    /// Max parallel downloads
    #[serde(default = "default_workers")]
    pub workers: usize,
    /// Retries per link
    #[serde(default = "default_retries")]
    pub retries: usize,
    /// HTTP timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_workers() -> usize { 4 }
fn default_retries() -> usize { 3 }
fn default_timeout() -> u64 { 120 }

// ============================================================================
// URL Parsing
// ============================================================================

/// Parse URLs from raw text (paste box content).
/// Extracts Google Drive and MediaFire links, deduplicates, preserves order.
pub fn parse_urls(text: &str) -> Vec<DownloadLink> {
    let url_re = Regex::new(
        r"https?://(?:drive\.google\.com|drive\.usercontent\.google\.com|(?:www\.)?mediafire\.com)/\S+"
    ).expect("invalid URL regex");

    let trailing_junk: &[char] = &[')', ']', '}', '>', '.', ',', ';', '\'', '"'];

    let mut seen = HashSet::new();
    let mut gdrive_ids_seen = HashSet::new();
    let mut links = Vec::new();

    for line in text.lines() {
        for mat in url_re.find_iter(line) {
            let url = mat.as_str().trim_end_matches(trailing_junk).to_string();

            if seen.contains(&url) {
                continue;
            }

            let service = classify_url(&url);
            if service.is_none() {
                continue;
            }
            let service = service.unwrap();

            // Deduplicate Google Drive by file ID
            let gdrive_id = if service == DownloadService::GoogleDrive {
                let id = extract_gdrive_id(&url);
                if let Some(ref gid) = id {
                    if gdrive_ids_seen.contains(gid) {
                        continue;
                    }
                    gdrive_ids_seen.insert(gid.clone());
                }
                id
            } else {
                None
            };

            seen.insert(url.clone());
            links.push(DownloadLink {
                url,
                service,
                gdrive_id,
            });
        }
    }

    links
}

/// Classify a URL as Google Drive or MediaFire
fn classify_url(url: &str) -> Option<DownloadService> {
    let lower = url.to_lowercase();
    if lower.contains("drive.google.com") || lower.contains("drive.usercontent.google.com") {
        Some(DownloadService::GoogleDrive)
    } else if lower.contains("mediafire.com") {
        Some(DownloadService::MediaFire)
    } else {
        None
    }
}

/// Extract Google Drive file ID from URL
fn extract_gdrive_id(url: &str) -> Option<String> {
    let patterns = [
        r"drive\.google\.com/file/d/([^/?#]+)",
        r"drive\.google\.com/open\?id=([^&]+)",
        r"drive\.google\.com/uc\?id=([^&]+)",
        r"drive\.google\.com/uc\?export=download&id=([^&]+)",
        r"[?&]id=([^&]+)",
    ];

    for pat in &patterns {
        if let Ok(re) = Regex::new(pat) {
            if let Some(caps) = re.captures(url) {
                if let Some(m) = caps.get(1) {
                    return Some(m.as_str().to_string());
                }
            }
        }
    }
    None
}

// ============================================================================
// Download Orchestrator
// ============================================================================

/// Run a batch of downloads with progress events.
pub async fn run_batch_downloads(
    links: Vec<DownloadLink>,
    options: DownloadOptions,
    event_tx: mpsc::UnboundedSender<DownloadProgressEvent>,
) -> AppResult<DownloadSummary> {
    let dest_dir = PathBuf::from(&options.dest_dir);
    std::fs::create_dir_all(&dest_dir).map_err(|e| {
        AppError::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("Cannot create dest dir: {}", e)))
    })?;

    let total = links.len();
    let workers = options.workers.clamp(1, 20);
    let semaphore = Arc::new(Semaphore::new(workers));

    // Collect existing files for skip detection
    let existing_files: HashSet<String> = if dest_dir.exists() {
        std::fs::read_dir(&dest_dir)
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().is_file())
                    .filter_map(|e| e.file_name().to_str().map(|s| s.to_lowercase()))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        HashSet::new()
    };
    let existing_files = Arc::new(existing_files);

    info!(
        "Starting batch download: {} links, {} workers, dest={}",
        total, workers, dest_dir.display()
    );

    let mut handles = Vec::new();

    for (index, link) in links.into_iter().enumerate() {
        let sem = Arc::clone(&semaphore);
        let dest = dest_dir.clone();
        let tx = event_tx.clone();
        let existing = Arc::clone(&existing_files);
        let retries = options.retries;
        let timeout = options.timeout_secs;

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed");

            // Emit pending → downloading
            let _ = tx.send(DownloadProgressEvent {
                index,
                total,
                file_name: None,
                status: DownloadStatus::Pending,
            });

            let start = Instant::now();
            let result = download_single_link(&link, &dest, retries, timeout, &existing).await;
            let duration = start.elapsed().as_secs_f64();

            let status = match result {
                Ok(status) => status,
                Err(e) => DownloadStatus::Failed {
                    error: e.to_string(),
                },
            };

            // Emit final status
            let file_name = match &status {
                DownloadStatus::Completed { file_name, .. } => Some(file_name.clone()),
                _ => None,
            };
            let _ = tx.send(DownloadProgressEvent {
                index,
                total,
                file_name,
                status: status.clone(),
            });

            DownloadResult {
                index,
                url: link.url,
                service: link.service,
                status,
            }
        });

        handles.push(handle);
    }

    // Collect results
    let mut results = Vec::with_capacity(total);
    for handle in handles {
        match handle.await {
            Ok(result) => results.push(result),
            Err(e) => {
                error!("Download task panicked: {}", e);
            }
        }
    }

    // Build summary
    let mut success = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut total_bytes = 0u64;
    let mut total_duration = 0f64;

    for r in &results {
        match &r.status {
            DownloadStatus::Completed {
                file_size,
                duration_secs,
                ..
            } => {
                success += 1;
                total_bytes += file_size;
                total_duration += duration_secs;
            }
            DownloadStatus::Failed { .. } => failed += 1,
            DownloadStatus::Skipped { .. } => skipped += 1,
            _ => {}
        }
    }

    info!(
        "Batch complete: {} OK, {} failed, {} skipped, {:.1} MB total",
        success,
        failed,
        skipped,
        total_bytes as f64 / (1024.0 * 1024.0)
    );

    Ok(DownloadSummary {
        total,
        success,
        failed,
        skipped,
        total_bytes,
        total_duration_secs: total_duration,
        results,
    })
}

/// Download a single link (dispatches to the right service).
async fn download_single_link(
    link: &DownloadLink,
    dest_dir: &Path,
    retries: usize,
    timeout_secs: u64,
    existing_files: &HashSet<String>,
) -> AppResult<DownloadStatus> {
    match link.service {
        DownloadService::GoogleDrive => {
            let direct_url = gdrive::resolve_gdrive_url(&link.url, link.gdrive_id.as_deref());
            http::download_file(&direct_url, dest_dir, None, retries, timeout_secs, existing_files).await
        }
        DownloadService::MediaFire => {
            let direct_url = mediafire::resolve_mediafire_url(&link.url, timeout_secs).await?;
            let suggested_name = http::filename_from_url(&link.url);
            http::download_file(&direct_url, dest_dir, suggested_name.as_deref(), retries, timeout_secs, existing_files).await
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_urls_gdrive() {
        let text = r#"
            Here are the links:
            https://drive.google.com/file/d/1ABC123/view?usp=sharing
            https://drive.google.com/file/d/2DEF456/view
            Some random text
            https://www.mediafire.com/file/xyz789/test.zip/file
        "#;

        let links = parse_urls(text);
        assert_eq!(links.len(), 3);
        assert_eq!(links[0].service, DownloadService::GoogleDrive);
        assert_eq!(links[0].gdrive_id, Some("1ABC123".into()));
        assert_eq!(links[1].service, DownloadService::GoogleDrive);
        assert_eq!(links[1].gdrive_id, Some("2DEF456".into()));
        assert_eq!(links[2].service, DownloadService::MediaFire);
    }

    #[test]
    fn test_parse_urls_deduplicates() {
        let text = r#"
            https://drive.google.com/file/d/1ABC123/view?usp=sharing
            https://drive.google.com/uc?id=1ABC123
        "#;

        let links = parse_urls(text);
        // Same GDrive ID → deduplicated
        assert_eq!(links.len(), 1);
    }

    #[test]
    fn test_parse_urls_empty() {
        let links = parse_urls("no links here, just text");
        assert!(links.is_empty());
    }

    #[test]
    fn test_classify_url() {
        assert_eq!(
            classify_url("https://drive.google.com/file/d/abc/view"),
            Some(DownloadService::GoogleDrive)
        );
        assert_eq!(
            classify_url("https://www.mediafire.com/file/abc/test.zip"),
            Some(DownloadService::MediaFire)
        );
        assert_eq!(classify_url("https://github.com/test"), None);
    }

    #[test]
    fn test_extract_gdrive_id() {
        assert_eq!(
            extract_gdrive_id("https://drive.google.com/file/d/1ABC123def/view?usp=sharing"),
            Some("1ABC123def".into())
        );
        assert_eq!(
            extract_gdrive_id("https://drive.google.com/uc?id=XYZ789&export=download"),
            Some("XYZ789".into())
        );
        assert_eq!(
            extract_gdrive_id("https://drive.google.com/open?id=TEST_ID"),
            Some("TEST_ID".into())
        );
    }
}
