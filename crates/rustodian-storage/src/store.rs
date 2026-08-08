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

/// Connection-local pragmas applied to every pooled connection.
///
/// These configure only the individual connection and take no database locks,
/// so they are safe to run concurrently while other connections read or write.
/// `busy_timeout` comes first so every subsequent connection-local statement
/// has a busy handler installed.
const CONNECTION_PRAGMAS: &str = "
    PRAGMA busy_timeout = 5000;
    PRAGMA synchronous = NORMAL;
    PRAGMA foreign_keys = ON;
";

/// Build an `r2d2` connection manager for a file-backed database.
///
/// Per-connection initialization applies **only** connection-local pragmas.
/// Persistent, database-wide properties (journal mode) are configured once by
/// [`bootstrap_journal_mode_wal`] before the pool exists.
fn file_manager(path: &Path) -> SqliteConnectionManager {
    SqliteConnectionManager::file(path).with_init(|c| c.execute_batch(CONNECTION_PRAGMAS))
}

/// Build an `r2d2` connection manager for a shared-cache in-memory database.
fn memory_manager(db_url: &str) -> SqliteConnectionManager {
    SqliteConnectionManager::file(db_url).with_init(|c| c.execute_batch(CONNECTION_PRAGMAS))
}

/// Set the persistent `journal_mode=WAL` database property exactly once, on a
/// dedicated connection before any pool connections exist.
///
/// The journal mode is stored in the database file header and is therefore a
/// database-wide, one-time property — not a per-connection one. Changing it
/// requires the database write lock (via `sqlite3BtreeSetVersion`), so running
/// it inside per-connection pool initialization makes the connections r2d2
/// eagerly creates at pool construction race one another with `SQLITE_BUSY`
/// ("database is locked") on a fresh database that is not yet in WAL mode.
///
/// For in-memory databases this is a harmless no-op (`WAL` is unsupported on
/// memory databases and the pragma reports `memory`), but it keeps the
/// bootstrap path uniform.
fn bootstrap_journal_mode_wal(path: &Path) -> Result<(), StorageError> {
    let conn = rusqlite::Connection::open(path).map_err(StorageError::Sqlite)?;
    // Configure the busy timeout before the journal-mode transition: the
    // transition takes the database write lock, and two Rustodian processes
    // starting concurrently on the same fresh database would otherwise fail
    // it immediately with `SQLITE_BUSY` instead of waiting for the other
    // process's lock to be released.
    conn.execute_batch("PRAGMA busy_timeout = 5000; PRAGMA journal_mode = WAL;")
        .map_err(StorageError::Sqlite)?;
    Ok(())
}

impl SqliteStore {
    /// Open or create a database at the given path.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        debug!(path = %path.display(), "Opening database pool");
        bootstrap_journal_mode_wal(path)?;

        let pool = r2d2::Pool::new(file_manager(path))
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
        bootstrap_journal_mode_wal(std::path::Path::new(&db_url))?;

        let pool = r2d2::Pool::builder()
            .max_size(1)
            .build(memory_manager(&db_url))
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

