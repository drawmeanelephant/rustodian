# RAG Export - Content (Part 1)

### Path: ./README.md
```
<div align="center">

# 🏛️ Rustodian

### Department of Project Custodianship

*A personal project observatory that discovers, indexes, and monitors your software projects.*

[![CI](https://github.com/drawmeanelephant/rustodian/actions/workflows/ci.yml/badge.svg)](https://github.com/drawmeanelephant/rustodian/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)

</div>

---

## What is Rustodian?

Rustodian scans your development directories, detects software projects (Rust, Python, Node.js, Go), and maintains a searchable index of their metadata. Think of it as `ls` for your entire project portfolio.

```bash
# Scan your projects directory
rustodian scan ~/projects

# List all discovered projects
rustodian list

# Filter by language
rustodian list --language rust

# Get detailed info about a project
rustodian info my-awesome-project

# Observatory status
rustodian status

# Remote Project Tracking
rustodian remote add my-org/my-repo --preserve "config.json"
rustodian remote list
rustodian remote refresh --dest ~/projects

```

## Features

- 🔍 **Smart Discovery** — Walks directory trees respecting `.gitignore` rules
- 🦀 **Language Detection** — Identifies Rust, Python, Node.js, and Go projects via manifest files
- 🌿 **Git Integration** — Extracts branch, remote, dirty status, and last commit info
- 💾 **Local Storage** — SQLite database for fast queries with zero configuration
- 📊 **Multiple Outputs** — Table and JSON output formats
- 🧹 **Digital Janitor** — Reclaims disk space by purging workspace cruft (e.g., `target/`, `node_modules/`). Supports dry-run for inspection and purge mode.
- 🌐 **Remote Project Tracking** — Track and refresh repositories from remote sources like GitHub directly into your local workspace.


## Desktop GUI

Rustodian includes a desktop graphical interface built with **Slint**. It features a project browser, command runner, a document viewer (for rendering `README.md`, `CHANGELOG.md`, `TODO.md`), and dedicated tabs for Ingest, Export, Explorer, Logs, and Docs.

To run the desktop app:

```bash
cargo run -p rustodian-desktop
```

## Installation

### From Source

```bash
git clone https://github.com/drawmeanelephant/rustodian.git
cd rustodian
cargo install --path crates/rustodian-cli
```

### Requirements

- Rust 1.85+ (edition 2024)

## Environment Variables

Rustodian supports the following environment variables to configure its behavior:

- `RUSTODIAN_DB`: Specifies the absolute path to the SQLite database file. If not set, it defaults to `~/.local/share/rustodian/rustodian.db` (or the equivalent data directory for your OS).
- `RUSTODIAN_SCAN_ROOT`: Specifies the default root directory for the `scan` command if no path is provided.

Add the following to your `~/.bashrc` or `~/.zshrc` for reproducible setups:

```bash
export RUSTODIAN_DB="$HOME/.config/rustodian/rustodian.db"
export RUSTODIAN_SCAN_ROOT="$HOME/projects"
```

## Architecture

Rustodian is built as a Cargo workspace with strict crate boundaries:

| Crate | Purpose |
|-------|--------|
| `rustodian-types` | Shared data structures (zero behavior) |
| `rustodian-core` | Domain traits and orchestration |
| `rustodian-storage` | SQLite persistence |
| `rustodian-scanner` | Filesystem project discovery |
| `rustodian-git` | Git repository inspection |
| `rustodian-cli` | CLI entry point |

See [ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full dependency graph and boundary rules.

## Development

```bash
# Run all checks
just ci

# Or individually
just fmt          # Format code
just clippy       # Run lints
just test         # Run tests
just build        # Build all crates
just run scan .   # Run the CLI
cargo xtask export-rag # Export codebase to RAG-friendly markdown files
```

See [DEVELOPMENT.md](docs/DEVELOPMENT.md) for the full guide.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.

```

