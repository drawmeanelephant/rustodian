# Scanner and Detection

## Directory Traversal (`ignore` vs `walkdir`)

Rustodian uses the `ignore` crate for filesystem traversal rather than `walkdir` to automatically respect `.gitignore` and `.ignore` rules. Without this, scanning would waste substantial I/O `stat` calls descending into large, irrelevant build artifact folders. For example, a single typical Node project might contain 30,000+ files deeply nested inside `node_modules/`. By automatically skipping such directories (and Rust's `target/`), `ignore` prevents traversing tens of thousands of generated files, avoiding severe performance bottlenecks and focusing discovery entirely on tracked source code.

## Recursion Limits

`ScanConfig.max_depth` limits recursion depth. Scanning deeply nested monorepos can cause prohibitively long execution times. Enforcing a maximum depth restricts scan time while still capturing typical project structures. A depth of `0` halts traversal immediately.

## Language Detection and Polyglot Projects

Language detection utilizes pure functions (like `detect_rust`) to evaluate directories for specific marker files. Detectors are evaluated independently. When a directory contains competing manifests (e.g., both `Cargo.toml` and `package.json`), Rustodian recognizes a polyglot project. It yields independent detections for all identified languages without reducing the confidence level of any of them.

### Markers and Confidence Rules

| Language | Marker File(s) | Confidence Rules |
|----------|----------------|------------------|
| **Rust** | `Cargo.toml`, `Cargo.lock` | **High:** `Cargo.toml` exists. **Medium:** Only `Cargo.lock` exists. |
| **Python**| `pyproject.toml`, `setup.py`, `setup.cfg`, `poetry.lock`, `Pipfile.lock`, `uv.lock`, `requirements.txt` | **High:** Manifest (`pyproject.toml`, `setup.py`, `setup.cfg`) exists. **Medium:** Only a lockfile or config file (`requirements.txt`) exists. |
| **Node** | `package.json`, `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`, `bun.lockb` | **High:** Any marker matched (always High confidence, even if only a lockfile exists). |
| **Go**   | `go.mod`, `go.sum` | **High:** Any marker matched (always High confidence, even if only a lockfile exists). |
| **Ruby** | `Gemfile`, `*.gemspec`, `Gemfile.lock` | **High:** `Gemfile` or `*.gemspec` exists. **Medium:** Only `Gemfile.lock` exists. |
| **Zig**  | `build.zig`, `build.zig.zon` | **High:** `build.zig` exists. **Medium:** Only `build.zig.zon` exists. |

## Detection Confidence Levels

The `DetectionConfidence` enum reflects evidence strength:

- **High:** A definitive manifest exists (e.g., `Cargo.toml`), indicating a project root. Note that for Node and Go, any detected marker (including lockfiles like `package-lock.json` or `go.sum`) uniquely defaults to High confidence.
- **Medium:** Supporting evidence exists, but isn't definitive. Examples include a `Cargo.lock` without a `Cargo.toml` (potentially a sub-crate) or a standalone `requirements.txt` config file.
- **Low:** Weak signals (e.g., only file extensions). Currently unused, reserved for future heuristics.

## Self-Healing Garbage Collection

Every scan (`Custodian::scan`) performs a self-healing garbage collection pass. If a tracked project's path no longer exists on disk, Rustodian purges it from the database.

Crucially, dependent tables handle cascading deletions. The `project_logs` table (which stores execution audit history) and the `project_languages` table define foreign keys referencing `projects(id)` with `ON DELETE CASCADE`. Therefore, when the scanner drops a missing project row, SQLite automatically cascade-deletes all of its associated historical execution logs and language mappings. This prevents orphaned records, keeping the schema clean and self-correcting without manual intervention.
