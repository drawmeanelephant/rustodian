# Storage Internals

This document outlines the SQLite persistence layer implemented in `crates/rustodian-storage/src/store.rs`.

## Database Schema

The database relies on a straightforward hybrid relational/document schema:

```sql
CREATE TABLE projects (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    path            TEXT NOT NULL UNIQUE,
    discovered_at   TEXT NOT NULL,
    last_scanned_at TEXT,
    metadata_json   TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE project_languages (
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    language    TEXT NOT NULL,
    confidence  TEXT NOT NULL DEFAULT 'high',
    PRIMARY KEY (project_id, language)
);

CREATE TABLE scans (
    id              TEXT PRIMARY KEY,
    root_path       TEXT NOT NULL,
    started_at      TEXT NOT NULL,
    completed_at    TEXT,
    projects_found  INTEGER NOT NULL DEFAULT 0,
    status          TEXT NOT NULL DEFAULT 'running'
);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE remote_projects (
    repo_slug         TEXT PRIMARY KEY,
    preserve_patterns TEXT NOT NULL DEFAULT '[]'
);

CREATE TABLE project_logs (
    id           TEXT PRIMARY KEY,
    project_id   TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    command_name TEXT NOT NULL,
    exit_code    INTEGER,
    log_text     TEXT NOT NULL DEFAULT '',
    run_at       TEXT NOT NULL
);
```

## Connection Pool and Concurrency

Rustodian utilizes `r2d2` for an SQLite connection pool. The database is configured to use Write-Ahead Logging (`PRAGMA journal_mode = WAL`), which significantly improves concurrent access by allowing readers to operate alongside a writer. This is critical for the desktop app where background scans write data while the UI queries it.

To ensure smooth concurrency, `busy_timeout = 5000` is configured. If the database is locked by a writer, this instructs SQLite to wait up to 5 seconds before returning a lock error, preventing transient UI failures.

## Upserting Projects

Projects are updated using an upsert pattern via `ON CONFLICT(path) DO UPDATE`. The `path` field is treated as the unique disk identifier. When a conflict occurs (meaning the project was already discovered in a previous scan), the upsert command updates fields such as `name`, `last_scanned_at`, `metadata_json`, and notably `discovered_at` (`discovered_at=excluded.discovered_at`).

During upserts, the `project_languages` table uses a delete-and-reinsert pattern. Existing language records for the ID are deleted and new ones inserted, which cleanly syncs data without tracking individual row changes.

## JSON Metadata Strategy

Instead of a normalized schema with strict, explicit columns for varying domains like VCS status, CI features, or languages, Rustodian uses a hybrid approach. The `metadata_json` column stores a JSON blob containing this data.

**Why JSON?** Packing this into JSON keeps the schema highly stable as the domain evolves. It avoids brittle, tedious SQL schema migrations every time a new metadata field is added to a project. This allows for rapid development on Rust structs (which `serde_json` maps natively to the blob).

## Project Languages Side Table

While language data technically exists within the `metadata_json` blob, there is also a dedicated `project_languages` side-table.

**Why a side-table?** This relational table exists purely for read performance. Standard SQLite JSON querying (`json_extract`) is slower to index and query over thousands of rows. Offloading this metric to a dedicated table guarantees high-performance filtering and sorting for desktop views.

## Row Deserialization

To avoid boilerplate, row deserialization into a `Project` struct is centralized in `parse_project_row`. This shared utility is used by read operations like `get_project` and `list_projects`, serving as the single truth for translating SQLite into Rust.

## Known Tradeoffs

The storage layer design includes several notable tradeoffs:

- **Malformed Record Handling**: If `parse_project_row` encounters invalid data (e.g., a corrupted JSON blob), functions like `list_projects` skip the row and log a warning rather than failing the entire query. This prioritizes application availability and UI stability over strict data integrity.
- **Timestamp Overwrites**: Because `discovered_at` is updated during the upsert (`discovered_at=excluded.discovered_at`), the application intentionally sacrifices accurate "first-seen" historical tracking for the sake of simpler `ON CONFLICT DO UPDATE` query logic.
- **WAL Concurrency Limits**: While WAL mode allows concurrent readers alongside a writer, it is still bottlenecked by a single active writer. If a background write transaction takes longer than the 5000ms `busy_timeout` limit, the UI will receive a "database is locked" error.
- **Write Churn**: The delete-and-reinsert pattern for the `project_languages` side table generates excess SQLite write churn and WAL size growth compared to granular row updates, trading off storage I/O performance for application-layer simplicity.
