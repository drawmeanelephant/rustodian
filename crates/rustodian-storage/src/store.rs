//! `SQLite` implementation of [`ProjectStore`].

use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};
use tracing::debug;

use r2d2_sqlite::SqliteConnectionManager;

use rustodian_core::CoreError;
use rustodian_core::traits::ProjectStore;
use rustodian_types::{
    Project, ProjectId, ProjectLog, ProjectMetadata, ScanId, ScanRecord, ScanStatus,
};

use crate::error::StorageError;
use crate::migrations;

/// `SQLite`-backed project store.
///
/// Uses an `r2d2` connection pool to allow concurrent reads/writes and prevent lock contention.
#[derive(Clone)]
pub struct SqliteStore {
    pub(crate) pool: std::sync::Arc<r2d2::Pool<SqliteConnectionManager>>,
}

impl SqliteStore {
    /// Open or create a database at the given path.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        debug!(path = %path.display(), "Opening database pool");
        let manager = SqliteConnectionManager::file(path).with_init(|c| {
            c.execute_batch(
                "
                    PRAGMA journal_mode = WAL;
                    PRAGMA synchronous = NORMAL;
                    PRAGMA busy_timeout = 5000;
                    PRAGMA foreign_keys = ON;
                ",
            )
        });
        let pool = r2d2::Pool::new(manager)
            .map_err(|e| StorageError::Migration(format!("failed to create database pool: {e}")))?;

