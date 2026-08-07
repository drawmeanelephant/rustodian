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

## Command Discovery and Precedence

`CommandDiscoverer` collects runnable commands from up to four sources per project:

1. `.rustodian.toml` (`[commands]` table)
2. `justfile` / `Justfile` recipes
3. `package.json` scripts
4. generated language defaults (e.g. `cargo test`/`cargo build` when `Cargo.toml` exists)

Exactly one `ProjectCommand` exists per command name: when the same name is provided by several sources, the **highest-priority source wins** (1 over 2 over 3 over 4). So if all four define `test`, only the `.rustodian.toml` definition survives, while unique names from every source remain available. After resolution, commands are returned sorted **alphabetically by name**, so discovery output is deterministic across runs.

## Self-Healing Garbage Collection

Every scan (`Custodian::scan`) performs a self-healing garbage collection pass. If a tracked project's path no longer exists on disk, Rustodian purges it from the database.

Crucially, dependent tables handle cascading deletions. The `project_logs` (which stores audit history) and `project_languages` tables define foreign keys referencing `projects(id)` with `ON DELETE CASCADE`. When the scan drops a missing project, SQLite automatically cascade-deletes all its associated languages and audit logs. This prevents orphaned records, keeping the schema clean and self-correcting without manual intervention.