### Path: ./DEVLOG.md
```
# Rustodian Devlog 🦀🏛️

> Running log of the Rustodian build — what we did, what we decided, and why.
> This file lives in the repo so progress is always visible.

---

## Session 1 — 2026-06-21

### 11:21 AM — Project Kickoff

**Starting point**: Empty directory. Clean slate.

**The brief**: Build the *architecture*, not the features. The goal is a production-quality scaffold that feels like version 0.4.0, not a weekend hack. The founding prompt was carefully shaped to prioritize structure over implementation — no GUI, no web server, no plugins yet. Just the skeleton.

**Core identity**: *Rustodian: Department of Project Custodianship* — a personal project observatory that discovers software projects on disk, indexes metadata, and provides a unified query interface.

**MVP scope** (ruthlessly defined):
- Scan project directories
- Detect: Rust, Python, Node, Go
- Store metadata in SQLite
- Commands: `scan`, `list`, `status`, `info`
- Comprehensive tests + CI

---

### 11:21 AM — Research Phase

Kicked off parallel research across the Rust ecosystem:

| Area | Finding |
|------|---------|
| **Rust stable** | 1.96.0 (May 2026), edition 2024 |
| **Workspace pattern** | Flat `crates/` layout, virtual manifest, `[workspace.dependencies]` |
| **CI** | `dtolnay/rust-toolchain@stable` + `Swatinem/rust-cache@v2` |
| **SQLite** | `rusqlite` 0.40 over `sqlx` 0.9 for CLI tools |
| **Git** | `git2` 0.21 with vendored libgit2 |
| **FS walking** | `ignore` 0.4 (gitignore-aware, from ripgrep) |

---

### 11:22 AM — Key Decision: `rusqlite` over `sqlx`

The original prompt suggested `sqlx` + `tokio`. Research and architecture review both pushed back hard.

| Factor | `sqlx` | `rusqlite` |
|--------|--------|------------|
| API | Async-first | Synchronous |
| Runtime needed | Tokio required | None |
| Best for | Web backends | CLI tools |
| Complexity | Medium | Low |
| MSRV | 1.94.0 (!) | Moderate |

**Decision**: `rusqlite` with `bundled` feature. **No `tokio`.**

> *"Why are we carrying an async runtime around like a grand piano?"* — the question every CLI architect should ask.

---

### 11:22 AM — Key Decision: `ignore` over `walkdir`

The `ignore` crate (by BurntSushi / ripgrep) respects `.gitignore` automatically. Developer directories are full of `node_modules/`, `target/`, `.venv/` — `ignore` skips all of that out of the box.

---

### 11:23 AM — Architecture Plan v1

Created the first implementation plan with 6 crates + xtask. Posed 4 open questions for review.

---

### 11:41 AM — Architecture Review (Round 1)

Got external review feedback. All four questions resolved:

| Question | Decision | Rationale |
|----------|----------|-----------|
| Projects crate | **Merged into core** | Domain too small to justify a separate crate |
| Justfile + xtask | **Both** | `just` = developer convenience, `xtask` = project automation |
| License | **MIT OR Apache-2.0** | Standard Rust dual-license |
| rusqlite | **Confirmed** | No async before there's a concrete reason |

**New architectural refinements from review:**

#### The Generic Hydra 🐉
Reviewer flagged `Custodian<S, Sc, G>` as over-engineering. Switched to `Box<dyn Trait>` — dynamic dispatch costs "approximately one molecule of CPU" when every call does disk I/O.

#### Dependency Boundary Rules
Created explicit "what can / must never depend on what" rules per crate. Infrastructure crates (storage, scanner, git) **never depend on each other**.

#### Future Crate Extraction Plan
Identified healthy future crates: `search`, `graph`, `todo-indexer`, `desktop-ui`, `plugins`.
Banned cursed crates: ~~`common`~~, ~~`shared`~~, ~~`utils`~~, ~~`helpers`~~ 🌿☠️

---

### 12:12 PM — Rust Toolchain Setup

Found `rustup` installed via Homebrew at `/opt/homebrew/bin/rustup` but `~/.cargo/bin` didn't exist (Homebrew's rustup doesn't create proxy symlinks). Created symlinks manually. **Rust 1.96.0** confirmed.

---

### 12:15 PM — Scaffold Generation

Deployed 4 parallel subagents to generate files simultaneously:
1. **GitHub/CI** — 9 files (workflows, templates, dependabot, CODEOWNERS)
2. **Root configs** — 10 files (Cargo.toml, licenses, justfile, deny.toml, etc.)
3. **Rust source** — 6 crates + xtask (~30 files)
4. **Documentation** — 5 files (README, ARCHITECTURE, DEVELOPMENT, TESTING, migrations)

Subagents 1, 2, 4 completed. Subagent 3 (source files) hit a rate limit after creating `rustodian-types` and `rustodian-core`. Finished the remaining 4 crates manually.

---

### 5:25 PM — Build & Fix

First `cargo check --workspace`:
- **Error**: `rusqlite::Connection` is not `Sync` (uses `RefCell` internally), but `ProjectStore` requires `Send + Sync`.
  - **Fix**: Wrapped `Connection` in `Mutex<Connection>`. For a single-threaded CLI tool, zero contention overhead.
- **Error**: `clap` missing `env` feature for `#[arg(env = "RUSTODIAN_DB")]`.
  - **Fix**: Added `"env"` to clap features in workspace dependencies.