        Ok(Self {
            pool: std::sync::Arc::new(pool),
        })
    }

    /// Create an in-memory database (for testing).
    pub fn open_in_memory() -> Result<Self, StorageError> {
        debug!("Opening in-memory database pool");
        let uuid = uuid::Uuid::new_v4().to_string();
        let db_url = format!("file:{uuid}?mode=memory&cache=shared");
        let manager = SqliteConnectionManager::file(&db_url).with_init(|c| {
            c.execute_batch(
                "
                    PRAGMA journal_mode = WAL;
                    PRAGMA synchronous = NORMAL;
                    PRAGMA busy_timeout = 5000;
                    PRAGMA foreign_keys = ON;
                ",
            )
        });
        let pool = r2d2::Pool::builder()
            .max_size(1)
            .build(manager)
            .map_err(|e| {
                StorageError::Migration(format!("failed to create in-memory pool: {e}"))
            })?;

        Ok(Self {
            pool: std::sync::Arc::new(pool),
        })
    }

    /// Run all pending database migrations.
    pub fn migrate(&self) -> Result<(), StorageError> {
        let conn = self
            .get_conn()
            .map_err(|e| StorageError::Migration(e.to_string()))?;
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

    /// Get a pooled connection from the pool.
    pub(crate) fn get_conn(
        &self,
    ) -> Result<r2d2::PooledConnection<SqliteConnectionManager>, CoreError> {
        self.pool
            .get()
            .map_err(|e| CoreError::Storage(format!("failed to get database connection: {e}")))
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

/// Parse raw column values into a [`Project`].
///
/// Used by `get_project`, `list_projects`, and `find_by_path` to avoid
/// duplicating the deserialization logic.
fn parse_project_row(
    id_str: &str,
    name: String,
    path_str: String,
    disc_str: &str,
    scan_str: Option<String>,
    meta_str: &str,
) -> Result<Project, CoreError> {
    let id = ProjectId(
        uuid::Uuid::parse_str(id_str)
            .map_err(|e| CoreError::Storage(format!("invalid project UUID '{id_str}': {e}")))?,
    );
    let path = PathBuf::from(path_str);
    let discovered_at = chrono::DateTime::parse_from_rfc3339(disc_str)
        .map_err(|e| CoreError::Storage(format!("invalid timestamp '{disc_str}': {e}")))?
        .with_timezone(&chrono::Utc);
    let last_scanned_at = scan_str
        .map(|s| {
            chrono::DateTime::parse_from_rfc3339(&s)
                .map_err(|e| CoreError::Storage(format!("invalid timestamp '{s}': {e}")))
                .map(|dt| dt.with_timezone(&chrono::Utc))
        })
        .transpose()?;

    let meta_json: serde_json::Value = serde_json::from_str(meta_str).map_err(|e| {
        CoreError::Storage(format!("invalid metadata JSON for project '{name}': {e}"))
    })?;

    let meta_val = meta_json.get("meta").ok_or_else(|| {
        CoreError::Storage(format!(
            "metadata JSON for project '{name}' missing 'meta' field"
        ))
    })?;
    let metadata: ProjectMetadata = serde_json::from_value(meta_val.clone()).map_err(|e| {
        CoreError::Storage(format!(
            "failed to deserialize ProjectMetadata for project '{name}': {e}"
        ))
    })?;

    let vcs_val = meta_json.get("vcs").ok_or_else(|| {
        CoreError::Storage(format!(
            "metadata JSON for project '{name}' missing 'vcs' field"
        ))
    })?;
    let vcs = serde_json::from_value(vcs_val.clone()).map_err(|e| {
        CoreError::Storage(format!(
            "failed to deserialize VCS metadata for project '{name}': {e}"
        ))
    })?;

    let lang_val = meta_json.get("languages").ok_or_else(|| {
        CoreError::Storage(format!(
            "metadata JSON for project '{name}' missing 'languages' field"
        ))
    })?;
    let languages = serde_json::from_value(lang_val.clone()).map_err(|e| {
        CoreError::Storage(format!(
            "failed to deserialize languages metadata for project '{name}': {e}"
        ))
    })?;

    Ok(Project {
        id,
        name,
        path,
        languages,
        vcs,
        discovered_at,
        last_scanned_at,
        metadata,
    })
}

impl ProjectStore for SqliteStore {
    fn save_project(&self, project: &Project) -> Result<ProjectId, CoreError> {
        let mut conn = self.get_conn()?;
        let tx = conn
            .transaction()
            .map_err(|e| CoreError::Storage(format!("failed to begin transaction: {e}")))?;

        tx.execute(
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
        tx.execute(
            "DELETE FROM project_languages WHERE project_id = ?1",
            params![project.id.to_string()],
        )
        .map_err(|e| CoreError::Storage(format!("failed to clean languages: {e}")))?;

        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO project_languages (project_id, language, confidence) VALUES (?1, ?2, ?3)",
            ).map_err(|e| CoreError::Storage(format!("failed to prepare statement: {e}")))?;

            for detection in &project.languages {
                stmt.execute(params![
                    project.id.to_string(),
                    detection.language.to_string(),
                    detection.confidence.to_string()
                ])
                .map_err(|e| {
                    CoreError::Storage(format!("failed to save project languages: {e}"))
                })?;
            }
        }

        tx.commit()
            .map_err(|e| CoreError::Storage(format!("failed to commit transaction: {e}")))?;

        Ok(project.id.clone())
    }

    fn get_project(&self, id: &ProjectId) -> Result<Option<Project>, CoreError> {
        let conn = self.get_conn()?;

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
            Ok(Some(parse_project_row(
                &id_str, name, path_str, &disc_str, scan_str, &meta_str,
            )?))
        } else {
            Ok(None)
        }
    }

    fn list_projects(&self) -> Result<Vec<Project>, CoreError> {
        let conn = self.get_conn()?;

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
        for row_result in rows {
            let (id_str, name, path_str, disc_str, scan_str, meta_str) = match row_result {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("Skipping malformed project row: {e}");
                    continue;
                }
            };
            match parse_project_row(
                &id_str,
                name,
                path_str.clone(),
                &disc_str,
                scan_str,
                &meta_str,
            ) {
                Ok(proj) => projects.push(proj),
                Err(e) => {
                    tracing::warn!("Skipping invalid project data for path '{path_str}': {e}");
                }
            }
        }
        Ok(projects)
    }

    fn delete_project(&self, id: &ProjectId) -> Result<bool, CoreError> {
        let conn = self.get_conn()?;
        let count = conn
            .execute(
                "DELETE FROM projects WHERE id = ?1",
                params![id.to_string()],
            )
            .map_err(|e| CoreError::Storage(format!("delete error: {e}")))?;
        Ok(count > 0)
    }

    fn find_by_path(&self, path: &Path) -> Result<Option<Project>, CoreError> {
        let conn = self.get_conn()?;

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
            Ok(Some(parse_project_row(
                &id_str, name, path_str, &disc_str, scan_str, &meta_str,
            )?))
        } else {
            Ok(None)
        }
    }

    fn save_scan(&self, scan: &ScanRecord) -> Result<ScanId, CoreError> {
        let conn = self.get_conn()?;

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
        let conn = self.get_conn()?;

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
            let id =
                ScanId(uuid::Uuid::parse_str(&id_str).map_err(|e| {
                    CoreError::Storage(format!("invalid scan UUID '{id_str}': {e}"))
                })?);
            let root_path = PathBuf::from(root_str);
            let started_at = chrono::DateTime::parse_from_rfc3339(&start_str)
                .map_err(|e| CoreError::Storage(format!("invalid timestamp '{start_str}': {e}")))?
                .with_timezone(&chrono::Utc);
            let completed_at = end_str
                .map(|s| {
                    chrono::DateTime::parse_from_rfc3339(&s)
                        .map_err(|e| CoreError::Storage(format!("invalid timestamp '{s}': {e}")))
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                })
                .transpose()?;
            let status = match status_str.as_str() {
                "running" => ScanStatus::Running,
                "completed" => ScanStatus::Completed,
                "failed" => ScanStatus::Failed,
                other => return Err(CoreError::Storage(format!("invalid scan status '{other}'"))),
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

    fn save_log(&self, log: &ProjectLog) -> Result<(), CoreError> {
        SqliteStore::save_log(self, log)
    }

    fn list_logs(&self, project_id: &str, limit: usize) -> Result<Vec<ProjectLog>, CoreError> {
        SqliteStore::list_logs(self, project_id, limit)
    }

    fn get_log(&self, id: &str) -> Result<Option<ProjectLog>, CoreError> {
        SqliteStore::get_log(self, id)
    }

    fn get_latest_log(&self, project_id: &str) -> Result<Option<ProjectLog>, CoreError> {
        SqliteStore::get_latest_log(self, project_id)
    }
}

impl SqliteStore {
    pub fn get_setting(&self, key: &str) -> Result<Option<String>, CoreError> {
        let conn = self.get_conn()?;
        let mut stmt = conn
            .prepare("SELECT value FROM settings WHERE key = ?1")
            .map_err(|e| CoreError::Storage(format!("prepare error: {e}")))?;

        let value: Option<String> = stmt
            .query_row(params![key], |row| row.get(0))
            .optional()
            .map_err(|e| CoreError::Storage(format!("query error: {e}")))?;

        Ok(value)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), CoreError> {
        let conn = self.get_conn()?;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value;",
            params![key, value],
        )
        .map_err(|e| CoreError::Storage(format!("insert error: {e}")))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_save_project_upsert_and_malformed_json() {
        use rustodian_core::traits::ProjectStore;
        use rustodian_types::{Project, ProjectId};
        use std::path::PathBuf;

        let store = SqliteStore::open_in_memory().unwrap();
        store.migrate().unwrap();

        let mut proj = Project {
            id: ProjectId::new(),
            name: "test_proj".to_string(),
            path: PathBuf::from("/test"),
            discovered_at: chrono::Utc::now(),
            last_scanned_at: None,
            vcs: None,
            languages: vec![],
            metadata: rustodian_types::ProjectMetadata::default(),
        };

        // Initial save
        let id = store.save_project(&proj).unwrap();

        // Upsert save
        proj.name = "test_proj_updated".to_string();
        store.save_project(&proj).unwrap();

        let loaded = store.get_project(&id).unwrap().unwrap();
        assert_eq!(loaded.name, "test_proj_updated");

        // Manually break the json
        let conn = store.get_conn().unwrap();
        conn.execute(
            "UPDATE projects SET metadata_json = 'not_json' WHERE id = ?1",
            rusqlite::params![id.to_string()],
        )
        .unwrap();
        drop(conn);

        let err = store.get_project(&id).unwrap_err();
        println!("{err}");
        assert!(err.to_string().contains("invalid metadata JSON"));
    }
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
