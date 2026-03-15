//! HTTP download engine with resume support
//!
//! Downloads files with:
//! - Range header resume (`.part` files)
//! - Content-Disposition filename detection
//! - HTML error page detection
//! - Streaming chunks for large files

use super::DownloadStatus;
use crate::error::{AppError, AppResult};
use reqwest::{Client, Url};
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const CHUNK_SIZE: usize = 2 * 1024 * 1024; // 2 MB

/// Download a file via HTTP with resume support.
///
/// Returns `DownloadStatus::Completed` on success, `::Skipped` if file exists,
/// or `::Failed` on error.
pub async fn download_file(
    url: &str,
    dest_dir: &Path,
    suggested_name: Option<&str>,
    retries: usize,
    timeout_secs: u64,
    existing_files: &HashSet<String>,
) -> AppResult<DownloadStatus> {
    let client = Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(timeout_secs))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| AppError::Config(format!("HTTP client error: {}", e)))?;

    let mut last_error = String::new();

    for attempt in 1..=retries {
        match try_download(&client, url, dest_dir, suggested_name, existing_files).await {
            Ok(status) => return Ok(status),
            Err(e) => {
                last_error = e.to_string();
                if attempt < retries {
                    let wait = std::cmp::min(2 * attempt as u64, 30);
                    warn!(
                        "Download attempt {}/{} failed: {}. Retry in {}s",
                        attempt, retries, e, wait
                    );
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                }
            }
        }
    }

    Ok(DownloadStatus::Failed { error: last_error })
}

/// Single download attempt with resume.
async fn try_download(
    client: &Client,
    url: &str,
    dest_dir: &Path,
    suggested_name: Option<&str>,
    existing_files: &HashSet<String>,
) -> AppResult<DownloadStatus> {
    let safe_name = suggested_name
        .map(|n| sanitize_filename(n))
        .unwrap_or_else(|| filename_from_url(url).unwrap_or_else(|| format!("{}.bin", uuid::Uuid::new_v4())));

    let part_path = dest_dir.join(format!("{}.part", safe_name));

    // Check for resume
    let resume_pos = if part_path.exists() {
        std::fs::metadata(&part_path)
            .map(|m| m.len())
            .unwrap_or(0)
    } else {
        0
    };

    let mut request = client.get(url);
    if resume_pos > 0 {
        request = request.header("Range", format!("bytes={}-", resume_pos));
        debug!("Attempting resume at {} bytes for {}", resume_pos, safe_name);
    }

    let resp = request
        .send()
        .await
        .map_err(|e| AppError::Config(format!("HTTP request failed: {}", e)))?;

    let status_code = resp.status().as_u16();

    // Handle response codes
    let is_resuming = match status_code {
        206 => true,
        200 => {
            if resume_pos > 0 {
                debug!("Server ignored resume, overwriting partial file");
            }
            false
        }
        416 => {
            warn!("Resume rejected (416), restarting download");
            let _ = std::fs::remove_file(&part_path);
            return Err(AppError::Config("Range not satisfiable, retry needed".into()));
        }
        _ => {
            return Err(AppError::Config(format!(
                "HTTP error {}: {}",
                status_code,
                resp.status().canonical_reason().unwrap_or("Unknown")
            )));
        }
    };

    // Determine final filename from Content-Disposition
    let final_name = filename_from_headers(resp.headers())
        .map(|n| sanitize_filename(&n))
        .unwrap_or_else(|| safe_name.clone());

    // Check if already exists (skip)
    if !is_resuming && existing_files.contains(&final_name.to_lowercase()) {
        info!("File already exists, skipping: {}", final_name);
        let _ = std::fs::remove_file(&part_path);
        return Ok(DownloadStatus::Skipped {
            reason: format!("File already exists: {}", final_name),
        });
    }

    // Check for HTML error page
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    if content_type.contains("text/html") {
        return Err(AppError::Config(
            "Got HTML page instead of file (possible error page)".into(),
        ));
    }

    let total_size = resp
        .content_length()
        .or_else(|| {
            resp.headers()
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse().ok())
        });

    let start = Instant::now();

    // Stream download to .part file
    {
        let mut file = if is_resuming {
            std::fs::OpenOptions::new()
                .append(true)
                .open(&part_path)
                .map_err(|e| AppError::Io(e))?
        } else {
            std::fs::File::create(&part_path).map_err(|e| AppError::Io(e))?
        };

        let mut stream = resp.bytes_stream();
        use futures::StreamExt;

        let mut downloaded = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| {
                AppError::Config(format!("Download stream error: {}", e))
            })?;
            file.write_all(&chunk).map_err(|e| AppError::Io(e))?;
            downloaded += chunk.len() as u64;
        }

        file.flush().map_err(|e| AppError::Io(e))?;

        // Validate download completeness
        if let Some(expected) = total_size {
            let actual = if is_resuming {
                resume_pos + downloaded
            } else {
                downloaded
            };
            if actual < expected {
                return Err(AppError::Config(format!(
                    "Incomplete download: {}/{} bytes",
                    actual, expected
                )));
            }
        }
    }

    // Finalize: rename .part → final name
    let final_path = ensure_unique(dest_dir.join(&final_name));
    std::fs::rename(&part_path, &final_path).map_err(|e| AppError::Io(e))?;

    // Verify it's not an HTML error page
    if is_html_file(&final_path) {
        let _ = std::fs::remove_file(&final_path);
        return Err(AppError::Config(
            "Downloaded file is HTML (error page)".into(),
        ));
    }

    let file_size = std::fs::metadata(&final_path)
        .map(|m| m.len())
        .unwrap_or(0);
    let duration = start.elapsed().as_secs_f64();

    info!(
        "Downloaded: {} ({:.1} MB, {:.1} MB/s)",
        final_name,
        file_size as f64 / (1024.0 * 1024.0),
        if duration > 0.0 {
            (file_size as f64 / (1024.0 * 1024.0)) / duration
        } else {
            0.0
        }
    );

    Ok(DownloadStatus::Completed {
        file_name: final_name,
        file_size,
        duration_secs: duration,
    })
}

