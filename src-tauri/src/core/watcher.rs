//! Folder watching module for new archives
//!
//! Watches a folder (e.g.: Downloads) to automatically detect
//! new DAZ archives to import.

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{debug, error, info};

// ============================================================================
// Types
// ============================================================================

/// Detected file event
#[derive(Debug, Clone)]
pub struct WatchEvent {
    pub path: PathBuf,
    pub event_type: WatchEventType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WatchEventType {
    /// New file created
    Created,
    /// File modified (finished writing)
    Modified,
    /// File removed
    Removed,
}

/// Supported archive extensions
const ARCHIVE_EXTENSIONS: &[&str] = &["zip", "rar", "7z"];

// ============================================================================
// FolderWatcher
// ============================================================================

/// Folder watcher
pub struct FolderWatcher {
    /// Watched folder
    watch_path: PathBuf,
    /// Active watcher
    watcher: Option<RecommendedWatcher>,
    /// Channel for receiving events
    event_rx: Option<Receiver<WatchEvent>>,
    /// Files being processed (to avoid duplicates)
    processing: Arc<Mutex<HashSet<PathBuf>>>,
    /// Stop flag
    running: Arc<Mutex<bool>>,
}

impl FolderWatcher {
    /// Creates a new watcher (not yet started)
    pub fn new(watch_path: &Path) -> Self {
        Self {
            watch_path: watch_path.to_path_buf(),
            watcher: None,
            event_rx: None,
            processing: Arc::new(Mutex::new(HashSet::new())),
            running: Arc::new(Mutex::new(false)),
        }
    }

    /// Starts watching
    pub fn start(&mut self) -> Result<(), String> {
        if self.is_running() {
            return Err("Watcher already running".to_string());
        }

        let (tx, rx) = channel::<WatchEvent>();
        let processing = Arc::clone(&self.processing);

        // Configure the watcher
        let watcher_tx = tx.clone();
        let watcher = notify::recommended_watcher(move |res: notify::Result<Event>| match res {
            Ok(event) => {
                if let Some(watch_event) = process_notify_event(event, &processing) {
                    if let Err(e) = watcher_tx.send(watch_event) {
                        error!("Failed to send watch event: {}", e);
                    }
                }
            }
            Err(e) => error!("Watch error: {:?}", e),
        })
        .map_err(|e| format!("Failed to create watcher: {}", e))?;

        self.watcher = Some(watcher);
        self.event_rx = Some(rx);

        // Start watching
        if let Some(ref mut w) = self.watcher {
            w.watch(&self.watch_path, RecursiveMode::NonRecursive)
                .map_err(|e| format!("Failed to watch path: {}", e))?;
        }

        *self.running.lock().unwrap() = true;
        info!("Started watching: {:?}", self.watch_path);

        Ok(())
    }

    /// Stops the watcher
    pub fn stop(&mut self) {
        *self.running.lock().unwrap() = false;

        if let Some(ref mut watcher) = self.watcher {
            let _ = watcher.unwatch(&self.watch_path);
        }

        self.watcher = None;
        self.event_rx = None;

        info!("Stopped watching: {:?}", self.watch_path);
    }

    /// Checks if the watcher is active
    pub fn is_running(&self) -> bool {
        *self.running.lock().unwrap()
    }

    /// Gets the next event (non-blocking)
    pub fn try_recv(&self) -> Option<WatchEvent> {
        self.event_rx.as_ref()?.try_recv().ok()
    }

    /// Gets the next event (blocking with timeout)
    #[allow(dead_code)]
    pub fn recv_timeout(&self, timeout: Duration) -> Option<WatchEvent> {
        self.event_rx.as_ref()?.recv_timeout(timeout).ok()
    }

    /// Marks a file as processed
    #[allow(dead_code)]
    pub fn mark_processed(&self, path: &Path) {
        self.processing.lock().unwrap().remove(path);
    }

    /// Scans the folder for existing archives
    pub fn scan_existing(&self) -> Vec<PathBuf> {
        let mut archives = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&self.watch_path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if is_archive(&path) && path.is_file() {
                    archives.push(path);
                }
            }
        }