        // `discovered_at` is intentionally left out of `DO UPDATE SET` so that the
        // original "first-seen" timestamp survives repeated saves of the same path.
        tx.execute(
            "INSERT INTO projects (id, name, path, discovered_at, last_scanned_at, metadata_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(path) DO UPDATE SET
                name=excluded.name,
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

    fn prune_logs(&self, project_id: &str, limit: usize) -> Result<usize, CoreError> {
        SqliteStore::prune_logs(self, project_id, limit)
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

    pub fn list_settings(&self) -> Result<std::collections::HashMap<String, String>, CoreError> {
        let conn = self.get_conn()?;
        let mut stmt = conn
            .prepare("SELECT key, value FROM settings")
            .map_err(|e| CoreError::Storage(format!("prepare error: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                let key: String = row.get(0)?;
                let value: String = row.get(1)?;
                Ok((key, value))
            })
            .map_err(|e| CoreError::Storage(format!("query error: {e}")))?;

        let mut settings = std::collections::HashMap::new();
        for (k, v) in rows.flatten() {
            settings.insert(k, v);
        }
        Ok(settings)
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
    #[test]
    fn test_upsert_preserves_discovered_at() {
        use rustodian_core::traits::ProjectStore;
        use rustodian_types::{Project, ProjectId, ProjectMetadata, VcsInfo, VcsType};
        use std::path::PathBuf;

        fn ts(s: &str) -> chrono::DateTime<chrono::Utc> {
            chrono::DateTime::parse_from_rfc3339(s)
                .unwrap()
                .with_timezone(&chrono::Utc)
        }

        let store = SqliteStore::open_in_memory().unwrap();
        store.migrate().unwrap();

        let path = PathBuf::from("/test/preserve-discovery");

        // First save: discovery timestamp A.
        let proj_a = Project {
            id: ProjectId::new(),
            name: "proj_a".to_string(),
            path: path.clone(),
            discovered_at: ts("2024-01-01T00:00:00Z"),
            last_scanned_at: Some(ts("2024-01-01T01:00:00Z")),
            vcs: None,
            languages: vec![],
            metadata: ProjectMetadata::default(),
        };
        let id = store.save_project(&proj_a).unwrap();

        // Second save of the same path: later discovery timestamp B, changed
        // metadata, last_scanned_at, and VCS data (as a fresh scan would produce).
        let metadata_b = ProjectMetadata {
            description: Some("updated description".to_string()),
            tags: vec!["rust".to_string()],
            ..ProjectMetadata::default()
        };

        let proj_b = Project {
            id: ProjectId::new(),
            name: "proj_b_updated".to_string(),
            path,
            discovered_at: ts("2024-06-01T00:00:00Z"),
            last_scanned_at: Some(ts("2024-06-01T12:00:00Z")),
            vcs: Some(VcsInfo {
                vcs_type: VcsType::Git,
                branch: Some("main".to_string()),
                remote_url: Some("https://example.com/repo.git".to_string()),
                is_dirty: true,
                last_commit: None,
            }),
            languages: vec![],
            metadata: metadata_b,
        };
        store.save_project(&proj_b).unwrap();

        // The original discovery timestamp must survive the upsert...
        let loaded = store.get_project(&id).unwrap().unwrap();
        assert_eq!(loaded.discovered_at, ts("2024-01-01T00:00:00Z"));
        assert_eq!(loaded.id, id);

        // ...while the mutable fields reflect the second save.
        assert_eq!(loaded.name, "proj_b_updated");
        assert_eq!(loaded.last_scanned_at, Some(ts("2024-06-01T12:00:00Z")));
        assert_eq!(
            loaded.metadata.description.as_deref(),
            Some("updated description")
        );
        assert_eq!(loaded.metadata.tags, vec!["rust".to_string()]);
        let vcs = loaded.vcs.as_ref().unwrap();
        assert_eq!(vcs.vcs_type, VcsType::Git);
        assert_eq!(vcs.branch.as_deref(), Some("main"));

        // A brand-new path still uses its supplied discovery timestamp.
        let proj_c = Project {
            id: ProjectId::new(),
            name: "proj_c".to_string(),
            path: PathBuf::from("/test/brand-new"),
            discovered_at: ts("2024-03-15T08:30:00Z"),
            last_scanned_at: None,
            vcs: None,
            languages: vec![],
            metadata: ProjectMetadata::default(),
        };
        let id_c = store.save_project(&proj_c).unwrap();
        let loaded_c = store.get_project(&id_c).unwrap().unwrap();
        assert_eq!(loaded_c.discovered_at, ts("2024-03-15T08:30:00Z"));
    }

    use super::*;

    #[test]
    fn test_delete_project_cascades_associated_records() {
        use rustodian_core::traits::ProjectStore;
        use rustodian_types::{
            DetectionConfidence, Language, LanguageDetection, Project, ProjectId, ProjectLog,
        };
        use std::path::PathBuf;

        let store = SqliteStore::open_in_memory().unwrap();
        store.migrate().unwrap();

        let proj = Project {
            id: ProjectId::new(),
            name: "cascade-me".to_string(),
            path: PathBuf::from("/cascade"),
            discovered_at: chrono::Utc::now(),
            last_scanned_at: None,
            vcs: None,
            languages: vec![LanguageDetection {
                language: Language::Rust,
                confidence: DetectionConfidence::High,
                markers: vec![rustodian_types::LanguageMarker::ManifestFile(
                    "Cargo.toml".to_string(),
                )],
            }],
            metadata: rustodian_types::ProjectMetadata::default(),
        };
        store.save_project(&proj).unwrap();

        let log = ProjectLog {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: proj.id.to_string(),
            command_name: "test_cmd".to_string(),
            exit_code: Some(0),
            log_text: "log".to_string(),
            run_at: chrono::Utc::now(),
        };
        store.save_log(&log).unwrap();

        // Deleting the project must cascade to its logs and languages.
        assert!(store.delete_project(&proj.id).unwrap());
        assert!(store.get_project(&proj.id).unwrap().is_none());
        assert!(
            store
                .list_logs(&proj.id.to_string(), 10)
                .unwrap()
                .is_empty()
        );

        let conn = store.get_conn().unwrap();
        let language_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM project_languages WHERE project_id = ?1",
                rusqlite::params![proj.id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(language_count, 0, "languages must cascade on delete");
    }

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

    /// Per-connection initialization must never take database locks.
    ///
    /// This mirrors r2d2's pool-fill: a fresh connection is initialized while
    /// another connection holds an open write transaction on a database that
    /// has **not** yet been switched to WAL (exactly the state at pool
    /// construction on a fresh database). Connection-local pragmas succeed
    /// here. If `PRAGMA journal_mode = WAL` were part of per-connection
    /// initialization, it would try to take the database write lock
    /// (`sqlite3BtreeSetVersion` -> `sqlite3BtreeBeginTrans`) and fail
    /// deterministically with `database is locked`.
    ///
    /// This is the smallest deterministic reproduction of the failing
    /// initialization statement. The full race (r2d2 eagerly creating ten
    /// connections across three worker threads at `Pool::new`, several of
    /// which transition the same fresh database to WAL simultaneously) is
    /// probabilistic per run but is covered by
    /// [`test_repeated_fresh_open_migrate_write_no_lock_errors`].
    #[test]
    fn test_per_connection_init_takes_no_database_locks() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("init.db");

        // Connection A: opens the fresh DB (still in DELETE journal mode) and
        // holds a write transaction without ever transitioning to WAL.
        let conn_a = rusqlite::Connection::open(&db_path).unwrap();
        conn_a
            .execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT);
                 INSERT INTO t (v) VALUES ('x');",
            )
            .unwrap();

        // Connection B: a freshly opened connection running the per-connection
        // init pragmas, exactly as a pooled connection would at creation time.
        let conn_b = rusqlite::Connection::open(&db_path).unwrap();
        conn_b
            .execute_batch(CONNECTION_PRAGMAS)
            .unwrap_or_else(|e| {
                panic!(
                    "per-connection init must not take database locks, got: {e}. \
                 PRAGMA journal_mode=WAL must be configured once during bootstrap, \
                 not per pooled connection."
                )
            });

        drop(conn_a);
        drop(conn_b);
    }

    /// Repeatedly perform the CLI's exact startup sequence on fresh on-disk
    /// databases (open the pool exactly as `SqliteStore::open` does, migrate,
    /// run a small representative scan/write workload) and assert that the
    /// pool never reports a connection error such as `database is locked`.
    ///
    /// The r2d2 error handler is captured per iteration because the eager
    /// pool-fill at `Pool::new` used to log `database is locked` transiently
    /// (and then retry to success), which is exactly the "ERROR database is
    /// locked ... exit 0" symptom observed in the field. On the buggy
    /// implementation (journal mode configured per connection), the concurrent
    /// WAL transitions collide on the fresh database and this test fails with
    /// the captured lock errors.
    #[test]
    fn test_repeated_fresh_open_migrate_write_no_lock_errors() {
        use rustodian_core::traits::ProjectStore;
        use rustodian_types::{Project, ProjectId, ScanId, ScanRecord, ScanStatus};

        /// Collects every connection error r2d2 reports (eager pool-fill and
        /// checkout validation) instead of logging it.
        #[derive(Debug)]
        struct CaptureErrors(std::sync::Arc<std::sync::Mutex<Vec<String>>>);
        impl r2d2::HandleError<rusqlite::Error> for CaptureErrors {
            fn handle_error(&self, error: rusqlite::Error) {
                self.0.lock().unwrap().push(error.to_string());
            }
        }

        let dir = tempfile::tempdir().unwrap();

        for i in 0..12 {
            let db_path = dir.path().join(format!("fresh-{i}.db"));
            let failures = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

            // Mirror `SqliteStore::open` startup ordering exactly: the
            // one-time WAL bootstrap runs on a dedicated connection before
            // the pool exists, then the pool is built with the same manager
            // and init pragmas. Only the error handler differs so transient
            // pool errors are observable in the test.
            bootstrap_journal_mode_wal(&db_path).unwrap();
            let pool = r2d2::Pool::builder()
                .error_handler(Box::new(CaptureErrors(failures.clone())))
                .build(file_manager(&db_path))
                .unwrap();
            let store = SqliteStore {
                pool: std::sync::Arc::new(pool),
            };
            store.migrate().unwrap();

            // Small representative scan/write workload.
            for n in 0..3 {
                let project = Project {
                    id: ProjectId::new(),
                    name: format!("proj-{i}-{n}"),
                    path: PathBuf::from(format!("/projects/{i}/{n}")),
                    discovered_at: chrono::Utc::now(),
                    last_scanned_at: None,
                    vcs: None,
                    languages: vec![],
                    metadata: rustodian_types::ProjectMetadata::default(),
                };
                store.save_project(&project).unwrap();
            }
            let scan = ScanRecord {
                id: ScanId::new(),
                root_path: PathBuf::from("/projects"),
                started_at: chrono::Utc::now(),
                completed_at: Some(chrono::Utc::now()),
                projects_found: 3,
                status: ScanStatus::Completed,
            };
            store.save_scan(&scan).unwrap();
            assert_eq!(store.list_projects().unwrap().len(), 3);

            // Multiple pooled connections must be acquirable concurrently.
            let handles: Vec<_> = (0..6)
                .map(|_| {
                    let store = store.clone();
                    std::thread::spawn(move || {
                        let conn = store.get_conn().unwrap();
                        conn.query_row("SELECT COUNT(*) FROM projects", [], |r| r.get::<_, i64>(0))
                            .unwrap();
                    })
                })
                .collect();
            for h in handles {
                h.join().unwrap();
            }

            let failures = failures.lock().unwrap();
            assert!(
                failures.is_empty(),
                "iteration {i}: pool reported connection errors: {failures:?}"
            );
        }
    }

    /// The exact CLI startup sequence (`SqliteStore::open` + `migrate`) must
    /// succeed repeatedly on fresh on-disk databases.
    #[test]
    fn test_repeated_cli_startup_sequence() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..5 {
            let db_path = dir.path().join(format!("cli-{i}.db"));
            let store = SqliteStore::open(&db_path).unwrap();
            store.migrate().unwrap();
        }
    }

    /// WAL must actually be active for an on-disk database.
    #[test]
    fn test_wal_active_on_disk_database() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("wal.db");
        let store = SqliteStore::open(&db_path).unwrap();
        store.migrate().unwrap();

        let conn = store.get_conn().unwrap();
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode.to_lowercase(), "wal");

