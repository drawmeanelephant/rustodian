//! `SQLite` implementation of [`ProjectStore`].

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, params};
use tracing::debug;

use rustodian_core::CoreError;
use rustodian_core::traits::ProjectStore;
use rustodian_types::{Project, ProjectId, ProjectMetadata, ScanId, ScanRecord, ScanStatus};

use crate::error::StorageError;
use crate::migrations;

/// `SQLite`-backed project store.
///
/// Wraps `Connection` in a `Mutex` to satisfy `Send + Sync` on `ProjectStore`.
/// For a single-threaded CLI tool this adds no overhead — there's no contention.
#[derive(Clone)]
pub struct SqliteStore {
    pub(crate) conn: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
}

impl SqliteStore {
    /// Open or create a database at the given path.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        debug!(path = %path.display(), "Opening database");
        let conn = Connection::open(path).map_err(StorageError::Sqlite)?;

        // Enable WAL mode for better concurrent read performance
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(StorageError::Sqlite)?;
        // Enable foreign key enforcement
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(StorageError::Sqlite)?;

        Ok(Self {
            conn: std::sync::Arc::new(Mutex::new(conn)),
        })
    }

    /// Create an in-memory database (for testing).
    pub fn open_in_memory() -> Result<Self, StorageError> {
        debug!("Opening in-memory database");
        let conn = Connection::open_in_memory().map_err(StorageError::Sqlite)?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(StorageError::Sqlite)?;
        Ok(Self {
            conn: std::sync::Arc::new(Mutex::new(conn)),
        })
    }

    /// Run all pending database migrations.
    pub fn migrate(&self) -> Result<(), StorageError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::Migration(format!("lock poisoned: {e}")))?;
        migrations::run_migrations(&conn)
    }

    /// Get the path to the default database location.
    ///
    /// Uses `$RUSTODIAN_DB` if set, otherwise `~/.local/share/rustodian/rustodian.db`.
    pub fn default_path() -> Result<PathBuf, CoreError> {
        if let Ok(path) = std::env::var("RUSTODIAN_DB") {
            return Ok(PathBuf::from(path));
        }

        let data_dir = dirs_next_or_fallback();
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| CoreError::Internal(format!("failed to create data dir: {e}")))?;
        Ok(data_dir.join("rustodian.db"))
    }
}

/// Get the data directory, with a fallback if dirs isn't available.
fn dirs_next_or_fallback() -> PathBuf {
    // Simple fallback: ~/.local/share/rustodian
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("rustodian")
}

