//! Timing instrumentation for the extraction pipeline
//!
//! This module provides tools to measure extraction performance:
//! - `ExtractionTimingSession`: records durations of different steps
//! - Optional writing to a dedicated log file (developer mode)

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing::{info, warn};

// ============================================================================
// Types
// ============================================================================

/// A timing step with its name and duration
#[derive(Debug, Clone, Serialize)]
pub struct TimingStep {
    pub name: String,
    pub duration_ms: u64,
}

/// Final extraction statistics
#[derive(Debug, Clone, Serialize)]
pub struct ExtractionStats {
    pub files: usize,
    pub dirs: usize,
    pub size_bytes: u64,
}

/// Extraction result (success or error)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtractionResult {
    Success,
    Error(String),
}

/// Complete timing report for an extraction session
#[derive(Debug, Clone, Serialize)]
pub struct TimingReport {
    pub source: String,
    pub started_at: String,
    pub finished_at: String,
    pub total_duration_ms: u64,
    pub steps: Vec<TimingStep>,
    pub stats: Option<ExtractionStats>,
    pub result: ExtractionResult,
}

// ============================================================================
// ExtractionTimingSession
// ============================================================================

/// Timing session for a complete extraction operation
///
/// Records the start and end of each step, and generates a final report.
/// If developer mode is enabled (`dev_log_extraction_timings`), the report
/// is written to a dedicated log file.
pub struct ExtractionTimingSession {
    source: String,
    started_at: DateTime<Utc>,
    session_start: Instant,
    steps: Vec<TimingStep>,
    current_step: Option<(String, Instant)>,
    stats: Option<ExtractionStats>,
    /// Whether to log timing reports to file
    log_to_file: bool,
    /// Path to the timing log file (if logging is enabled)
    log_path: Option<PathBuf>,
}

impl ExtractionTimingSession {
    /// Creates a new timing session
    pub fn new(source: &str, dev_log_extraction_timings: bool, app_data_dir: &std::path::Path) -> Self {
        let now = Instant::now();
        info!("[TIMING] Starting extraction session for: {}", source);

        let log_path = if dev_log_extraction_timings {
            Some(app_data_dir.join("logs").join("extraction-timings.log"))
        } else {
            None
        };

        Self {
            source: source.to_string(),
            started_at: Utc::now(),
            session_start: now,
            steps: Vec::new(),
            current_step: None,
            stats: None,
            log_to_file: dev_log_extraction_timings,
            log_path,
        }
    }

    /// Starts a new timing step
    ///
    /// If a previous step was in progress, it is automatically ended.
    pub fn start_step(&mut self, name: &str) {
        // End previous step if it exists
        self.end_current_step();

        info!("[TIMING] Starting step: {}", name);
        self.current_step = Some((name.to_string(), Instant::now()));
    }

    /// Ends the current step (called automatically by start_step)
    pub fn end_step(&mut self) {
        self.end_current_step();
    }

    /// Ends the current step and records its duration
    fn end_current_step(&mut self) {
        if let Some((name, start)) = self.current_step.take() {
            let duration = start.elapsed();
            let duration_ms = duration.as_millis() as u64;

            info!("[TIMING] Step '{}' completed in {} ms", name, duration_ms);

            self.steps.push(TimingStep { name, duration_ms });
        }
    }

    /// Records a step with its duration directly (for functions that measure themselves)
    pub fn record_step(&mut self, name: &str, duration: Duration) {
        let duration_ms = duration.as_millis() as u64;
        info!("[TIMING] Step '{}' recorded: {} ms", name, duration_ms);

        self.steps.push(TimingStep {
            name: name.to_string(),
            duration_ms,
        });
    }

    /// Sets the final extraction statistics
    pub fn set_stats(&mut self, files: usize, dirs: usize, size_bytes: u64) {
        self.stats = Some(ExtractionStats {
            files,
            dirs,
            size_bytes,
        });
    }

    /// Finishes the session with success and generates the report
    pub fn finish_success(mut self) -> TimingReport {
        self.end_current_step();
        self.generate_report(ExtractionResult::Success)
    }

    /// Finishes the session with an error and generates the report
    pub fn finish_error(mut self, error: &str) -> TimingReport {
        self.end_current_step();
        self.generate_report(ExtractionResult::Error(error.to_string()))
    }

    /// Generates the final report and writes it to file if dev mode is enabled
    fn generate_report(self, result: ExtractionResult) -> TimingReport {
        let finished_at = Utc::now();
        let total_duration = self.session_start.elapsed();
        let total_duration_ms = total_duration.as_millis() as u64;

        let report = TimingReport {
            source: self.source.clone(),
            started_at: self.started_at.to_rfc3339(),
            finished_at: finished_at.to_rfc3339(),
            total_duration_ms,
            steps: self.steps,
            stats: self.stats,
            result,
        };

        info!(
            "[TIMING] Extraction session completed in {} ms ({} steps)",
            total_duration_ms,
            report.steps.len()
        );

        // Log steps with their durations
        for step in &report.steps {
            info!("[TIMING]   - {}: {} ms", step.name, step.duration_ms);
        }

        // Write to log file if dev mode is enabled
        if self.log_to_file {
            if let Some(ref log_path) = self.log_path {
                if let Err(e) = write_report_to_file(&report, log_path) {
                    warn!("[TIMING] Failed to write timing report to file: {}", e);
                }
            }
        }

        report
    }
}

// ============================================================================
// Utility functions
// ============================================================================

/// Writes the report to the log file
fn write_report_to_file(report: &TimingReport, log_path: &std::path::Path) -> std::io::Result<()> {

    // Create logs folder if necessary
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Open in append mode
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    // Write the report as JSON (one line per report)
    let json = serde_json::to_string(report)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    writeln!(file, "{}", json)?;

    info!("[TIMING] Report written to {:?}", log_path);
    Ok(())
}

/// Helper to measure the time of a closure and return the duration
pub fn measure<T, F: FnOnce() -> T>(f: F) -> (T, Duration) {
    let start = Instant::now();
    let result = f();
    let duration = start.elapsed();
    (result, duration)
}

/// Helper to measure and log the time of an operation
pub fn timed<T, F: FnOnce() -> T>(name: &str, f: F) -> T {
    let start = Instant::now();
    let result = f();
    let duration = start.elapsed();
    info!(
        "[TIMING] {} completed in {:?} ({} ms)",
        name,
        duration,
        duration.as_millis()
    );
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_timing_session() {
        let temp = std::env::temp_dir().join("fmd_timing_test");
        let mut session = ExtractionTimingSession::new("test_archive.zip", false, &temp);

        session.start_step("extract");
        thread::sleep(Duration::from_millis(10));
        session.end_step();

        session.start_step("analyze");
        thread::sleep(Duration::from_millis(5));
        session.end_step();

        session.set_stats(100, 10, 1024);

        let report = session.finish_success();

        assert_eq!(report.steps.len(), 2);
        assert!(report.total_duration_ms >= 15);
        assert!(report.stats.is_some());
    }
}
