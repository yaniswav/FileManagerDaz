//! # Import Tasks Persistence
//!
//! SQLite-based persistence for import tasks, enabling:
//! - Resume interrupted imports after app restart
//! - Retry failed imports with one click
//! - View import history
//!
//! ## Schema
//!
//! The `import_tasks` table stores:
//! - Task metadata (id, name, source path)
//! - Status (pending, processing, done, error, interrupted)
//! - Result data (destination, file count, content type)
//! - Timestamps (started_at, completed_at)
//!
//! ## Interrupted Task Recovery
//!
//! On application startup, [`ImportTasksRepository::mark_interrupted`] is called
//! to transition any "processing" tasks to "interrupted" status, making them
//! available for retry.

use crate::error::{AppError, AppResult};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tracing::{debug, info, warn};

// =============================================================================
// STATUS ENUM
// =============================================================================

/// Import task lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportTaskStatus {
    /// Task created, waiting to start.
    Pending,
    /// Currently extracting/processing.
    Processing,
    /// Successfully completed.
    Done,
    /// Failed with error (retryable).
    Error,
    /// Was processing when app closed (retryable).
    Interrupted,
}

impl ImportTaskStatus {
    /// Converts status to database string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            ImportTaskStatus::Pending => "pending",
            ImportTaskStatus::Processing => "processing",
            ImportTaskStatus::Done => "done",
            ImportTaskStatus::Error => "error",
            ImportTaskStatus::Interrupted => "interrupted",
        }
    }

    /// Parses status from database string.
    pub fn from_str(s: &str) -> Self {
        match s {
            "pending" => ImportTaskStatus::Pending,
            "processing" => ImportTaskStatus::Processing,
            "done" => ImportTaskStatus::Done,
            "error" => ImportTaskStatus::Error,
            "interrupted" => ImportTaskStatus::Interrupted,
            other => {
                warn!("Unknown ImportTaskStatus '{}', defaulting to Pending", other);
                ImportTaskStatus::Pending
            }
        }
    }
}

// =============================================================================
// PERSISTED TASK MODEL
// =============================================================================

/// A persisted import task record.
///
/// This struct maps directly to the `import_tasks` table schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedImportTask {
    /// Unique task identifier (UUID).
    pub id: String,
    /// Path to source archive or directory.
    pub source_path: String,
    /// Human-readable display name.
    pub name: String,
    /// Current task status.
    pub status: ImportTaskStatus,
    /// Final destination path (set on completion).
    pub destination: Option<String>,
    /// Error message (set on failure).
    pub error_message: Option<String>,
    /// Number of files extracted.
    pub files_count: Option<i64>,
    /// Total size in bytes.
    pub total_size: Option<i64>,
    /// Detected DAZ content type.
    pub content_type: Option<String>,
    /// Task start timestamp (Unix millis).
    pub started_at: i64,
    /// Task completion timestamp (Unix millis).
    pub completed_at: Option<i64>,
    /// Target DAZ library path.
    pub target_library: Option<String>,
}

// =============================================================================
// REPOSITORY
// =============================================================================

/// Repository for import task database operations.
///
/// Thread-safe wrapper around SQLite connection.
pub struct ImportTasksRepository {
    connection: Mutex<Connection>,
}

impl ImportTasksRepository {
    /// Creates a new repository with the given SQLite connection.
    pub fn new(conn: Connection) -> Self {
        Self {
            connection: Mutex::new(conn),
        }
    }

    /// Executes a function with access to the database connection.
    fn with_connection<T, F>(&self, operation: F) -> AppResult<T>
    where
        F: FnOnce(&Connection) -> AppResult<T>,
    {
        let conn = self
            .connection
            .lock()
            .map_err(|_| AppError::Database("Database lock poisoned".into()))?;
        operation(&conn)
    }

    /// Creates the import_tasks table if it doesn't exist.
    pub fn initialize(&self) -> AppResult<()> {
        self.with_connection(|conn| {
            conn.execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS import_tasks (
                id TEXT PRIMARY KEY,
                source_path TEXT NOT NULL,
                name TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                destination TEXT,
                error_message TEXT,
                files_count INTEGER,
                total_size INTEGER,
                content_type TEXT,
                started_at INTEGER NOT NULL,
                completed_at INTEGER,
                target_library TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_import_tasks_status ON import_tasks(status);
            CREATE INDEX IF NOT EXISTS idx_import_tasks_started ON import_tasks(started_at DESC);
            "#,
            )?;

            debug!("Import tasks table initialized");
            Ok(())
        })
    }