        // A WAL-mode database has the -wal and -shm sidecar files.
        assert!(dir.path().join("wal.db-wal").exists());
        assert!(dir.path().join("wal.db-shm").exists());
    }

    /// Foreign key enforcement must remain enabled on pooled connections.
    #[test]
    fn test_foreign_keys_enabled_on_pooled_connections() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(&dir.path().join("fk.db")).unwrap();
        store.migrate().unwrap();

        let conn = store.get_conn().unwrap();
        let err = conn
            .execute(
                "INSERT INTO project_languages (project_id, language, confidence)
                 VALUES ('no-such-project', 'rust', 'high')",
                [],
            )
            .unwrap_err();
        assert!(
            err.to_string().contains("FOREIGN KEY"),
            "expected foreign key violation, got: {err}"
        );
    }

    /// The busy timeout must remain configured on pooled connections.
    #[test]
    fn test_busy_timeout_configured_on_pooled_connections() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(&dir.path().join("busy.db")).unwrap();
        store.migrate().unwrap();

        let conn = store.get_conn().unwrap();
        let timeout: i64 = conn
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .unwrap();
        assert_eq!(timeout, 5000);
    }

    /// Multiple pooled connections can be acquired concurrently and share the
    /// database without lock errors.
    #[test]
    fn test_multiple_pooled_connections_acquired_safely() {
        use rustodian_core::traits::ProjectStore;
        use rustodian_types::{Project, ProjectId};

        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(&dir.path().join("concurrent.db")).unwrap();
        store.migrate().unwrap();

        let handles: Vec<_> = (0..8)
            .map(|i| {
                let store = store.clone();
                std::thread::spawn(move || {
                    let project = Project {
                        id: ProjectId::new(),
                        name: format!("proj-{i}"),
                        path: PathBuf::from(format!("/projects/{i}")),
                        discovered_at: chrono::Utc::now(),
                        last_scanned_at: None,
                        vcs: None,
                        languages: vec![],
                        metadata: rustodian_types::ProjectMetadata::default(),
                    };
                    store.save_project(&project).unwrap();
                    store.list_projects().unwrap();
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(store.list_projects().unwrap().len(), 8);
    }

    /// A forced storage write failure must propagate through
    /// `Custodian::scan` as an error instead of being swallowed.
    ///
    /// A single-connection pool (`max_size(1)`) makes the setup deterministic:
    /// there is exactly one pooled connection, so switching it to
    /// `PRAGMA query_only` guarantees the scan's write fails with a real
    /// `SQLite` error (`SQLITE_READONLY`, "attempt to write a readonly
    /// database") instead of relying on sequential checkouts visiting every
    /// connection of a larger pool.
    #[test]
    fn test_scan_write_failure_propagates_through_scan() {
        use rustodian_core::Custodian;
        use rustodian_core::runner::DefaultCommandRunner;
        use rustodian_core::traits::{GitInspector, ProjectScanner};
        use rustodian_types::{ScanConfig, VcsInfo};

        struct NoGit;
        impl GitInspector for NoGit {
            fn inspect(&self, _path: &Path) -> Result<Option<VcsInfo>, CoreError> {
                Ok(None)
            }
            fn get_dirty_files(&self, _path: &Path) -> Result<Vec<PathBuf>, CoreError> {
                Ok(vec![])
            }
        }

        struct OneProject;
        impl ProjectScanner for OneProject {
            fn scan(
                &self,
                _root: &Path,
                _config: &ScanConfig,
            ) -> Result<Vec<rustodian_core::traits::DiscoveredProject>, CoreError> {
                Ok(vec![rustodian_core::traits::DiscoveredProject {
                    name: "p".to_string(),
                    path: PathBuf::from("/projects/p"),
                    languages: vec![],
                    commands: vec![],
                }])
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Single-connection pool so `query_only` deterministically applies to
        // the only connection the scan can use.
        bootstrap_journal_mode_wal(&db_path).unwrap();
        let pool = r2d2::Pool::builder()
            .max_size(1)
            .build(file_manager(&db_path))
            .unwrap();
        let store = SqliteStore {
            pool: std::sync::Arc::new(pool),
        };
        store.migrate().unwrap();

        // Force the sole pooled connection to reject writes, then return it
        // to the pool.
        let conn = store.get_conn().unwrap();
        conn.execute_batch("PRAGMA query_only = ON;").unwrap();
        drop(conn);

        let custodian = Custodian::new(
            Box::new(store),
            Box::new(OneProject),
            Box::new(NoGit),
            Box::new(DefaultCommandRunner),
        );

        let err = custodian
            .scan(Path::new("/projects"), &ScanConfig::default())
            .expect_err("scan must propagate the storage write failure");
        assert!(
            err.to_string().contains("readonly"),
            "expected a readonly storage error, got: {err}"
        );
    }
}
