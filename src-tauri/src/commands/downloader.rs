//! Tauri commands for the bulk downloader

use crate::core::downloader::{
    parse_urls, run_batch_downloads, DownloadLink, DownloadOptions, DownloadProgressEvent,
    DownloadSummary,
};
use crate::error::ApiResponse;
use tauri::Emitter;
use tokio::sync::mpsc;
use tracing::info;

/// Parse URLs from pasted text and return the list of detected links.
#[tauri::command]
pub async fn parse_download_links(text: String) -> Result<ApiResponse<Vec<DownloadLink>>, String> {
    info!("parse_download_links: {} chars of text", text.len());
    let links = parse_urls(&text);
    info!("Parsed {} download links", links.len());
    Ok(ApiResponse::success(links))
}

/// Start downloading all provided links.
/// Emits `download-progress` events to the frontend for real-time tracking.
#[tauri::command]
pub async fn start_downloads(
    app: tauri::AppHandle,
    links: Vec<DownloadLink>,
    options: DownloadOptions,
) -> Result<ApiResponse<DownloadSummary>, String> {
    info!(
        "start_downloads: {} links, {} workers, dest={}",
        links.len(),
        options.workers,
        options.dest_dir
    );

    let (tx, mut rx) = mpsc::unbounded_channel::<DownloadProgressEvent>();

    // Forward progress events to the frontend
    let app_handle = app.clone();
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let _ = app_handle.emit("download-progress", &event);
        }
    });

    match run_batch_downloads(links, options, tx).await {
        Ok(summary) => {
            // Emit final summary event
            let _ = app.emit("download-complete", &summary);
            Ok(ApiResponse::success(summary))
        }
        Err(e) => {
            let _ = app.emit("download-error", e.to_string());
            Ok(ApiResponse::error_msg("DOWNLOAD_ERROR", &e.to_string()))
        }
    }
}
