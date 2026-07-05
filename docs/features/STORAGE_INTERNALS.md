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

CREATE INDEX idx_projects_path ON projects(path);
CREATE INDEX idx_scans_started ON scans(started_at DESC);
CREATE INDEX idx_project_logs_project ON project_logs(project_id, run_at DESC);
```

## Concurrency

Rustodian uses `r2d2` for pooling and Write-Ahead Logging (`WAL`), aiding concurrent access. This prevents locks in the desktop app during background scans. A `busy_timeout = 5000` prevents transient failures if locked.

## Upserts

Projects use `ON CONFLICT(path) DO UPDATE`. Notably, `discovered_at` is updated during upserts (`discovered_at=excluded.discovered_at`). `project_languages` uses delete-and-reinsert to sync cleanly.

## JSON Metadata Strategy

Instead of strict columns, `metadata_json` stores a blob structured as: `{"meta": project.metadata, "vcs": project.vcs, "languages": project.languages}`.

**Why JSON?** Keeps the schema stable and avoids migrations when domains evolve, allowing rapid struct development via `serde_json`.

## Languages Side Table

**Why a side-table?** Purely for read performance. SQLite's `json_extract` is slow over thousands of rows; this table guarantees fast filtering for desktop views.

## Deserialization

`parse_project_row` centralizes translation to `Project` structs to avoid boilerplate.

## Tradeoffs

- **Malformed Records**: Invalid JSON skips the row rather than failing the query, prioritizing UI stability.
- **Timestamps**: Upserts overwrite `discovered_at`, sacrificing first-seen tracking for simpler queries.
- **WAL Limits**: WAL is bottlenecked by a single writer; writes over 5000ms will lock the DB.
- **Write Churn**: Delete-and-reinsert for languages increases WAL size for simpler logic.
- **JSON Parsing**: Using JSON avoids migrations but incurs CPU cost on every read due to `serde_json` deserialization.
