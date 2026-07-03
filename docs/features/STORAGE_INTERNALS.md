# Storage Internals

This document outlines the SQLite persistence layer implemented in `crates/rustodian-storage/src/store.rs`.

## Database Schema (ER-Style Description)

The database relies on a straightforward hybrid relational/document schema, structured as follows:

- `projects`: The core table storing discovered repositories.
  - `id` (TEXT PRIMARY KEY)
  - `path` (TEXT UNIQUE): The absolute path to the project.
  - `name` (TEXT)
  - `discovered_at` (TEXT)
  - `last_scanned_at` (TEXT)
  - `metadata_json` (TEXT): A JSON blob containing flexible data.
- `project_languages`: A side table for queryable language metrics.
  - `project_id` (TEXT, FK to projects.id)
  - `language` (TEXT)
  - `confidence` (TEXT)
- `scans`: Tracks directory scanning history.
  - `id` (TEXT PRIMARY KEY)
  - `started_at` (TEXT)
  - `completed_at` (TEXT)
  - `projects_found` (INTEGER)
  - `status` (TEXT)
- `settings`: Simple key-value store for app configuration.
  - `key` (TEXT PRIMARY KEY)
  - `value` (TEXT)

## Connection Pool and Concurrency

Rustodian utilizes the `r2d2` crate to manage a connection pool for SQLite (`r2d2::Pool<SqliteConnectionManager>`). The database is explicitly configured to use Write-Ahead Logging (`PRAGMA journal_mode = WAL`). WAL mode significantly improves concurrent access by allowing readers to operate simultaneously with a writer. This is critical for a desktop application architecture where background directory scans might write to the database concurrently with the user interface querying data.

To further ensure smooth concurrency, a `busy_timeout = 5000` is configured via PRAGMA. If the database is temporarily locked by a writer, the `busy_timeout` instructs SQLite to wait (up to 5 seconds) and retry before immediately returning a "database is locked" error. This prevents transient concurrency failures and ensures stable operations for the desktop app.

## Upserting Projects

Projects are inserted or updated using an upsert pattern via `ON CONFLICT(path) DO UPDATE`. The `path` field is treated as the unique identifier for a project on disk. When a conflict occurs (meaning the project was already discovered in a previous scan), the upsert command updates dynamic fields such as `name`, `last_scanned_at`, and `metadata_json`. Crucially, it excludes immutable fields like `id` and `discovered_at`, preserving the original first-seen timestamp and preventing ID churn that would break foreign key relations.

## JSON Metadata Strategy

Instead of maintaining a highly normalized schema with complex migrations for every new piece of metadata, Rustodian employs a hybrid approach. The `metadata_json` column stores a single JSON blob containing nested information like:
- Project metadata (`meta`)
- Version control system details (`vcs`)
- Language summaries (`languages`)

By packing this into JSON, the schema remains stable even as the domain model evolves. We avoid writing expensive and brittle schema migrations whenever a new metadata attribute is added or modified.

## The Project Languages Side Table

While language data is naturally nested within `metadata_json` for quick retrieval alongside the main project struct, there is also a dedicated `project_languages` side table. This table exists separately from the JSON blob to enable efficient, queryable filtering. Because standard SQLite's JSON querying capabilities can be less performant for indexing and aggregation, a dedicated relational table allows users to quickly filter, sort, and group projects by specific programming languages from the UI.

## Row Deserialization Strategy

To avoid duplicating boilerplate code, the extraction and deserialization of a database row into a `Project` struct is centralized in the `parse_project_row` function. This shared utility is called by various read operations, including `get_project`, `list_projects`, and `find_by_path`. It consistently handles the parsing of standard SQL columns and the deserialization of the `metadata_json` blob, ensuring a single source of truth for translating SQLite data into Rust structures.

## Known Tradeoffs

A notable tradeoff in the storage layer design is the handling of malformed records. If `parse_project_row` encounters invalid data (e.g., a corrupted JSON blob), it returns an error. However, functions like `list_projects` are designed to be resilient: rather than failing the entire query and breaking the UI if a single row is corrupted, malformed rows are skipped and a warning is logged (`tracing::warn!("Skipping malformed project row: {e}")`). This prioritizes application availability over strict data integrity for bulk read operations.
