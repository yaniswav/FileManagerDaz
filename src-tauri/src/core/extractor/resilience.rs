//! Resilience and error recovery for extraction operations
//!
//! Provides retry logic, timeout handling, and graceful degradation
//! for batch operations on large datasets.

use crate::error::{AppError, AppResult};
use std::path::Path;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

/// Configuration for retry and timeout behavior
#[derive(Debug, Clone)]
pub struct ResilienceConfig {
    /// Maximum number of retry attempts for failed operations
    pub max_retries: u32,
    /// Base delay between retries (exponential backoff)
    pub base_retry_delay: Duration,
    /// Maximum timeout per extraction operation
    pub extraction_timeout: Option<Duration>,
    /// Maximum size for a single archive (bytes, None = unlimited)
    pub max_archive_size: Option<u64>,
    /// Skip corrupted archives instead of failing the entire batch
    pub skip_corrupted: bool,
}

impl Default for ResilienceConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_retry_delay: Duration::from_secs(2),
            extraction_timeout: Some(Duration::from_secs(3600)), // 1 hour per archive
            max_archive_size: Some(10 * 1024 * 1024 * 1024),     // 10 GB
            skip_corrupted: true,
        }
    }
}

/// Retry strategy with exponential backoff
pub struct RetryStrategy {
    config: ResilienceConfig,
    attempt: u32,
}

impl RetryStrategy {
    pub fn new(config: ResilienceConfig) -> Self {
        Self { config, attempt: 0 }
    }

    /// Check if error is a file lock/in-use error
    fn is_file_locked_error(error: &AppError) -> bool {
        match error {
            AppError::Io(e) => {
                // Windows error codes:
                // ERROR_SHARING_VIOLATION = 32
                // ERROR_LOCK_VIOLATION = 33
                e.raw_os_error() == Some(32) || e.raw_os_error() == Some(33)
            }
            AppError::ZipError(msg) | AppError::RarError(msg) | AppError::SevenZipError(msg) => {
                msg.contains("being used by another process")
                    || msg.contains("sharing violation")
                    || msg.contains("locked")
                    || msg.contains("in use")
            }
            _ => false,
        }
    }

    /// Executes an operation with retry logic
    pub fn execute<F, T>(&mut self, mut operation: F) -> AppResult<T>
    where
        F: FnMut() -> AppResult<T>,
    {
        loop {
            self.attempt += 1;

            match operation() {
                Ok(result) => {
                    if self.attempt > 1 {
                        debug!("Operation succeeded after {} attempts", self.attempt);
                    }
                    return Ok(result);
                }
                Err(err) => {
                    if self.attempt >= self.config.max_retries {
                        warn!("Operation failed after {} attempts: {}", self.attempt, err);
                        return Err(err);
                    }

                    // Calculate exponential backoff delay
                    // If file is locked, use longer delay to wait for antivirus/Windows Defender
                    let mut delay = self.config.base_retry_delay * 2_u32.pow(self.attempt - 1);
                    if Self::is_file_locked_error(&err) {
                        warn!(
                            "File locked/in-use (attempt {}/{}): {}. Waiting longer for release...",
                            self.attempt, self.config.max_retries, err
                        );
                        // Add extra delay for file locks (antivirus scan, etc.)
                        delay = delay + Duration::from_secs(3);
                    } else {
                        warn!(
                            "Operation failed (attempt {}/{}): {}. Retrying in {:?}",
                            self.attempt, self.config.max_retries, err, delay
                        );
                    }

                    std::thread::sleep(delay);
                }
            }
        }
    }
}

/// Validates archive before processing
pub fn validate_archive(path: &Path, config: &ResilienceConfig) -> AppResult<()> {
    // Check file exists
    if !path.exists() {
        return Err(AppError::NotFound(path.to_path_buf()));
    }

    // Check file size if limit is configured
    if let Some(max_size) = config.max_archive_size {
        if let Ok(metadata) = std::fs::metadata(path) {
            let size = metadata.len();
            if size > max_size {
                return Err(AppError::UnsupportedFormat(format!(
                    "Archive too large: {} bytes (max: {} bytes)",
                    size, max_size
                )));
            }
        }
    }

    // Check read permissions
    if let Err(e) = std::fs::File::open(path) {
        return Err(AppError::Io(e));
    }

    Ok(())
}

/// Wrapper for timeout-aware operations
pub struct TimeoutGuard {
    start: Instant,
    timeout: Option<Duration>,
    operation_name: String,
}

impl TimeoutGuard {
    pub fn new(operation_name: impl Into<String>, timeout: Option<Duration>) -> Self {
        Self {
            start: Instant::now(),
            timeout,
            operation_name: operation_name.into(),
        }
    }

    /// Check if operation has exceeded timeout
    pub fn check_timeout(&self) -> AppResult<()> {
        if let Some(timeout) = self.timeout {
            let elapsed = self.start.elapsed();
            if elapsed > timeout {
                return Err(AppError::Internal(format!(
                    "Operation '{}' timed out after {:?} (limit: {:?})",
                    self.operation_name, elapsed, timeout
                )));
            }
        }
        Ok(())
    }

    /// Get elapsed time
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

/// Batch processor with error isolation
pub struct BatchProcessor<T> {
    items: Vec<T>,
    config: ResilienceConfig,
}

impl<T> BatchProcessor<T> {
    pub fn new(items: Vec<T>, config: ResilienceConfig) -> Self {
        Self { items, config }
    }

    /// Process all items, collecting results and errors separately
    pub fn process_all<F, R>(self, mut process_fn: F) -> (Vec<(T, R)>, Vec<(T, AppError)>)
    where
        F: FnMut(&T) -> AppResult<R>,
    {
        let mut successes = Vec::new();
        let mut failures = Vec::new();

        for item in self.items {
            let mut retry = RetryStrategy::new(self.config.clone());

            match retry.execute(|| process_fn(&item)) {
                Ok(result) => successes.push((item, result)),
                Err(error) => {
                    if self.config.skip_corrupted {
                        warn!("Skipping failed item: {}", error);
                        failures.push((item, error));
                    } else {
                        // If not skipping, we could break here
                        failures.push((item, error));
                    }
                }
            }
        }

        (successes, failures)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_success_on_second_attempt() {
        let mut attempt = 0;
        let config = ResilienceConfig {
            max_retries: 3,
            base_retry_delay: Duration::from_millis(10),
            ..Default::default()
        };

        let mut retry = RetryStrategy::new(config);
        let result = retry.execute(|| {
            attempt += 1;
            if attempt < 2 {
                Err(AppError::Internal("Temporary error".into()))
            } else {
                Ok("Success")
            }
        });

        assert!(result.is_ok());
        assert_eq!(attempt, 2);
    }

    #[test]
    fn test_retry_exhausted() {
        let config = ResilienceConfig {
            max_retries: 2,
            base_retry_delay: Duration::from_millis(10),
            ..Default::default()
        };

        let mut retry = RetryStrategy::new(config);
        let result: AppResult<()> =
            retry.execute(|| Err(AppError::Internal("Persistent error".into())));

        assert!(result.is_err());
    }

    #[test]
    fn test_timeout_guard() {
        let guard = TimeoutGuard::new("test", Some(Duration::from_millis(50)));

        assert!(guard.check_timeout().is_ok());

        std::thread::sleep(Duration::from_millis(60));
        assert!(guard.check_timeout().is_err());
    }
}