    /// Marks all "processing" tasks as "interrupted"
    /// Should be called at application startup
    pub fn mark_interrupted(&self) -> AppResult<usize> {
        self.with_connection(|conn| {

        let count = conn.execute(
            "UPDATE import_tasks SET status = 'interrupted' WHERE status = 'processing' OR status = 'pending'",
            [],
        )?;

            if count > 0 {
                warn!("{} task(s) marked as interrupted from previous session", count);
            }

            Ok(count)
        })
    }

    /// Adds a new task
    pub fn add_task(&self, task: &PersistedImportTask) -> AppResult<()> {
        self.with_connection(|conn| {
            conn.execute(
                r#"
            INSERT INTO import_tasks (
                id, source_path, name, status, destination, error_message,
                files_count, total_size, content_type, started_at, completed_at, target_library
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
                params![
                    task.id,
                    task.source_path,
                    task.name,
                    task.status.as_str(),
                    task.destination,
                    task.error_message,
                    task.files_count,
                    task.total_size,
                    task.content_type,
                    task.started_at,
                    task.completed_at,
                    task.target_library,
                ],
            )?;

            info!("Added import task: {} ({})", task.name, task.id);
            Ok(())
        })
    }

    /// Updates the status of a task
    pub fn update_status(&self, id: &str, status: ImportTaskStatus) -> AppResult<bool> {
        self.with_connection(|conn| {
            let completed_at =
                if status == ImportTaskStatus::Done || status == ImportTaskStatus::Error {
                    Some(chrono::Utc::now().timestamp_millis())
                } else {
                    None
                };

            let rows = if let Some(ts) = completed_at {
                conn.execute(
                    "UPDATE import_tasks SET status = ?1, completed_at = ?2 WHERE id = ?3",
                    params![status.as_str(), ts, id],
                )?
            } else {
                conn.execute(
                    "UPDATE import_tasks SET status = ?1 WHERE id = ?2",
                    params![status.as_str(), id],
                )?
            };

            Ok(rows > 0)
        })
    }

    /// Updates a task with the result (success)
    pub fn set_result(
        &self,
        id: &str,
        destination: &str,
        files_count: i64,
        total_size: i64,
        content_type: Option<&str>,
    ) -> AppResult<bool> {
        self.with_connection(|conn| {
            let now = chrono::Utc::now().timestamp_millis();

            let rows = conn.execute(
                r#"
            UPDATE import_tasks 
            SET status = 'done', destination = ?1, files_count = ?2, total_size = ?3,
                content_type = ?4, completed_at = ?5
            WHERE id = ?6
            "#,
                params![destination, files_count, total_size, content_type, now, id],
            )?;

            info!(
                "Task {} completed: {} files, {} bytes",
                id, files_count, total_size
            );
            Ok(rows > 0)
        })
    }

    /// Updates a task with an error
    pub fn set_error(&self, id: &str, error: &str) -> AppResult<bool> {
        self.with_connection(|conn| {
            let now = chrono::Utc::now().timestamp_millis();

        let rows = conn.execute(
            "UPDATE import_tasks SET status = 'error', error_message = ?1, completed_at = ?2 WHERE id = ?3",
            params![error, now, id],
        )?;

            warn!("Task {} failed: {}", id, error);
            Ok(rows > 0)
        })
    }

    /// Gets a task by ID
    pub fn get_task(&self, id: &str) -> AppResult<Option<PersistedImportTask>> {
        self.with_connection(|conn| {
            let mut stmt = conn.prepare(
                r#"
            SELECT id, source_path, name, status, destination, error_message,
                   files_count, total_size, content_type, started_at, completed_at, target_library
            FROM import_tasks WHERE id = ?1
            "#,
            )?;

            let task = stmt.query_row([id], Self::map_row).optional()?;

            Ok(task)
        })
    }

    /// Lists all tasks (most recent first)
    pub fn list_tasks(&self) -> AppResult<Vec<PersistedImportTask>> {
        self.with_connection(|conn| {
            let mut stmt = conn.prepare(
                r#"
            SELECT id, source_path, name, status, destination, error_message,
                   files_count, total_size, content_type, started_at, completed_at, target_library
            FROM import_tasks
            ORDER BY started_at DESC
            "#,
            )?;

            let tasks = stmt
                .query_map([], Self::map_row)?
                .collect::<Result<Vec<_>, _>>()?;

            debug!("Listed {} import tasks", tasks.len());
            Ok(tasks)
        })
    }

    /// Lists recent tasks (with limit)
    pub fn list_recent_tasks(&self, limit: usize) -> AppResult<Vec<PersistedImportTask>> {
        self.with_connection(|conn| {
            let mut stmt = conn.prepare(
                r#"
            SELECT id, source_path, name, status, destination, error_message,
                   files_count, total_size, content_type, started_at, completed_at, target_library
            FROM import_tasks
            ORDER BY started_at DESC
            LIMIT ?1
            "#,
            )?;

            let tasks = stmt
                .query_map([limit], Self::map_row)?
                .collect::<Result<Vec<_>, _>>()?;

            Ok(tasks)
        })
    }

    /// Lists tasks created within the last `days` days
    pub fn list_tasks_since_days(&self, days: i64) -> AppResult<Vec<PersistedImportTask>> {
        let cutoff = chrono::Utc::now()
            .timestamp_millis()
            .saturating_sub(days.saturating_mul(24 * 60 * 60 * 1000));

        self.with_connection(|conn| {
            let mut stmt = conn.prepare(
                r#"
            SELECT id, source_path, name, status, destination, error_message,
                   files_count, total_size, content_type, started_at, completed_at, target_library
            FROM import_tasks
            WHERE started_at >= ?1
            ORDER BY started_at DESC
            "#,
            )?;

            let tasks = stmt
                .query_map([cutoff], Self::map_row)?
                .collect::<Result<Vec<_>, _>>()?;

            Ok(tasks)
        })
    }

    /// Lists tasks by status
    pub fn list_tasks_by_status(
        &self,
        status: ImportTaskStatus,
    ) -> AppResult<Vec<PersistedImportTask>> {
        self.with_connection(|conn| {
            let mut stmt = conn.prepare(
                r#"
            SELECT id, source_path, name, status, destination, error_message,
                   files_count, total_size, content_type, started_at, completed_at, target_library
            FROM import_tasks
            WHERE status = ?1
            ORDER BY started_at DESC
            "#,
            )?;

            let tasks = stmt
                .query_map([status.as_str()], Self::map_row)?
                .collect::<Result<Vec<_>, _>>()?;

            Ok(tasks)
        })
    }

    /// Deletes a task
    pub fn delete_task(&self, id: &str) -> AppResult<bool> {
        self.with_connection(|conn| {
            let rows = conn.execute("DELETE FROM import_tasks WHERE id = ?1", [id])?;
            Ok(rows > 0)
        })
    }

    /// Deletes completed tasks older than a certain number of days
    pub fn cleanup_old_tasks(&self, days: i64) -> AppResult<usize> {
        self.with_connection(|conn| {
            let cutoff = chrono::Utc::now().timestamp_millis() - (days * 24 * 60 * 60 * 1000);

            let rows = conn.execute(
                "DELETE FROM import_tasks WHERE status IN ('done', 'error') AND completed_at < ?1",
                [cutoff],
            )?;

            if rows > 0 {
                info!("Cleaned up {} old import tasks", rows);
            }

            Ok(rows)
        })
    }

    /// Prepares a task for retry (resets its status)
    pub fn prepare_retry(&self, id: &str) -> AppResult<bool> {
        self.with_connection(|conn| {
            let now = chrono::Utc::now().timestamp_millis();

            let rows = conn.execute(
                r#"
            UPDATE import_tasks
            SET status = 'pending', error_message = NULL, destination = NULL,
                files_count = NULL, total_size = NULL, content_type = NULL,
                started_at = ?1, completed_at = NULL
            WHERE id = ?2 AND status IN ('error', 'interrupted')
            "#,
                params![now, id],
            )?;

            if rows > 0 {
                info!("Task {} prepared for retry", id);
            }

            Ok(rows > 0)
        })
    }

    /// Deletes all completed tasks (done or error status)
    pub fn delete_completed_tasks(&self) -> AppResult<usize> {
        self.with_connection(|conn| {
            let rows = conn.execute(
                "DELETE FROM import_tasks WHERE status IN ('done', 'error')",
                [],
            )?;

            if rows > 0 {
                info!("Deleted {} completed import tasks", rows);
            }

            Ok(rows)
        })
    }

    fn map_row(row: &rusqlite::Row) -> Result<PersistedImportTask, rusqlite::Error> {
        Ok(PersistedImportTask {
            id: row.get(0)?,
            source_path: row.get(1)?,
            name: row.get(2)?,
            status: ImportTaskStatus::from_str(&row.get::<_, String>(3)?),
            destination: row.get(4)?,
            error_message: row.get(5)?,
            files_count: row.get(6)?,
            total_size: row.get(7)?,
            content_type: row.get(8)?,
            started_at: row.get(9)?,
            completed_at: row.get(10)?,
            target_library: row.get(11)?,
        })
    }
}

// Trait for rusqlite::OptionalExtension
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> ImportTasksRepository {
        let conn = Connection::open_in_memory().unwrap();
        let repo = ImportTasksRepository::new(conn);
        repo.initialize().unwrap();
        repo
    }

