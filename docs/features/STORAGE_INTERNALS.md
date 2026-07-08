# Storage Internals

This document outlines the SQLite persistence layer in `crates/rustodian-storage/src/store.rs`.

## Database Schema

The database uses a hybrid relational/document schema, balancing queryable relational data with flexible JSON storage.

```sql
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
    id TEXT PRIMARY KEY,
    root_path TEXT NOT NULL,
    started_at TEXT NOT NULL,
    completed_at TEXT,
    projects_found INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'running'
);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS remote_projects (
    repo_slug TEXT PRIMARY KEY,
    preserve_patterns TEXT NOT NULL DEFAULT '[]'
);

CREATE TABLE IF NOT EXISTS project_logs (
    id           TEXT PRIMARY KEY,
    project_id   TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    command_name TEXT NOT NULL,
    exit_code    INTEGER,
    log_text     TEXT NOT NULL DEFAULT '',
    run_at       TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_projects_path ON projects(path);
CREATE INDEX IF NOT EXISTS idx_scans_started ON scans(started_at DESC);
CREATE INDEX IF NOT EXISTS idx_project_logs_project ON project_logs(project_id, run_at DESC);
```

## Concurrency

Rustodian uses `r2d2` for connection pooling and configures SQLite in Write-Ahead Logging (`WAL`) mode.

**Why WAL and r2d2?** WAL allows simultaneous readers and a single writer, crucial for the `rustodian-desktop` app. It ensures scanning threads safely read/write without locking the Slint UI thread's reads. A `busy_timeout = 5000` prevents transient failures if the writer lock is held.

## JSON Metadata Strategy

Instead of defining strict columns for every possible project attribute, flexible data is serialized into a single `metadata_json` column. The structure maps to: `{"meta": project.metadata, "vcs": project.vcs, "languages": project.languages}`.

**Why JSON?** Minimizes schema migrations as the domain model evolves. It avoids sparse tables with many NULL columns. Offloading structure to `serde_json` allows rapid iteration at the cost of slightly higher parsing overhead.

## Languages Side-Table

We maintain a `project_languages` side-table with an `ON DELETE CASCADE` foreign key reference to `projects`.

**Why a side-table?** Languages are also stored inside `metadata_json`, but extracting/filtering JSON data via `json_extract` scales poorly. The side-table guarantees fast relational querying for UI views. The `ON DELETE CASCADE` clause enables self-healing garbage collection by automatically deleting languages when a project is purged.

## Upserts & Data Syncing

When saving a project, the system uses an `ON CONFLICT(path) DO UPDATE` query.

**Why overwrite `discovered_at`?** It intentionally overwrites `discovered_at=excluded.discovered_at`. This sacrifices tracking the immutable "first-seen" timestamp but simplifies SQL by avoiding conditional merges, treating the latest scan as the source of truth.

Updating `project_languages` relies on a delete-and-reinsert pattern for a `project_id`.

**Why delete-and-reinsert?** Eliminates the need to calculate a complex diff in SQL, reducing logic complexity at the cost of write churn.

## Known Tradeoffs

- **Malformed Records Handling:** Bulk queries skip rows with corrupted JSON, logging a warning. This prevents queries from failing due to a single bad record, preserving UI stability.
- **Data Duplication:** Language data exists in `metadata_json` and `project_languages`. This increases storage footprint and requires syncing both locations during upserts.
- **WAL Writer Bottleneck:** WAL allows only one concurrent writer. Slow transactions can lock out other writes, causing failures on timeout.
- **Write Churn:** The delete-and-reinsert pattern increases WAL file size and write IOPS.
- **JSON Parsing Overhead:** Deserializing `metadata_json` incurs CPU cost on reads.
- **Timestamps:** Upserts overwrite `discovered_at`, sacrificing first-seen tracking.