- **Warning**: Unused `ScanStatus` import in custodian.rs.
  - **Fix**: Removed.

Second `cargo check --workspace`: ✅ Clean (only expected dead_code warning for stubbed fields).

---

### 5:25 PM — Tests Pass

```
test result: ok. 8 passed; 0 failed; 0 ignored
```

- 6 language detection tests (Rust, Python, Node, Go, multi-language, empty dir)
- 2 SQLite migration tests (open in-memory, idempotent migrations)

---

### 5:27 PM — Pushed to GitHub

**61 files, 3,860 lines** committed and pushed to [github.com/drawmeanelephant/rustodian](https://github.com/drawmeanelephant/rustodian).

CI is running on GitHub Actions. 🤞

---

## Session Summary

### What We Built
A production-quality Cargo workspace scaffold with:
- 6 crates + xtask with strict dependency boundaries
- Domain types, trait contracts, and `Box<dyn Trait>` orchestrator
- SQLite schema with migration tracking
- Working language detection (4 languages, 6 tests)
- Full CI/CD pipeline (3 workflows)
- GitHub templates, dependabot, CODEOWNERS
- Documentation (ARCHITECTURE, DEVELOPMENT, TESTING)
- Conventional commits, git-cliff changelog, cargo-deny

### What We Didn't Build (On Purpose)
- Scanner logic (stubbed with `todo!()`)
- Git inspection logic (stubbed)
- SQLite CRUD operations (stubbed)
- CLI command handlers (stubbed)
- Any GUI, web server, or plugin system

### Architectural Decisions Made
1. `rusqlite` over `sqlx` — no async runtime for a CLI tool
2. `ignore` over `walkdir` — gitignore-aware by default
3. `Box<dyn Trait>` over generics — simplicity over zero-cost abstraction
4. `Mutex<Connection>` for thread safety on the store
5. `clap` with `env` feature for config via environment variables
6. Edition 2024 with `resolver = "3"`

### What's Next
- Implement `FsScanner` (walk dirs with `ignore` crate)
- Implement `Git2Inspector` (extract branch/remote/dirty/commit)
- Implement `SqliteStore` CRUD operations
- Wire up CLI command handlers
- Add integration tests with `assert_cmd`

---

*More entries will be added as we build.* 🦀🏛️📂

```

### Path: ./LICENSE-APACHE
```
                              Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.

Copyright (c) 2026 drawmeanelephant

```

### Path: ./docs/features/STORAGE_INTERNALS.md
```
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

**Why overwrite `discovered_at`?** The upsert logic intentionally overwrites `discovered_at` (`discovered_at=excluded.discovered_at`). While this sacrifices the ability to track the immutable "first-seen" timestamp of a project, it drastically simplifies the SQL queries by avoiding complex conditional merges.

Updating the `project_languages` side-table relies on a simple delete-and-reinsert pattern for a given `project_id`.

**Why delete-and-reinsert?** It eliminates the need for calculating a complex diff (inserting new, updating existing, removing deleted languages) in SQL, heavily reducing logic complexity at the cost of increased write churn.

## Known Tradeoffs

- **Malformed Records Handling:** Bulk queries (like `list_projects`) are designed to be resilient. If a single row contains corrupted JSON, the system skips it, logs a warning, and continues. This tradeoff prevents an entire database query from failing due to one bad record, maintaining stability in the desktop UI.
- **WAL Writer Bottleneck:** WAL only allows one concurrent writer. Heavy or slow transactions can lock out other write operations, causing failures if the 5000ms timeout is exceeded.
- **Write Churn:** The delete-and-reinsert synchronization pattern for `project_languages` increases the size of the WAL file and write IOPS due to unnecessary row deletion and recreation.
- **JSON Parsing Overhead:** Bypassing relational schema for `metadata_json` incurs a continuous CPU cost on every database read to deserialize records back into `Project` structs.
- **Timestamps:** Upserts overwrite `discovered_at`, sacrificing first-seen tracking for simpler queries.

```

### Path: ./docs/features/REMOTE_TRACKING.md
```
# Remote Repository Tracking

This document outlines the remote repository tracking features implemented in the `rustodian-remote` crate, specifically focusing on `GithubDownloader` in `crates/rustodian-remote/src/downloader.rs`.

## Pull Requests
The `PullRequestFetcher` trait defines the interface for fetching open PRs. `GithubDownloader` implements this trait, fetching PR metadata (number, title, author, branch, url, update time, and draft status) from the GitHub API.

In the desktop UI (`rustodian-desktop`), the Slint UI interacts with the async `PullRequestFetcher` trait via background thread messaging. When the UI dispatches a message over the worker channel, the background worker creates a short-lived, local `Tokio` runtime to bridge the synchronous event loop and the async PR fetching logic without blocking the main thread.

## GithubDownloader Flow
When downloading an archive, `GithubDownloader` requests the `main` branch tarball (`/archive/refs/heads/main.tar.gz`). If it receives a `404 Not Found`, it automatically falls back to `master` (`/archive/refs/heads/master.tar.gz`), ensuring compatibility with both new and legacy branch naming conventions.

## Zip Slip and Path Traversal Protections
Extracting untrusted archives carries "Zip Slip" risks, where malicious entries use path traversal (`../`) or symlinks to overwrite files outside the intended directory.

The downloader implements strict protections:
1. **Component Verification:** Extraction is rejected if any path component is not a normal file/directory or the current directory (`.`). `..` components trigger an immediate security error.
2. **Prefix Stripping:** Top-level archive directories are discarded via component iterator manipulation (`strip_prefix`) to prevent unnecessary nesting.
3. **Canonicalization Checks:** It uses `canonicalize` on the target directory parent of each entry, strictly validating that the resolved extraction path begins exactly with the intended extraction root.
4. **Symlink Mitigation:** If an archive contains a symlink pointing outside the root and a subsequent entry attempts to write to it, the canonicalization check intercepts the operation and aborts extraction, preventing arbitrary file overwrites.

## Preserve Patterns
To prevent overwriting local configurations or files when refreshing an archive, the downloader supports a `preserve_patterns` glob mechanism.

During extraction, each archive entry's stripped path is matched against a compiled `globset`. If an entry matches a preserve pattern (e.g., `config.json`, `*.local`), it is safely skipped, leaving the local file intact.

## Rate Limit Handling
When fetching PRs, `GithubDownloader` monitors the HTTP response. A `403 Forbidden` with an `X-RateLimit-Remaining` header of `"0"` is mapped to `CoreError::RateLimitExceeded`. This enables upper layers to handle rate limits gracefully.

## Example CLI Usage
You can use the `rustodian` CLI to manage remote repositories. Here is a realistic end-to-end example: adding a project with a preserve pattern, listing tracked projects, and refreshing the repository.

```bash
$ rustodian remote add octocat/Hello-World --preserve "config.json"
Added remote project: octocat/Hello-World

$ rustodian remote list
+---------------------+-------------------+
| Repo Slug           | Preserve Patterns |
+=========================================+
| octocat/Hello-World | config.json       |
+---------------------+-------------------+

$ rustodian remote refresh --dest ./my_remotes
Refreshing octocat/Hello-World...
Successfully refreshed octocat/Hello-World
Scanning project octocat/Hello-World...
Scan completed. Found 1 projects.
```

```

### Path: ./docs/features/DIGITAL_JANITOR.md
```
# Digital Janitor

The Digital Janitor is an autonomous workspace cruft purger that inspects tracked projects for bloated build artifacts and temporary directories, calculates reclaimable bytes, and optionally purges them.

## Cruft Targets

The Janitor targets specific well-known artifact directories that are generally safe to remove because they can be easily reconstructed by standard build tools.

| Target Directory | Description                       | Why it's safe to delete                                       | Typical Size Impact |
|------------------|-----------------------------------|---------------------------------------------------------------|---------------------|
| `target`         | Rust build directory              | Rebuilt on next `cargo build`                                 | Very High (1GB+)    |
| `node_modules`   | Node.js / JavaScript packages     | Reinstalled on next `npm install` or `yarn`                   | High (500MB+)       |
| `.venv`          | Python virtual environment        | Can be recreated via `python -m venv .venv` and `pip install` | Medium (100MB+)     |
| `.gopath`        | Go workspace (Rustodian-isolated) | Dependencies fetched again by `go build` or `go mod download` | Medium (100MB+)     |
| `.next`          | Next.js build output              | Regenerated on `next build` or `next dev`                     | Medium (100MB+)     |
| `dist`           | Generic build output              | Rebuilt by the project's build tool                           | Low (10MB+)         |
| `build`          | Generic build output              | Rebuilt by the project's build tool                           | Low (10MB+)         |
| `__pycache__`    | Python bytecode cache             | Automatically regenerated by Python at runtime                | Very Low (<5MB)     |

## Dry-run vs Purge

By default, the Janitor operates in **dry-run** mode, calculating potential space savings via a recursive directory walk (`dirsize`) without deleting anything. On deep or file-heavy directories, this calculation may take a noticeable amount of time. To execute the actual deletion, you must explicitly provide the `--purge` flag.

Every successful purge operation is fully auditable. The Janitor logs the event to the `project_logs` database via `Custodian::store.save_log()`, recording the command (`janitor:clean`), targets removed, and total space reclaimed. You can query this history using `rustodian logs my-project`.

## Worked Example

Suppose you have a project with a stale Rust `target/` directory taking up about 850 MB.

**Dry-run inspection (default):**
```bash
$ rustodian janitor example-rust-app
+--------------+-----------------------+-----------+
| Cruft Target | Status                | Bytes     |
+==============+=======================+===========+
| target       | Reclaimable (Dry Run) |           |
+--------------+-----------------------+-----------+
| Total        | Reclaimable (Dry Run) | 892341020 |
+--------------+-----------------------+-----------+
```

**Actual Purge operation:**
```bash
$ rustodian janitor example-rust-app --purge
+--------------+-----------+-----------+
| Cruft Target | Status    | Bytes     |
+==============+===========+===========+
| target       | Reclaimed |           |
+--------------+-----------+-----------+
| Total        | Reclaimed | 892341020 |
+--------------+-----------+-----------+
```

## Gotchas

* **Permission Denied Errors:** If the Janitor encounters a permissions error and fails to remove a directory during a `--purge` operation (via `fs::remove_dir_all`), it warns via standard logging. However, it currently **still** includes the full directory size in the reported `bytes_reclaimed` and the database audit log.

```

### Path: ./docs/features/SCANNER_DETECTION.md
```
# Scanner and Detection

## Directory Traversal (`ignore` vs `walkdir`)

Rustodian uses the `ignore` crate for filesystem traversal rather than `walkdir` to respect `.gitignore` and `.ignore` rules automatically. Without this, scanning would waste substantial I/O stat calls descending into large, irrelevant build artifact folders. By automatically skipping directories like `node_modules/` (Node) or `target/` (Rust), `ignore` prevents traversing tens of thousands of generated files. This avoids severe performance bottlenecks, focusing discovery entirely on tracked source code.

## Recursion Limits

`ScanConfig.max_depth` limits recursion depth. Scanning deeply nested monorepos can cause prohibitively long execution times. Enforcing a maximum depth restricts scan time while still capturing typical project structures. A depth of `0` halts traversal immediately.

## Language Detection and Polyglot Projects

Language detection utilizes pure functions (like `detect_rust`) to evaluate directories for specific marker files. Detectors are evaluated independently. When a directory contains competing manifests (e.g., both `Cargo.toml` and `package.json`), Rustodian recognizes a polyglot project. It yields independent, High-confidence detections for both languages, without reducing the confidence level of either.

### Markers and Confidence Rules

| Language | Marker File(s) | Confidence Rules |
|----------|----------------|------------------|
| **Rust** | `Cargo.toml`, `Cargo.lock` | **High:** `Cargo.toml` exists. **Medium:** Only `Cargo.lock` exists. |
| **Python**| `pyproject.toml`, `setup.py`, `setup.cfg`, `poetry.lock`, `Pipfile.lock`, `uv.lock`, `requirements.txt` | **High:** Manifest (`pyproject.toml`, `setup.py`, `setup.cfg`) exists. **Medium:** Only a lockfile or `requirements.txt` exists. |
| **Node** | `package.json`, `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`, `bun.lockb` | **High:** Any marker matched (always High, even with just lockfiles). |
| **Go**   | `go.mod`, `go.sum` | **High:** Any marker matched (always High, even with just lockfiles). |
| **Ruby** | `Gemfile`, `*.gemspec`, `Gemfile.lock` | **High:** `Gemfile` or `*.gemspec` exists. **Medium:** Only `Gemfile.lock` exists. |
| **Zig**  | `build.zig`, `build.zig.zon` | **High:** `build.zig` exists. **Medium:** Only `build.zig.zon` exists. |

## Detection Confidence Levels

The `DetectionConfidence` enum reflects evidence strength:

- **High:** A definitive manifest exists (e.g., `Cargo.toml`, `package.json`), indicating a project root. For Node and Go, any marker (even just a lockfile) yields High confidence.
- **Medium:** Supporting evidence exists, but isn't definitive. Examples include a `Cargo.lock` without a `Cargo.toml` (potentially a sub-crate) or a standalone `requirements.txt`.
- **Low:** Weak signals (e.g., only file extensions). Currently unused, reserved for future heuristics.

## Self-Healing Garbage Collection

Every scan (`Custodian::scan`) performs a self-healing garbage collection pass. If a tracked project's path no longer exists on disk, Rustodian purges it from the database.

Crucially, dependent tables handle cascading deletions. The `project_logs` (which stores audit history) and `project_languages` tables define foreign keys referencing `projects(id)` with `ON DELETE CASCADE`. When the scan drops a missing project, SQLite automatically cascade-deletes all its associated languages and audit logs. This prevents orphaned records, keeping the schema clean and self-correcting without manual intervention.

```

### Path: ./docs/features/BOOTSTRAP_ISOLATION.md
```
# Environment Isolation in Rustodian

When Rustodian bootstraps and verifies projects, it isolates operations to prevent host system pollution, ensuring reproducible environments across codebases.

## Language Command Mapping

| Language | Isolation Mechanism | Setup Command | Verify Command |
| --- | --- | --- | --- |
| Rust | No isolation (native `target/`) | `cargo build` | `cargo test` |
| Node | Local `node_modules` directory | `npm install` [^1] | `npm test` [^1] |
| Go | `GOPATH` env var override to `.gopath` | `go mod download` | `go test ./...` |
| Python | Virtual Env (`.venv`) directory | `pip install -r requirements.txt`<br>and/or `pip install .` [^2] | `pytest -v`<br>(fallback: `python -m unittest discover`) [^3] |

[^1]: Node projects dynamically detect lockfiles to substitute `npm` with the correct package manager (`yarn`, `pnpm`, or `bun`).
[^2]: Python installs dependencies sequentially. It runs the `requirements.txt` install if the file exists, and then the `.` install if `pyproject.toml` or `setup.py` exists. The executable path differs by OS (Unix: `.venv/bin/pip`, Windows: `.venv\Scripts\pip`).
[^3]: The `pytest` executable is used if present in `.venv`, otherwise falls back to `python -m unittest discover`. The executable path differs by OS (Unix: `.venv/bin/`, Windows: `.venv\Scripts\`).

## Isolation Strategies

**Rust:** Builds natively in the `target/` directory with no additional isolation.

**Node.js:** Dependencies are localized in `node_modules`. Rustodian detects lockfiles (`yarn.lock`, `pnpm-lock.yaml`, `bun.lockb`) to select the package manager, with `npm` as the fallback.

**Go:** Rustodian overrides the global `GOPATH` environment variable to a project-local `.gopath` directory, protecting the global module cache.

**Python:** Rustodian uses a Virtual Environment (`.venv`):
1. Attempts creation via `python3 -m venv .venv`, falling back to `python -m venv .venv`.
2. Installs dependencies sequentially: runs `pip install -r requirements.txt` if `requirements.txt` exists; runs `pip install .` if `pyproject.toml` or `setup.py` exists (using `.venv/bin/pip` on Unix or `.venv\Scripts\pip` on Windows).
3. Verifies via local `pytest -v` if the executable exists, falling back to `python -m unittest discover` (using `.venv/bin/` on Unix or `.venv\Scripts\` on Windows).

## Example: Mixed-Language Monorepo

```text
my-monorepo/
├── frontend/ (Node)
│   ├── pnpm-lock.yaml
│   └── package.json
└── backend/ (Python)
    ├── pyproject.toml
    ├── requirements.txt
    └── main.py
```

When Rustodian scans this directory:
1. **Frontend:** It detects `pnpm-lock.yaml`, isolating dependencies in `frontend/node_modules/` via `pnpm install`, and verifies using `pnpm test`.
2. **Backend:** It creates `backend/.venv`. On Unix, it installs sequentially running `backend/.venv/bin/pip install -r requirements.txt` followed by `backend/.venv/bin/pip install .`. It verifies with `backend/.venv/bin/pytest -v` (or `unittest`). Windows uses `backend\.venv\Scripts\pip` and `backend\.venv\Scripts\pytest`.

Neither project affects the host system's global state or each other.

```

### Path: ./docs/TESTING.md
```
# Testing Strategy

## Unit Tests

Each crate has inline `#[cfg(test)]` modules. Run with:

```bash
cargo test --workspace
```

## Integration Tests

CLI integration tests use `assert_cmd` and `predicates`:

```bash
cargo test -p rustodian-cli
```

## Test Fixtures

Tests that need project directories use `tempfile::TempDir` to create
isolated fixture directories with specific marker files.

## Snapshot Testing

`insta` is available for snapshot testing of complex outputs:

```bash
# Run tests and review snapshots
cargo insta test --workspace
cargo insta review
```

## Coverage

```bash
cargo xtask coverage
# Or directly:
cargo tarpaulin --workspace --out html
```

## What to Test

| Crate | Focus |
|-------|-------|
| types | Serialization roundtrips |
| core | Custodian orchestration with mocks |
| storage | Migration idempotency, CRUD operations |
| scanner | Language detection, directory walking |
| git | Git info extraction from fixture repos |
| cli | End-to-end command testing |

```

### Path: ./docs/ARCHITECTURE.md
```
# Architecture

Rustodian is a Cargo workspace with 8 library/binary crates organized for clean separation of concerns.

## Crate Dependency Graph

```
       rustodian-cli (binary)          rustodian-desktop (binary)
                 |                                 |
                 +-----------------+---------------+
                                   |
    rustodian-remote   rustodian-storage   rustodian-scanner   rustodian-git
             \                 |                   |                  /
              \                |                   |                 /
               \               +-------------------+----------------+
                \                                  |
                 +-------------------------- rustodian-core (traits)
                                                   |
                                            rustodian-types (data)

    xtask (automation) --------------------> rustodian-core, rustodian-git
```

## Boundary Rules

These are the constitutional rules. Violations should fail code review.

### rustodian-types
- **Is**: Pure data structures, enums, newtypes
- **Depends on**: serde, chrono, uuid (serialization only)
- **Never depends on**: Any infrastructure crate

### rustodian-core
- **Is**: Trait definitions + Custodian orchestrator
- **Depends on**: rustodian-types, thiserror, tracing
- **Never depends on**: rusqlite, git2, ignore, clap

### rustodian-storage
- **Is**: ProjectStore implementation (SQLite)
- **Depends on**: rustodian-types, rustodian-core, rusqlite
- **Never depends on**: git2, ignore, clap

### rustodian-scanner
- **Is**: ProjectScanner implementation (filesystem)
- **Depends on**: rustodian-types, rustodian-core, ignore
- **Never depends on**: rusqlite, git2, clap

### rustodian-git
- **Is**: GitInspector implementation (libgit2)
- **Depends on**: rustodian-types, rustodian-core, git2
- **Never depends on**: rusqlite, ignore, clap

### rustodian-cli
- **Is**: Composition root, CLI entry point
- **Depends on**: Everything (it wires implementations together)
- **Nobody depends on**: cli

### rustodian-desktop
- **Is**: Desktop GUI application and composition root
- **Depends on**: Everything (it wires implementations together for the GUI)
- **Nobody depends on**: desktop

### rustodian-remote
- **Is**: Remote repository fetcher (e.g., GitHub)
- **Depends on**: rustodian-types, rustodian-core, tokio, reqwest
- **Never depends on**: storage, scanner, git, clap

### xtask
- **Is**: Workspace automation tasks (e.g., `export-rag`)
- **Depends on**: rustodian-core, rustodian-git, ignore
- **Nobody depends on**: xtask


## Key Invariant

Infrastructure crates (storage, scanner, git) **never depend on each other**.
They only know about types (data) and core (contracts).
The CLI is the only place where they meet.

## Dynamic Dispatch

The `Custodian` orchestrator uses `Box<dyn Trait>` instead of generics:

```rust
pub struct Custodian {
    store: Box<dyn ProjectStore>,
    scanner: Box<dyn ProjectScanner>,
    git: Box<dyn GitInspector>,
}
```

Rationale: A CLI tool that waits on filesystem I/O and SQLite gains nothing from monomorphization. Dynamic dispatch costs one vtable lookup per call — irrelevant when each call does disk I/O.

## Desktop UI Note

The desktop application (`rustodian-desktop`) includes a project browser, command runner, doc viewer, and several tabs built with **Slint UI**. The **Pull Requests** tab is fully operational, displaying PR numbers, authors, branches, draft flags, and auto-populated repo slugs.

## Future Extension Points

| Feature | How It Fits |
|---------|------------|
| Plugin system | New trait + crate for plugin loading |
| Code search | New crate implementing a SearchIndex trait |
| Dependency graphs | New crate implementing a DependencyAnalyzer trait |

```

### Path: ./docs/DEVELOPMENT.md
```
# Development Guide

## Prerequisites

- **Rust**: 1.85+ (install via [rustup](https://rustup.rs))
- **just**: Task runner (install via `cargo install just` or `brew install just`)

## Quick Start

```bash
git clone https://github.com/drawmeanelephant/rustodian.git
cd rustodian

# Run all checks
just ci

# Build and run
just run --help
just run scan ~/projects
```

## Common Tasks

| Command | Description |
|---------|------------|
| `just fmt` | Format all code |
| `just clippy` | Run clippy lints |
| `just test` | Run all tests |
| `just test-verbose` | Run tests with output |
| `just build` | Build all crates |
| `just doc-open` | Build and open docs |
| `just ci` | Run full CI locally |
| `just run <args>` | Run the CLI |

## Adding a New Language Detector

1. Open `crates/rustodian-scanner/src/detection.rs`
2. Add a new `detect_<language>` function following the existing pattern
3. Register it in the `detect_languages` function
4. Add the language variant to `Language` enum in `crates/rustodian-types/src/language.rs`
5. Add tests

## Adding a New CLI Command

1. Create `crates/rustodian-cli/src/commands/<name>.rs`
2. Add to `crates/rustodian-cli/src/commands/mod.rs`
3. Add the subcommand variant to `Commands` enum in `main.rs`
4. Wire up in the match block

## Adding a New Crate

1. Create `crates/rustodian-<name>/`
2. Add `Cargo.toml` using workspace inheritance
3. The workspace auto-discovers via `members = ["crates/*"]`
4. Update `docs/ARCHITECTURE.md` with new boundary rules

## Conventions

- **Commits**: Use [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `docs:`, etc.)
- **Errors**: Use `thiserror` in libraries, `anyhow` in the CLI binary
- **Logging**: Use `tracing` macros (`info!`, `debug!`, `warn!`)
- **Testing**: Unit tests in the same file, integration tests in `tests/`

```

### Path: ./LICENSE-MIT
```
MIT License

Copyright (c) 2026 drawmeanelephant

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

```

### Path: ./.github/PULL_REQUEST_TEMPLATE.md
```
## Summary

Brief description of changes.

## Type of Change

- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation update
- [ ] Refactor
- [ ] CI/Build

## Checklist

- [ ] `cargo fmt --all` passes
- [ ] `cargo clippy --workspace` passes
- [ ] `cargo test --workspace` passes
- [ ] Documentation updated (if applicable)
- [ ] No new warnings introduced

## Related Issues

Closes #

```

### Path: ./.github/ISSUE_TEMPLATE/bug_report.md
```
---
name: Bug Report
about: Report a bug in Rustodian
title: '[BUG] '
labels: bug
assignees: ''
---

## Description
A clear description of the bug.

## Steps to Reproduce
1. Run `rustodian ...`
2. ...

## Expected Behavior
What you expected to happen.

## Actual Behavior
What actually happened.

## Environment
- OS: 
- Rustodian version: 
- Rust version: 

## Additional Context
Any other context, logs, or screenshots.

```

### Path: ./.github/ISSUE_TEMPLATE/feature_request.md
```
---
name: Feature Request
about: Suggest a feature for Rustodian
title: '[FEATURE] '
labels: enhancement
assignees: ''
---

## Problem
What problem does this solve?

## Proposed Solution
How should it work?

## Alternatives Considered
Any other approaches you considered.

## Additional Context
Any other context or references.

```

