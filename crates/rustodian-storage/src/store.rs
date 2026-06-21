//! `SQLite` implementation of [`ProjectStore`].

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;
use tracing::debug;

use rustodian_core::CoreError;
use rustodian_core::traits::ProjectStore;
use rustodian_types::{Project, ProjectId, ScanId, ScanRecord};

use crate::error::StorageError;
use crate::migrations;

/// `SQLite`-backed project store.
///
/// Wraps `Connection` in a `Mutex` to satisfy `Send + Sync` on `ProjectStore`.
/// For a single-threaded CLI tool this adds no overhead — there's no contention.
pub struct SqliteStore {
    conn: Mutex<Connection>,
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
            conn: Mutex::new(conn),
        })
    }

    /// Create an in-memory database (for testing).
    pub fn open_in_memory() -> Result<Self, StorageError> {
        debug!("Opening in-memory database");
        let conn = Connection::open_in_memory().map_err(StorageError::Sqlite)?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(StorageError::Sqlite)?;
        Ok(Self {
            conn: Mutex::new(conn),
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
        let _ = project;
        todo!("INSERT project into SQLite")
    }

    fn get_project(&self, id: &ProjectId) -> Result<Option<Project>, CoreError> {
        let _ = id;
        todo!("SELECT project by ID")
    }

    fn list_projects(&self) -> Result<Vec<Project>, CoreError> {
        todo!("SELECT all projects")
    }

    fn delete_project(&self, id: &ProjectId) -> Result<bool, CoreError> {
        let _ = id;
        todo!("DELETE project by ID")
    }

    fn find_by_path(&self, path: &Path) -> Result<Option<Project>, CoreError> {
        let _ = path;
        todo!("SELECT project by path")
    }

    fn save_scan(&self, scan: &ScanRecord) -> Result<ScanId, CoreError> {
        let _ = scan;
        todo!("INSERT scan record")
    }

    fn get_latest_scan(&self) -> Result<Option<ScanRecord>, CoreError> {
        todo!("SELECT latest scan")
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
