# Scanner and Detection

## Directory Traversal (`ignore` vs `walkdir`)

Rustodian uses the `ignore` crate for filesystem traversal rather than `walkdir` because `ignore` automatically respects `.gitignore` and `.ignore` rules. Without this, walking a directory would waste vast amounts of I/O stat calls descending into massive build artifact folders. For example, skipping a `node_modules/` or `target/` directory automatically prevents traversing tens of thousands of unnecessary files. This ensures rapid discovery focused solely on tracked source code, avoiding severe performance bottlenecks.

## Performance: `ScanConfig.max_depth`

The `ScanConfig.max_depth` configuration limits recursion depth. Scanning deep monorepos with hundreds of nested directories can be prohibitively slow. Enforcing a maximum depth restricts unbounded scan times while capturing typical project structures. A depth of 0 halts traversal entirely, returning empty results.

## Language Detection Pattern

Language detection uses pure functions in `detect_languages` (e.g., `detect_rust`). Each directory is examined for specific marker files. These detectors are evaluated independently. If a directory contains competing manifests, such as both `Cargo.toml` and `package.json`, it is recognized as a polyglot project. This yields independent, High-confidence detections for both Rust and Node, without reducing the confidence of either.

### Markers and Confidence Table

| Language | Marker File(s) | Confidence Rules |
|----------|----------------|------------------|
| **Rust** | `Cargo.toml`, `Cargo.lock` | **High:** `Cargo.toml` exists. **Medium:** Only `Cargo.lock` exists. |
| **Python**| `pyproject.toml`, `setup.py`, `setup.cfg`, `poetry.lock`, `Pipfile.lock`, `uv.lock`, `requirements.txt` | **High:** Manifest (`pyproject.toml`, `setup.py`, `setup.cfg`) exists. **Medium:** Only lockfile or `requirements.txt` config exists. |
| **Node** | `package.json`, `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`, `bun.lockb` | **High:** Any marker matched (always High, even with just lockfiles). |
| **Go**   | `go.mod`, `go.sum` | **High:** Any marker matched (always High, even with just lockfiles). |
| **Ruby** | `Gemfile`, `*.gemspec`, `Gemfile.lock` | **High:** `Gemfile` or `*.gemspec` exists. **Medium:** Only `Gemfile.lock` exists. |
| **Zig**  | `build.zig`, `build.zig.zon` | **High:** `build.zig` exists. **Medium:** Only `build.zig.zon` exists. |

## Detection Confidence Levels

The `DetectionConfidence` enum categorizes evidence strength:

- **High:** A definitive manifest file is present (e.g., `Cargo.toml`, `package.json`, `go.mod`), strongly indicating a project root. Note that Node and Go always yield High confidence, even if only lockfiles are found.
- **Medium:** Supporting evidence exists, but is not definitive. For instance, a `Cargo.lock` without a `Cargo.toml` might indicate a sub-crate, and a standalone `requirements.txt` might be a loosely tracked dependency list.
- **Low:** Weak signals (e.g., file extensions). Currently unused, but designed for future heuristics when only source files exist.

## Self-Healing Garbage Collection

During every scan (`Custodian::scan`), Rustodian performs a self-healing garbage collection pass. It checks all tracked projects; if a project's filesystem path no longer exists, it purges the project from the database.

Crucially, the `project_logs` table schema (which stores audit history like janitor actions) defines a foreign key `REFERENCES projects(id) ON DELETE CASCADE`. When the self-healing process deletes a purged project's row from the `projects` table, SQLite automatically cascade-deletes all associated audit logs. This guarantees that no orphaned log records remain, allowing the database to self-correct effortlessly without manual intervention.