        archives
    }
}

impl Drop for FolderWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Processes a notify event and returns a WatchEvent if relevant
fn process_notify_event(
    event: Event,
    processing: &Arc<Mutex<HashSet<PathBuf>>>,
) -> Option<WatchEvent> {
    // We only care about created or modified files
    let event_type = match event.kind {
        EventKind::Create(_) => WatchEventType::Created,
        EventKind::Modify(_) => WatchEventType::Modified,
        EventKind::Remove(_) => WatchEventType::Removed,
        _ => return None,
    };

    // Filter to keep only archives
    for path in event.paths {
        if !is_archive(&path) {
            continue;
        }

        // For creations/modifications, verify that the file exists and is not empty
        if event_type != WatchEventType::Removed {
            if !path.exists() {
                continue;
            }

            // Wait a bit for the file to be completely written
            // (large files may take time)
            if let Ok(meta) = std::fs::metadata(&path) {
                if meta.len() == 0 {
                    continue;
                }
            }

            // Avoid duplicates
            {
                let mut proc = processing.lock().unwrap();
                if proc.contains(&path) {
                    continue;
                }
                proc.insert(path.clone());
            }
        }

        debug!("Watch event: {:?} - {:?}", event_type, path);

        return Some(WatchEvent { path, event_type });
    }

    None
}

/// Checks if a file is a supported archive
fn is_archive(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ARCHIVE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

// ============================================================================
// Global state for Tauri
// ============================================================================

/// Watcher state for Tauri
pub struct WatcherState {
    watcher: Mutex<Option<FolderWatcher>>,
    watch_path: Mutex<Option<PathBuf>>,
}

impl WatcherState {
    pub fn new() -> Self {
        Self {
            watcher: Mutex::new(None),
            watch_path: Mutex::new(None),
        }
    }

    /// Starts watching a folder
    pub fn start(&self, path: &Path) -> Result<(), String> {
        let mut watcher_guard = self.watcher.lock().unwrap();
        let mut path_guard = self.watch_path.lock().unwrap();

        // Stop old watcher if it exists
        if let Some(ref mut w) = *watcher_guard {
            w.stop();
        }

        // Create and start the new watcher
        let mut watcher = FolderWatcher::new(path);
        watcher.start()?;

        *watcher_guard = Some(watcher);
        *path_guard = Some(path.to_path_buf());

        Ok(())
    }

    /// Stops watching
    pub fn stop(&self) {
        let mut watcher_guard = self.watcher.lock().unwrap();
        let mut path_guard = self.watch_path.lock().unwrap();

        if let Some(ref mut w) = *watcher_guard {
            w.stop();
        }

        *watcher_guard = None;
        *path_guard = None;
    }

    /// Checks if watching is active
    pub fn is_watching(&self) -> bool {
        self.watcher
            .lock()
            .unwrap()
            .as_ref()
            .map(|w| w.is_running())
            .unwrap_or(false)
    }

    /// Returns the watched path
    pub fn get_watch_path(&self) -> Option<PathBuf> {
        self.watch_path.lock().unwrap().clone()
    }

    /// Gets pending events
    pub fn poll_events(&self) -> Vec<WatchEvent> {
        let mut events = Vec::new();

        if let Some(ref watcher) = *self.watcher.lock().unwrap() {
            while let Some(event) = watcher.try_recv() {
                events.push(event);
            }
        }

        events
    }

    /// Scans existing archives
    pub fn scan_existing(&self) -> Vec<PathBuf> {
        self.watcher
            .lock()
            .unwrap()
            .as_ref()
            .map(|w| w.scan_existing())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_archive() {
        assert!(is_archive(Path::new("test.zip")));
        assert!(is_archive(Path::new("test.ZIP")));
        assert!(is_archive(Path::new("test.rar")));
        assert!(is_archive(Path::new("test.7z")));
        assert!(!is_archive(Path::new("test.txt")));
        assert!(!is_archive(Path::new("test.duf")));
    }
}
