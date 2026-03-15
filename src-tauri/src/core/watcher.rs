//! Folder watching module for new archives
//!
//! Watches a folder (e.g.: Downloads) to automatically detect
//! new DAZ archives to import.
//!
//! Uses a two-phase file-stability mechanism:
//! 1. The `notify` callback enqueues raw filesystem events into a `pending`
//!    map — **no disk I/O is performed under any lock**.
//! 2. A dedicated stability-checker thread polls file sizes **outside** any
//!    Mutex, and only emits a "ready" event once the size hasn't changed for
//!    [`STABILITY_DURATION`] consecutive seconds.
//!
//! This prevents extraction of partially-downloaded archives regardless of
//! whether the download comes from the built-in downloader, a browser, or
//! any other source.

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

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

/// How long a file's size must remain unchanged before it's considered stable
const STABILITY_DURATION: Duration = Duration::from_secs(3);

/// How often the stability checker thread polls file sizes
const STABILITY_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Tracks a file that appeared but may still be downloading
#[derive(Debug, Clone)]
struct PendingFile {
    last_size: u64,
    last_changed: Instant,
}

/// Result of an I/O size-check performed **outside** any Mutex lock
enum SizeCheckResult {
    /// File exists and has this size in bytes
    Exists(u64),
    /// File was not found on disk — should be removed from pending
    Gone,
    /// I/O error other than `NotFound` (e.g. `PermissionDenied`, file locked).
    /// Assume the file is still being written; do **not** reset the timer.
    Inaccessible,
}

// ============================================================================
// FolderWatcher
// ============================================================================

/// Folder watcher with file-stability support
pub struct FolderWatcher {
    /// Watched folder
    watch_path: PathBuf,
    /// Active `notify` watcher
    watcher: Option<RecommendedWatcher>,
    /// Receiving end of the "stable events" channel
    event_rx: Option<Receiver<WatchEvent>>,
    /// Paths already emitted as stable (deduplication set)
    processing: Arc<Mutex<HashSet<PathBuf>>>,
    /// Files waiting for their size to stabilise
    pending: Arc<Mutex<HashMap<PathBuf, PendingFile>>>,
    /// Stop flag shared with the stability thread
    running: Arc<Mutex<bool>>,
    /// Handle to the stability-checker thread for clean shutdown
    stability_thread: Option<JoinHandle<()>>,
}

impl FolderWatcher {
    /// Creates a new watcher (not yet started)
    pub fn new(watch_path: &Path) -> Self {
        Self {
            watch_path: watch_path.to_path_buf(),
            watcher: None,
            event_rx: None,
            processing: Arc::new(Mutex::new(HashSet::new())),
            pending: Arc::new(Mutex::new(HashMap::new())),
            running: Arc::new(Mutex::new(false)),
            stability_thread: None,
        }
    }

