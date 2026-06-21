//! Database migration management.

use rusqlite::Connection;
use tracing::info;

use crate::error::StorageError;

/// The initial database schema.
const MIGRATION_001: &str = r#"
CREATE TABLE IF NOT EXISTS projects (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    path            TEXT NOT NULL UNIQUE,
    discovered_at   TEXT NOT NULL,
    last_scanned_at TEXT,
    metadata_json   TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS project_languages (
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    language    TEXT NOT NULL,
    confidence  TEXT NOT NULL DEFAULT 'high',
    PRIMARY KEY (project_id, language)
);

CREATE TABLE IF NOT EXISTS scans (
    id              TEXT PRIMARY KEY,
    root_path       TEXT NOT NULL,
    started_at      TEXT NOT NULL,
    completed_at    TEXT,
    projects_found  INTEGER NOT NULL DEFAULT 0,
    status          TEXT NOT NULL DEFAULT 'running'
);

CREATE INDEX IF NOT EXISTS idx_projects_path ON projects(path);
CREATE INDEX IF NOT EXISTS idx_scans_started ON scans(started_at DESC);
"#;

/// Run all pending migrations.
pub fn run_migrations(conn: &Connection) -> Result<(), StorageError> {
    info!("Running database migrations");

    // Create migrations tracking table
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            id      INTEGER PRIMARY KEY,
            name    TEXT NOT NULL,
            applied TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .map_err(StorageError::Sqlite)?;

    // Check if migration 001 has been applied
    let applied: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM _migrations WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .map_err(StorageError::Sqlite)?;

    if !applied {
        info!("Applying migration 001: initial schema");
        conn.execute_batch(MIGRATION_001)
            .map_err(StorageError::Sqlite)?;
        conn.execute(
            "INSERT INTO _migrations (id, name) VALUES (1, 'initial_schema')",
            [],
        )
        .map_err(StorageError::Sqlite)?;
    }

    info!("Migrations complete");
    Ok(())
}