// ============================================================================
// Filename utilities
// ============================================================================

/// Extract filename from Content-Disposition header
fn filename_from_headers(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let cd = headers.get("content-disposition")?.to_str().ok()?;

    // RFC 5987 encoded: filename*=UTF-8''name
    if let Some(caps) = regex::Regex::new(r"filename\*=.*?''(.+)")
        .ok()?
        .captures(cd)
    {
        return caps.get(1).map(|m| {
            percent_decode(m.as_str())
        });
    }

    // Standard: filename="name" or filename=name
    if let Some(caps) = regex::Regex::new(r#"filename="?([^";]+)"?"#)
        .ok()?
        .captures(cd)
    {
        return caps.get(1).map(|m| m.as_str().trim().to_string());
    }

    None
}

/// Extract filename from URL path
pub fn filename_from_url(url: &str) -> Option<String> {
    let path = Url::parse(url).ok()?.path().to_string();
    let tail = path.rsplit('/').next()?;
    let name = tail.split('?').next()?;

    if name.is_empty() || !name.contains('.') {
        return None;
    }

    Some(sanitize_filename(&percent_decode(name)))
}

/// Sanitize a filename for OS compatibility
fn sanitize_filename(name: &str) -> String {
    let decoded = percent_decode(name.trim());
    let sanitized: String = decoded
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            _ => c,
        })
        .collect();

    if sanitized.is_empty() {
        format!("{}.bin", uuid::Uuid::new_v4())
    } else {
        sanitized
    }
}

/// Simple percent-decoding
fn percent_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();

    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                    continue;
                }
            }
            result.push('%');
            result.push_str(&hex);
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }

    result
}

/// If path exists, add numeric suffix to avoid overwrites
fn ensure_unique(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }

    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let parent = path.parent().unwrap_or(Path::new("."));

    for idx in 1..1000 {
        let candidate = if ext.is_empty() {
            parent.join(format!("{}_{}", stem, idx))
        } else {
            parent.join(format!("{}_{}.{}", stem, idx, ext))
        };
        if !candidate.exists() {
            return candidate;
        }
    }

    path
}

/// Check if a file is actually HTML (error page)
fn is_html_file(path: &Path) -> bool {
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() > 10_000 {
            return false; // Large files are unlikely to be error pages
        }
    }

    if let Ok(data) = std::fs::read(path) {
        let header: Vec<u8> = data.iter().take(512).cloned().collect();
        let lower: String = header.iter().map(|&b| (b as char).to_ascii_lowercase()).collect();
        return lower.contains("<html") || lower.contains("<!doctype");
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("test<file>.zip"), "test_file_.zip");
        assert_eq!(sanitize_filename("normal.zip"), "normal.zip");
        assert_eq!(sanitize_filename("a:b/c\\d.rar"), "a_b_c_d.rar");
    }

    #[test]
    fn test_filename_from_url() {
        assert_eq!(
            filename_from_url("https://example.com/files/test.zip"),
            Some("test.zip".into())
        );
        assert_eq!(
            filename_from_url("https://example.com/files/my%20file.zip"),
            Some("my file.zip".into())
        );
        assert_eq!(filename_from_url("https://example.com/"), None);
    }

    #[test]
    fn test_percent_decode() {
        assert_eq!(percent_decode("hello%20world"), "hello world");
        assert_eq!(percent_decode("test%2Ffile"), "test/file");
        assert_eq!(percent_decode("normal"), "normal");
    }

    #[test]
    fn test_ensure_unique() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.zip");
        assert_eq!(ensure_unique(path.clone()), path);

        // Create the file
        std::fs::write(&path, "test").unwrap();
        let unique = ensure_unique(path.clone());
        assert_eq!(unique, dir.path().join("test_1.zip"));
    }

    #[test]
    fn test_is_html_file() {
        let dir = tempfile::tempdir().unwrap();

        let html_path = dir.path().join("error.html");
        std::fs::write(&html_path, "<html><body>Error 404</body></html>").unwrap();
        assert!(is_html_file(&html_path));

        let zip_path = dir.path().join("data.bin");
        std::fs::write(&zip_path, &[0u8; 100]).unwrap();
        assert!(!is_html_file(&zip_path));
    }
}
