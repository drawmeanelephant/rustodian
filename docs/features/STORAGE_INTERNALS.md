# Storage Internals

This document outlines the SQLite persistence layer implemented in `crates/rustodian-storage/src/store.rs`.

## Database Schema

The database relies on a straightforward hybrid relational/document schema. This structure balances queryable relational data with flexible schema-less JSON storage.

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

CREATE INDEX idx_projects_path ON projects(path);
CREATE INDEX idx_scans_started ON scans(started_at DESC);
CREATE INDEX idx_project_logs_project ON project_logs(project_id, run_at DESC);
```

## Concurrency

Rustodian uses `r2d2` for connection pooling and configures SQLite in Write-Ahead Logging (`WAL`) mode.

**Why WAL and r2d2?** WAL allows simultaneous readers and a single writer, which is crucial for the `rustodian-desktop` application. It ensures background scanning threads can safely read and write to the database without locking up the Slint UI thread's reads. A `busy_timeout = 5000` is also employed to prevent transient failure if the single writer lock is temporarily held.

## JSON Metadata Strategy

Instead of defining strict columns for every possible project attribute, flexible data is serialized into a single `metadata_json` column. The structure maps to: `{"meta": project.metadata, "vcs": project.vcs, "languages": project.languages}`.

**Why JSON?** This minimizes schema migrations as the domain model evolves. By offloading complex structure to `serde_json`, we gain rapid iteration speed for Rust structs at the cost of slightly higher parsing overhead during reads.

## Languages Side-Table

We maintain a `project_languages` side-table with an `ON DELETE CASCADE` foreign key reference to `projects`.

**Why a side-table?** While languages are also stored inside `metadata_json`, extracting and filtering on JSON data via SQLite's `json_extract` scales poorly over thousands of rows. The side-table guarantees fast relational querying and filtering for the desktop UI views.

## Upserts & Data Syncing

When saving a project, the system uses an `ON CONFLICT(path) DO UPDATE` query to upsert records.

**Why preserve `discovered_at`?** The upsert deliberately leaves `discovered_at` out of the `DO UPDATE SET` clause, so it is only ever written on the initial insert. Repeated saves of an existing path therefore keep the original "first-seen" timestamp: SQLite's `ON CONFLICT(path) DO UPDATE` preserves the column's existing value when it is not assigned, requiring no application-side read-before-write.

Updating the `project_languages` side-table relies on a simple delete-and-reinsert pattern for a given `project_id`.

**Why delete-and-reinsert?** It eliminates the need for calculating a complex diff (inserting new, updating existing, removing deleted languages) in SQL, heavily reducing logic complexity at the cost of increased write churn.

## Known Tradeoffs

- **Malformed Records Handling:** Bulk queries (like `list_projects`) are designed to be resilient. If a single row contains corrupted JSON, the system skips it, logs a warning, and continues. This tradeoff prevents an entire database query from failing due to one bad record, maintaining stability in the desktop UI.
- **WAL Writer Bottleneck:** WAL only allows one concurrent writer. Heavy or slow transactions can lock out other write operations, causing failures if the 5000ms timeout is exceeded.
- **Write Churn:** The delete-and-reinsert synchronization pattern for `project_languages` increases the size of the WAL file and write IOPS due to unnecessary row deletion and recreation.
- **JSON Parsing Overhead:** Bypassing relational schema for `metadata_json` incurs a continuous CPU cost on every database read to deserialize records back into `Project` structs.
- **Timestamps:** `discovered_at` is set once on insert and preserved on subsequent upserts, providing stable first-seen tracking for each project path.