    #[test]
    fn test_add_and_get_task() {
        let repo = setup_test_db();

        let task = PersistedImportTask {
            id: "test-123".to_string(),
            source_path: "C:\\test\\archive.zip".to_string(),
            name: "archive.zip".to_string(),
            status: ImportTaskStatus::Pending,
            destination: None,
            error_message: None,
            files_count: None,
            total_size: None,
            content_type: None,
            started_at: 1234567890,
            completed_at: None,
            target_library: None,
        };

        repo.add_task(&task).unwrap();

        let retrieved = repo.get_task("test-123").unwrap().unwrap();
        assert_eq!(retrieved.name, "archive.zip");
        assert_eq!(retrieved.status, ImportTaskStatus::Pending);
    }

    #[test]
    fn test_set_result() {
        let repo = setup_test_db();

        let task = PersistedImportTask {
            id: "test-456".to_string(),
            source_path: "C:\\test\\archive.rar".to_string(),
            name: "archive.rar".to_string(),
            status: ImportTaskStatus::Processing,
            destination: None,
            error_message: None,
            files_count: None,
            total_size: None,
            content_type: None,
            started_at: 1234567890,
            completed_at: None,
            target_library: None,
        };

        repo.add_task(&task).unwrap();
        repo.set_result(
            "test-456",
            "C:\\Library\\Content",
            150,
            1024000,
            Some("Character"),
        )
        .unwrap();

        let updated = repo.get_task("test-456").unwrap().unwrap();
        assert_eq!(updated.status, ImportTaskStatus::Done);
        assert_eq!(
            updated.destination,
            Some("C:\\Library\\Content".to_string())
        );
        assert_eq!(updated.files_count, Some(150));
    }

