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
```

## Connection Pool and Concurrency

Rustodian utilizes `r2d2` for an SQLite connection pool. The database is configured to use Write-Ahead Logging (`PRAGMA journal_mode = WAL`), which significantly improves concurrent access by allowing readers to operate alongside a writer. This is critical for the desktop app where background scans write data while the UI queries it.

To ensure smooth concurrency, `busy_timeout = 5000` is configured. If the database is locked by a writer, this instructs SQLite to wait up to 5 seconds before returning a lock error, preventing transient UI failures.

## Upserting Projects

Projects are updated using an upsert pattern via `ON CONFLICT(path) DO UPDATE`. The `path` field is treated as the unique disk identifier. When a conflict occurs (meaning the project was already discovered in a previous scan), the upsert command updates dynamic fields such as `name`, `last_scanned_at`, and `metadata_json`. Crucially, it excludes immutable fields like `id` and `discovered_at`, preserving the original first-seen timestamp and preventing ID churn that would break foreign key relations.

During upserts, the `project_languages` table uses a delete-and-reinsert pattern. Existing language records for the ID are deleted and new ones inserted, removing stale data simply.

## JSON Metadata Strategy

Instead of a normalized schema with complex migrations, Rustodian uses a hybrid approach. The `metadata_json` column stores a JSON blob containing project metadata, VCS details, and languages. Packing this into JSON keeps the schema stable as the domain evolves, preventing brittle migrations and allowing flexible parsing into Rust structs.

## Project Languages Side Table

While language data exists in `metadata_json`, there is also a dedicated `project_languages` table. This relational table enables fast filtering and sorting from the UI. Standard SQLite JSON querying is slower to index, so offloading this metric to a dedicated table guarantees high-performance desktop views.

## Row Deserialization

To avoid boilerplate, row deserialization into a `Project` struct is centralized in `parse_project_row`. This shared utility is used by read operations like `get_project` and `list_projects`, serving as the single truth for translating SQLite into Rust.

## Known Tradeoffs

The storage layer design includes several notable tradeoffs:

- **Malformed Record Handling**: If `parse_project_row` encounters invalid data (e.g., a corrupted JSON blob), functions like `list_projects` skip the row and log a warning rather than failing the entire query. This prioritizes application availability and UI stability over strict data integrity.
- **WAL Concurrency Limits**: While WAL mode allows concurrent readers alongside a writer, it is still bottlenecked by a single active writer. If a background write transaction takes longer than the 5000ms `busy_timeout` limit, the UI will receive a "database is locked" error.
- **Write Churn**: The delete-and-reinsert pattern for the `project_languages` side table generates excess SQLite write churn and WAL size growth compared to granular row updates, trading off storage I/O performance for application-layer simplicity.