impl ProjectStore for SqliteStore {
    fn save_project(&self, project: &Project) -> Result<ProjectId, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Storage(format!("lock poisoned: {e}")))?;

        conn.execute(
            "INSERT INTO projects (id, name, path, discovered_at, last_scanned_at, metadata_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(path) DO UPDATE SET
                name=excluded.name,
                discovered_at=excluded.discovered_at,
                last_scanned_at=excluded.last_scanned_at,
                metadata_json=excluded.metadata_json;",
            params![
                project.id.to_string(),
                project.name,
                project.path.to_string_lossy(),
                project.discovered_at.to_rfc3339(),
                project.last_scanned_at.map(|d| d.to_rfc3339()),
                serde_json::json!({
                    "meta": project.metadata,
                    "vcs": project.vcs,
                    "languages": project.languages
                })
                .to_string()
            ],
        )
        .map_err(|e| CoreError::Storage(format!("failed to save project: {e}")))?;

        // we'll update the project languages table
        conn.execute(
            "DELETE FROM project_languages WHERE project_id = ?1",
            params![project.id.to_string()],
        )
        .map_err(|e| CoreError::Storage(format!("failed to clean languages: {e}")))?;

        for detection in &project.languages {
            conn.execute(
                "INSERT INTO project_languages (project_id, language, confidence) VALUES (?1, ?2, ?3)",
                params![project.id.to_string(), detection.language.to_string(), detection.confidence.to_string()]
            ).map_err(|e| CoreError::Storage(format!("failed to save project languages: {e}")))?;
        }

        Ok(project.id.clone())
    }

    fn get_project(&self, id: &ProjectId) -> Result<Option<Project>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Storage(format!("lock poisoned: {e}")))?;

        let mut stmt = conn.prepare("SELECT id, name, path, discovered_at, last_scanned_at, metadata_json FROM projects WHERE id = ?1")
            .map_err(|e| CoreError::Storage(format!("prepare error: {e}")))?;

        let project = stmt
            .query_row(params![id.to_string()], |row| {
                let id_str: String = row.get(0)?;
                let name: String = row.get(1)?;
                let path_str: String = row.get(2)?;
                let disc_str: String = row.get(3)?;
                let scan_str: Option<String> = row.get(4)?;
                let meta_str: String = row.get(5)?;

                Ok((id_str, name, path_str, disc_str, scan_str, meta_str))
            })
            .optional()
            .map_err(|e| CoreError::Storage(format!("query error: {e}")))?;

        if let Some((id_str, name, path_str, disc_str, scan_str, meta_str)) = project {
            let id = ProjectId(uuid::Uuid::parse_str(&id_str).unwrap_or_default());
            let path = PathBuf::from(path_str);
            let discovered_at = chrono::DateTime::parse_from_rfc3339(&disc_str)
                .unwrap()
                .with_timezone(&chrono::Utc);
            let last_scanned_at = scan_str.map(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .unwrap()
                    .with_timezone(&chrono::Utc)
            });

            let meta_json: serde_json::Value =
                serde_json::from_str(&meta_str).unwrap_or(serde_json::json!({}));
            let metadata: ProjectMetadata = serde_json::from_value(
                meta_json
                    .get("meta")
                    .cloned()
                    .unwrap_or(serde_json::json!({})),
            )
            .unwrap_or_default();
            let vcs = serde_json::from_value(
                meta_json
                    .get("vcs")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            )
            .unwrap_or(None);
            let languages = serde_json::from_value(
                meta_json
                    .get("languages")
                    .cloned()
                    .unwrap_or(serde_json::json!([])),
            )
            .unwrap_or_default();

            Ok(Some(Project {
                id,
                name,
                path,
                languages,
                vcs,
                discovered_at,
                last_scanned_at,
                metadata,
            }))
        } else {
            Ok(None)
        }
    }

    fn list_projects(&self) -> Result<Vec<Project>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Storage(format!("lock poisoned: {e}")))?;

        let mut stmt = conn.prepare("SELECT id, name, path, discovered_at, last_scanned_at, metadata_json FROM projects")
            .map_err(|e| CoreError::Storage(format!("prepare error: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                let id_str: String = row.get(0)?;
                let name: String = row.get(1)?;
                let path_str: String = row.get(2)?;
                let disc_str: String = row.get(3)?;
                let scan_str: Option<String> = row.get(4)?;
                let meta_str: String = row.get(5)?;
                Ok((id_str, name, path_str, disc_str, scan_str, meta_str))
            })
            .map_err(|e| CoreError::Storage(format!("query map error: {e}")))?;

        let mut projects = Vec::new();
        for (id_str, name, path_str, disc_str, scan_str, meta_str) in rows.flatten() {
            let id = ProjectId(uuid::Uuid::parse_str(&id_str).unwrap_or_default());
            let path = PathBuf::from(path_str);
            let discovered_at = chrono::DateTime::parse_from_rfc3339(&disc_str)
                .unwrap()
                .with_timezone(&chrono::Utc);
            let last_scanned_at = scan_str.map(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .unwrap()
                    .with_timezone(&chrono::Utc)
            });

            let meta_json: serde_json::Value =
                serde_json::from_str(&meta_str).unwrap_or(serde_json::json!({}));
            let metadata: ProjectMetadata = serde_json::from_value(
                meta_json
                    .get("meta")
                    .cloned()
                    .unwrap_or(serde_json::json!({})),
            )
            .unwrap_or_default();
            let vcs = serde_json::from_value(
                meta_json
                    .get("vcs")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            )
            .unwrap_or(None);
            let languages = serde_json::from_value(
                meta_json
                    .get("languages")
                    .cloned()
                    .unwrap_or(serde_json::json!([])),
            )
            .unwrap_or_default();

            projects.push(Project {
                id,
                name,
                path,
                languages,
                vcs,
                discovered_at,
                last_scanned_at,
                metadata,
            });
        }
        Ok(projects)
    }

    fn delete_project(&self, id: &ProjectId) -> Result<bool, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Storage(format!("lock poisoned: {e}")))?;
        let count = conn
            .execute(
                "DELETE FROM projects WHERE id = ?1",
                params![id.to_string()],
            )
            .map_err(|e| CoreError::Storage(format!("delete error: {e}")))?;
        Ok(count > 0)
    }

    fn find_by_path(&self, path: &Path) -> Result<Option<Project>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Storage(format!("lock poisoned: {e}")))?;

        let mut stmt = conn.prepare("SELECT id, name, path, discovered_at, last_scanned_at, metadata_json FROM projects WHERE path = ?1")
            .map_err(|e| CoreError::Storage(format!("prepare error: {e}")))?;

        let project = stmt
            .query_row(params![path.to_string_lossy()], |row| {
                let id_str: String = row.get(0)?;
                let name: String = row.get(1)?;
                let path_str: String = row.get(2)?;
                let disc_str: String = row.get(3)?;
                let scan_str: Option<String> = row.get(4)?;
                let meta_str: String = row.get(5)?;
                Ok((id_str, name, path_str, disc_str, scan_str, meta_str))
            })
            .optional()
            .map_err(|e| CoreError::Storage(format!("query error: {e}")))?;

        if let Some((id_str, name, path_str, disc_str, scan_str, meta_str)) = project {
            let id = ProjectId(uuid::Uuid::parse_str(&id_str).unwrap_or_default());
            let path = PathBuf::from(path_str);
            let discovered_at = chrono::DateTime::parse_from_rfc3339(&disc_str)
                .unwrap()
                .with_timezone(&chrono::Utc);
            let last_scanned_at = scan_str.map(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .unwrap()
                    .with_timezone(&chrono::Utc)
            });

            let meta_json: serde_json::Value =
                serde_json::from_str(&meta_str).unwrap_or(serde_json::json!({}));
            let metadata: ProjectMetadata = serde_json::from_value(
                meta_json
                    .get("meta")
                    .cloned()
                    .unwrap_or(serde_json::json!({})),
            )
            .unwrap_or_default();
            let vcs = serde_json::from_value(
                meta_json
                    .get("vcs")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            )
            .unwrap_or(None);
            let languages = serde_json::from_value(
                meta_json
                    .get("languages")
                    .cloned()
                    .unwrap_or(serde_json::json!([])),
            )
            .unwrap_or_default();

            Ok(Some(Project {
                id,
                name,
                path,
                languages,
                vcs,
                discovered_at,
                last_scanned_at,
                metadata,
            }))
        } else {
            Ok(None)
        }
    }

    fn save_scan(&self, scan: &ScanRecord) -> Result<ScanId, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Storage(format!("lock poisoned: {e}")))?;

        conn.execute(
            "INSERT INTO scans (id, root_path, started_at, completed_at, projects_found, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                completed_at=excluded.completed_at,
                projects_found=excluded.projects_found,
                status=excluded.status;",
            params![
                scan.id.to_string(),
                scan.root_path.to_string_lossy(),
                scan.started_at.to_rfc3339(),
                scan.completed_at.map(|d| d.to_rfc3339()),
                scan.projects_found,
                scan.status.to_string()
            ],
        )
        .map_err(|e| CoreError::Storage(format!("failed to save scan: {e}")))?;

        Ok(scan.id.clone())
    }

    fn get_latest_scan(&self) -> Result<Option<ScanRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Storage(format!("lock poisoned: {e}")))?;

        let mut stmt = conn.prepare("SELECT id, root_path, started_at, completed_at, projects_found, status FROM scans ORDER BY started_at DESC LIMIT 1")
            .map_err(|e| CoreError::Storage(format!("prepare error: {e}")))?;

        let scan = stmt
            .query_row([], |row| {
                let id_str: String = row.get(0)?;
                let root_str: String = row.get(1)?;
                let start_str: String = row.get(2)?;
                let end_str: Option<String> = row.get(3)?;
                let found: usize = row.get(4)?;
                let status_str: String = row.get(5)?;
                Ok((id_str, root_str, start_str, end_str, found, status_str))
            })
            .optional()
            .map_err(|e| CoreError::Storage(format!("query error: {e}")))?;

        if let Some((id_str, root_str, start_str, end_str, found, status_str)) = scan {
            let id = ScanId(uuid::Uuid::parse_str(&id_str).unwrap_or_default());
            let root_path = PathBuf::from(root_str);
            let started_at = chrono::DateTime::parse_from_rfc3339(&start_str)
                .unwrap()
                .with_timezone(&chrono::Utc);
            let completed_at = end_str.map(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .unwrap()
                    .with_timezone(&chrono::Utc)
            });
            let status = match status_str.as_str() {
                "running" => ScanStatus::Running,
                "completed" => ScanStatus::Completed,
                _ => ScanStatus::Failed,
            };

            Ok(Some(ScanRecord {
                id,
                root_path,
                started_at,
                completed_at,
                projects_found: found,
                status,
            }))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_in_memory() {
        let store = SqliteStore::open_in_memory().expect("should open in-memory db");
        store.migrate().expect("should run migrations");
    }

    #[test]
    fn test_migrations_idempotent() {
        let store = SqliteStore::open_in_memory().expect("should open");
        store.migrate().expect("first migration");
        store
            .migrate()
            .expect("second migration should be idempotent");
    }
}
