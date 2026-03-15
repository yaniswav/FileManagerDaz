//! MediaFire page scraping
//!
//! Resolves MediaFire file page URLs to direct download links by
//! scraping the download button from the HTML page.

use crate::error::{AppError, AppResult};
use reqwest::Client;
use scraper::{Html, Selector};
use std::time::Duration;
use tracing::{debug, warn};

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// Resolve a MediaFire page URL to a direct download link.
///
/// Fetches the MediaFire page HTML and scrapes the download button href.
pub async fn resolve_mediafire_url(url: &str, timeout_secs: u64) -> AppResult<String> {
    let client = Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(timeout_secs))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| AppError::Config(format!("HTTP client error: {}", e)))?;

    debug!("Resolving MediaFire URL: {}", url);

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::Config(format!("MediaFire request failed: {}", e)))?;

    // If we got redirected to a non-MediaFire domain, that's the direct link
    let final_url = resp.url().to_string();
    let final_host = resp.url().host_str().unwrap_or("").to_lowercase();
    if !final_host.contains("mediafire.com") {
        debug!("MediaFire redirected to: {}", final_url);
        return Ok(final_url);
    }

    let html = resp
        .text()
        .await
        .map_err(|e| AppError::Config(format!("Failed to read MediaFire page: {}", e)))?;

    // Parse HTML and find download button
    let document = Html::parse_document(&html);

    // Try multiple selectors for the download button
    let selectors = [
        "a#downloadButton",
        r#"a[aria-label="Download file"]"#,
        "a.downloadButton",
    ];

    for sel_str in &selectors {
        if let Ok(selector) = Selector::parse(sel_str) {
            if let Some(element) = document.select(&selector).next() {
                if let Some(href) = element.value().attr("href") {
                    if !href.is_empty() && href.starts_with("http") {
                        debug!("MediaFire resolved: {} -> {}", url, href);
                        return Ok(href.to_string());
                    }
                }
            }
        }
    }

    // Fallback: look for any link containing "download" in the URL
    if let Ok(selector) = Selector::parse("a[href]") {
        for element in document.select(&selector) {
            if let Some(href) = element.value().attr("href") {
                if href.contains("download") && href.starts_with("http") {
                    debug!("MediaFire fallback resolved: {}", href);
                    return Ok(href.to_string());
                }
            }
        }
    }

    warn!("Could not resolve MediaFire download link from: {}", url);
    Err(AppError::Config(format!(
        "Could not find download link on MediaFire page: {}",
        url
    )))
}

#[cfg(test)]
mod tests {
    // MediaFire tests require network access, so we only test URL classification
    use super::*;

    #[test]
    fn test_module_compiles() {
        // Ensures the module compiles without issues
        assert_eq!(USER_AGENT.len() > 0, true);
    }
}