    /// Starts watching
    pub fn start(&mut self) -> Result<(), String> {
        if self.is_running() {
            return Err("Watcher already running".to_string());
        }

        // Clear stale state from a previous run (safe: thread was joined in stop())
        if let Ok(mut p) = self.pending.lock() {
            p.clear();
        }
        if let Ok(mut p) = self.processing.lock() {
            p.clear();
        }

        let (stable_tx, stable_rx) = channel::<WatchEvent>();
        let pending = Arc::clone(&self.pending);
        let processing = Arc::clone(&self.processing);
        let running = Arc::clone(&self.running);

        // Clones captured by the notify callback closure
        let cb_pending = Arc::clone(&self.pending);
        let cb_processing = Arc::clone(&self.processing);
        let cb_tx = stable_tx.clone();

        let watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            match res {
                Ok(event) => {
                    enqueue_raw_event(event, &cb_pending, &cb_processing, &cb_tx);
                }
                Err(e) => error!("Watch error: {:?}", e),
            }
        })
        .map_err(|e| format!("Failed to create watcher: {}", e))?;

        self.watcher = Some(watcher);
        self.event_rx = Some(stable_rx);

        // Start watching the folder
        if let Some(ref mut w) = self.watcher {
            w.watch(&self.watch_path, RecursiveMode::NonRecursive)
                .map_err(|e| format!("Failed to watch path: {}", e))?;
        }

        *self
            .running
            .lock()
            .map_err(|_| "Running mutex poisoned".to_string())? = true;
        info!("Started watching: {:?}", self.watch_path);

        // Spawn the stability checker thread and store its handle
        let handle = std::thread::Builder::new()
            .name("watcher-stability".into())
            .spawn(move || {
                stability_checker_loop(pending, processing, stable_tx, running);
            })
            .map_err(|e| format!("Failed to spawn stability thread: {}", e))?;

        self.stability_thread = Some(handle);

        Ok(())
    }

    /// Stops the watcher and joins the stability thread for clean shutdown
    pub fn stop(&mut self) {
        // 1. Signal the stability thread to exit
        if let Ok(mut running) = self.running.lock() {
            *running = false;
        } else {
            warn!("Could not lock running mutex during stop");
        }

        // 2. Stop the notify watcher (no more callbacks)
        if let Some(ref mut watcher) = self.watcher {
            let _ = watcher.unwatch(&self.watch_path);
        }
        self.watcher = None;
        self.event_rx = None;

        // 3. Join the stability thread (blocks at most ~STABILITY_POLL_INTERVAL)
        if let Some(handle) = self.stability_thread.take() {
            if let Err(e) = handle.join() {
                warn!("Stability checker thread panicked: {:?}", e);
            }
        }

        info!("Stopped watching: {:?}", self.watch_path);
    }

    /// Checks if the watcher is active
    pub fn is_running(&self) -> bool {
        self.running.lock().map(|g| *g).unwrap_or(false)
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

    /// Marks a file as processed (clears from the deduplication set)
    #[allow(dead_code)]
    pub fn mark_processed(&self, path: &Path) {
        if let Ok(mut proc) = self.processing.lock() {
            proc.remove(path);
        }
    }

    /// Scans the folder for existing archives
    pub fn scan_existing(&self) -> Vec<PathBuf> {
        let mut archives = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&self.watch_path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if is_archive(&path) && path.is_file() {
                    archives.push(normalize_path(&path));
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
// Notify callback — must be fast, no disk I/O under locks
// ============================================================================

/// Handles a raw `notify` event.
///
/// - **Create / Modify**: inserts the (normalised) path into `pending` with
///   `last_size = 0`.  The stability thread will poll the real size on disk.
///   **No disk I/O is performed here.**
/// - **Remove**: immediately removes the path from both `pending` and
///   `processing`, then emits a [`WatchEventType::Removed`] event through
///   `stable_tx` so the frontend can react.
fn enqueue_raw_event(
    event: Event,
    pending: &Arc<Mutex<HashMap<PathBuf, PendingFile>>>,
    processing: &Arc<Mutex<HashSet<PathBuf>>>,
    stable_tx: &Sender<WatchEvent>,
) {
    let is_create_or_modify = matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_)
    );
    let is_remove = matches!(event.kind, EventKind::Remove(_));

    if !is_create_or_modify && !is_remove {
        return;
    }

    for path in event.paths {
        if !is_archive(&path) {
            continue;
        }

        let normalized = normalize_path(&path);

        if is_remove {
            // Clean up from pending (may still have been waiting for stability)
            if let Ok(mut map) = pending.lock() {
                map.remove(&normalized);
            }
            // Clean up from processing so re-downloading the same file works
            if let Ok(mut proc) = processing.lock() {
                proc.remove(&normalized);
            }
            // Emit the Removed event immediately (no stability wait needed)
            let _ = stable_tx.send(WatchEvent {
                path: normalized,
                event_type: WatchEventType::Removed,
            });
            continue;
        }

        // Create or Modify — enqueue for stability checking.
        // Size is set to 0; the stability thread will poll the real size.
        if let Ok(mut map) = pending.lock() {
            map.entry(normalized).or_insert(PendingFile {
                last_size: 0,
                last_changed: Instant::now(),
            });
        }
    }
}

// ============================================================================
// Stability checker thread
// ============================================================================

/// Background loop that polls file sizes **outside** any Mutex lock and emits
/// "ready" events once a file's size has been stable for [`STABILITY_DURATION`].
///
/// The loop runs in four phases per iteration:
/// 1. **Snapshot** — briefly lock `pending`, clone keys + values, release lock.
/// 2. **I/O** — call `fs::metadata` for each path (no lock held).
/// 3. **Update** — re-acquire `pending` lock, apply size/timer updates, collect
///    ready files, remove them from the map.
/// 4. **Emit** — send ready files through the channel (with dedup via
///    `processing`).
fn stability_checker_loop(
    pending: Arc<Mutex<HashMap<PathBuf, PendingFile>>>,
    processing: Arc<Mutex<HashSet<PathBuf>>>,
    stable_tx: Sender<WatchEvent>,
    running: Arc<Mutex<bool>>,
) {
    debug!("Stability checker thread started");

    loop {
        // ── Check stop flag ──────────────────────────────────────────
        if let Ok(r) = running.lock() {
            if !*r {
                break;
            }
        }

        std::thread::sleep(STABILITY_POLL_INTERVAL);

        // ── Phase 1: snapshot pending paths (brief lock, zero I/O) ───
        let snapshot: Vec<PathBuf> = match pending.lock() {
            Ok(map) => map.keys().cloned().collect(),
            Err(_) => continue,
        }; // lock released here

        if snapshot.is_empty() {
            continue;
        }

        // ── Phase 2: check file sizes on disk (NO lock held) ─────────
        let checks: Vec<(PathBuf, SizeCheckResult)> = snapshot
            .into_iter()
            .map(|path| {
                let result = match std::fs::metadata(&path) {
                    Ok(meta) => SizeCheckResult::Exists(meta.len()),
                    Err(e) if e.kind() == ErrorKind::NotFound => SizeCheckResult::Gone,
                    Err(_) => SizeCheckResult::Inaccessible,
                };
                (path, result)
            })
            .collect();

        // ── Phase 3: re-acquire lock, apply results ──────────────────
        let mut ready_files: Vec<PathBuf> = Vec::new();
        {
            let mut map = match pending.lock() {
                Ok(m) => m,
                Err(_) => continue,
            };

            for (path, result) in &checks {
                match result {
                    SizeCheckResult::Gone => {
                        // File disappeared while waiting — just discard it
                        map.remove(path);
                    }
                    SizeCheckResult::Inaccessible => {
                        // File is locked / permission denied by another process.
                        // Assume still being written — keep the entry as-is,
                        // do NOT reset the stability timer.
                    }
                    SizeCheckResult::Exists(current_size) => {
                        if *current_size == 0 {
                            // File exists but is empty; keep waiting
                            continue;
                        }

                        if let Some(info) = map.get_mut(path) {
                            if *current_size != info.last_size {
                                // Size changed — still downloading, reset timer
                                info.last_size = *current_size;
                                info.last_changed = Instant::now();
                            } else if info.last_changed.elapsed() >= STABILITY_DURATION {
                                // Stable long enough — mark as ready
                                ready_files.push(path.clone());
                            }
                        }
                    }
                }
            }

            // Remove ready files from pending
            for path in &ready_files {
                map.remove(path);
            }
        } // lock released

        // ── Phase 4: emit stable files (dedup via processing) ────────
        for path in ready_files {
            let already_processed = match processing.lock() {
                Ok(mut proc) => {
                    if proc.contains(&path) {
                        true
                    } else {
                        proc.insert(path.clone());
                        false
                    }
                }
                Err(_) => false,
            };

            if already_processed {
                continue;
            }

            info!("Archive stable and ready: {:?}", path);
            if let Err(e) = stable_tx.send(WatchEvent {
                path,
                event_type: WatchEventType::Created,
            }) {
                error!("Failed to send stable event: {}", e);
                return; // Channel closed (receiver dropped), stop thread
            }
        }
    }

    debug!("Stability checker thread stopped");
}

// ============================================================================
// Path helpers
// ============================================================================

/// Normalises a path for consistent `HashMap` / `HashSet` keying.
///
/// Tries [`std::fs::canonicalize`] first (resolves symlinks, normalises
/// separators).  On Windows the `\\?\` extended-length prefix added by
/// `canonicalize` is stripped.  If canonicalisation fails (file does not exist,
/// e.g. on a `Remove` event), falls back to component-based normalisation
/// which resolves `.` / `..` and normalises separator characters.
fn normalize_path(path: &Path) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        #[cfg(windows)]
        {
            let s = canonical.to_string_lossy();
            if let Some(stripped) = s.strip_prefix(r"\\?\") {
                return PathBuf::from(stripped);
            }
        }
        return canonical;
    }

    // Fallback for non-existent files (e.g. Remove events).
    // Manually resolve `.` and `..` components.
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                result.pop();
            }
            std::path::Component::CurDir => {
                // skip
            }
            other => {
                result.push(other);
            }
        }
    }
    result
}

