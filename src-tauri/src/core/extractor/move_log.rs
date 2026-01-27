//! Detailed move logging for extraction merges.
//!
//! Writes JSON lines describing file moves, skips, and summaries when enabled.

use crate::config::SETTINGS;
use chrono::Utc;
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use tracing::warn;

#[derive(Debug, Default, Clone, Serialize)]
pub struct MoveLogCounts {
    pub moved_files: u64,
    pub missing_files: u64,
    pub skipped_files: u64,
    pub skipped_entries: u64,
    pub unmoved_files: u64,
    pub merged_dirs: u64,
    pub errors: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MoveLogEntry {
    pub ts: String,
    pub session_id: String,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dest_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_root: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_anchor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counts: Option<MoveLogCounts>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_loose_files: Option<bool>,
}

impl MoveLogEntry {
    pub fn new(event: &str) -> Self {
        Self {
            ts: String::new(),
            session_id: String::new(),
            event: event.to_string(),
            source_path: None,
            temp_dir: None,
            dest_path: None,
            anchor_root: None,
            anchors: Vec::new(),
            preferred_anchor: None,
            reason: None,
            status: None,
            message: None,
            counts: None,
            anchor_count: None,
            has_loose_files: None,
        }
    }
}

pub struct MoveLogger {
    writer: BufWriter<std::fs::File>,
    session_id: String,
}

impl MoveLogger {
    pub fn new(session_id: String) -> Option<Self> {
        if !should_log_to_file() {
            return None;
        }

        let log_path = get_move_log_path()?;

        if let Some(parent) = log_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                warn!("[MOVE_LOG] Failed to create logs dir: {}", e);
                return None;
            }
        }

        let file = match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            Ok(file) => file,
            Err(e) => {
                warn!("[MOVE_LOG] Failed to open log file: {}", e);
                return None;
            }
        };

        Some(Self {
            writer: BufWriter::new(file),
            session_id,
        })
    }

    pub fn log(&mut self, mut entry: MoveLogEntry) -> std::io::Result<()> {
        entry.ts = Utc::now().to_rfc3339();
        entry.session_id = self.session_id.clone();

        let json = serde_json::to_string(&entry)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        writeln!(self.writer, "{}", json)?;
        Ok(())
    }
}

fn should_log_to_file() -> bool {
    SETTINGS
        .read()
        .map(|s| s.dev_log_extraction_details)
        .unwrap_or(false)
}

fn get_move_log_path() -> Option<PathBuf> {
    SETTINGS
        .read()
        .ok()
        .map(|s| s.app_data_dir.join("logs").join("extraction-moves.log"))
}