    #[test]
    fn test_mark_interrupted() {
        let repo = setup_test_db();

        // Add processing tasks
        for i in 0..3 {
            let task = PersistedImportTask {
                id: format!("task-{}", i),
                source_path: format!("C:\\test\\file{}.zip", i),
                name: format!("file{}.zip", i),
                status: ImportTaskStatus::Processing,
                destination: None,
                error_message: None,
                files_count: None,
                total_size: None,
                content_type: None,
                started_at: 1234567890,
                completed_at: None,
                target_library: None,
            };
            repo.add_task(&task).unwrap();
        }

        let count = repo.mark_interrupted().unwrap();
        assert_eq!(count, 3);

        let tasks = repo
            .list_tasks_by_status(ImportTaskStatus::Interrupted)
            .unwrap();
        assert_eq!(tasks.len(), 3);
    }

    #[test]
    fn test_prepare_retry() {
        let repo = setup_test_db();

        let task = PersistedImportTask {
            id: "retry-test".to_string(),
            source_path: "C:\\test\\fail.zip".to_string(),
            name: "fail.zip".to_string(),
            status: ImportTaskStatus::Error,
            destination: None,
            error_message: Some("Test error".to_string()),
            files_count: None,
            total_size: None,
            content_type: None,
            started_at: 1234567890,
            completed_at: Some(1234567900),
            target_library: None,
        };

        repo.add_task(&task).unwrap();
        repo.prepare_retry("retry-test").unwrap();

        let updated = repo.get_task("retry-test").unwrap().unwrap();
        assert_eq!(updated.status, ImportTaskStatus::Pending);
        assert!(updated.error_message.is_none());
        assert!(updated.completed_at.is_none());
    }
}