/// Checks if a file is a supported archive based on its extension
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
        let mut watcher_guard = self
            .watcher
            .lock()
            .map_err(|_| "Watcher mutex poisoned".to_string())?;
        let mut path_guard = self
            .watch_path
            .lock()
            .map_err(|_| "Watch path mutex poisoned".to_string())?;

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
        if let Ok(mut watcher_guard) = self.watcher.lock() {
            if let Some(ref mut w) = *watcher_guard {
                w.stop();
            }
            *watcher_guard = None;
        }

        if let Ok(mut path_guard) = self.watch_path.lock() {
            *path_guard = None;
        }
    }

    /// Checks if watching is active
    pub fn is_watching(&self) -> bool {
        self.watcher
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|w| w.is_running()))
            .unwrap_or(false)
    }

    /// Returns the watched path
    pub fn get_watch_path(&self) -> Option<PathBuf> {
        self.watch_path.lock().ok()?.clone()
    }

    /// Gets pending events
    pub fn poll_events(&self) -> Vec<WatchEvent> {
        let mut events = Vec::new();

        if let Ok(guard) = self.watcher.lock() {
            if let Some(ref watcher) = *guard {
                while let Some(event) = watcher.try_recv() {
                    events.push(event);
                }
            }
        }

        events
    }

    /// Scans existing archives
    pub fn scan_existing(&self) -> Vec<PathBuf> {
        self.watcher
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|w| w.scan_existing()))
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

    #[test]
    fn test_normalize_path_nonexistent() {
        // For a path that doesn't exist, normalize_path should still return
        // a usable PathBuf via the component fallback
        let p = normalize_path(Path::new("some/nonexistent/../file.zip"));
        assert!(p.to_string_lossy().contains("file.zip"));
        assert!(!p.to_string_lossy().contains(".."));
    }

    #[test]
    fn test_pending_file_clone() {
        let pf = PendingFile {
            last_size: 42,
            last_changed: Instant::now(),
        };
        let pf2 = pf.clone();
        assert_eq!(pf2.last_size, 42);
    }
}
