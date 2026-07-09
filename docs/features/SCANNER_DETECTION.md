# Scanner and Detection

## Directory Traversal (`ignore` vs `walkdir`)

Rustodian uses the `ignore` crate for filesystem traversal rather than `walkdir` to automatically respect `.gitignore` and `.ignore` rules. Standard traversal requires manual rule parsing or exhaustive tree visiting. By honoring native ignore files out-of-the-box, Rustodian avoids descending into massive, irrelevant artifact directories. For example, if a `.gitignore` contains `node_modules/`, `ignore` silently skips it, saving thousands of wasteful I/O stat calls. This ensures traversal focuses exclusively on tracked source files and mitigates severe performance bottlenecks.

## Recursion Limits

`ScanConfig.max_depth` limits recursion depth. Unbounded traversal of deep monorepos can trigger prohibitive execution times. Enforcing a strict depth bounds scan duration while capturing standard project hierarchies. A depth of `0` halts traversal immediately.

## Language Detection and Polyglot Projects

Language detection evaluates directories independently using pure functions (e.g., `detect_rust`) that scan for known marker files. If a directory contains competing manifests (e.g., both `Cargo.toml` and `package.json`), Rustodian identifies a polyglot project. It yields independent detections for both languages at full confidence, without diluting the `DetectionConfidence` for either.

### Markers and Confidence Rules

| Language | Marker File(s) | Confidence Rules |
|----------|----------------|------------------|
| **Rust** | `Cargo.toml`, `Cargo.lock` | **High:** `Cargo.toml` exists. **Medium:** Only `Cargo.lock` exists. |
| **Python**| `pyproject.toml`, `setup.py`, `setup.cfg`, `poetry.lock`, `Pipfile.lock`, `uv.lock`, `requirements.txt` | **High:** Manifest (`pyproject.toml`, `setup.py`, `setup.cfg`) exists. **Medium:** Only lockfiles or `requirements.txt` exist. |
| **Node** | `package.json`, `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`, `bun.lockb` | **High:** Any marker matched. |
| **Go**   | `go.mod`, `go.sum` | **High:** Any marker matched. |
| **Ruby** | `Gemfile`, `*.gemspec`, `Gemfile.lock` | **High:** `Gemfile` or `*.gemspec` exists. **Medium:** Only `Gemfile.lock` exists. |
| **Zig**  | `build.zig`, `build.zig.zon` | **High:** `build.zig` exists. **Medium:** Only `build.zig.zon` exists. |

## Detection Confidence Levels

The `DetectionConfidence` enum maps evidence strength:

- **High:** A definitive manifest exists (e.g., `Cargo.toml`, `package.json`), anchoring a project root. For Node and Go, discovering just a lockfile provides High confidence.
- **Medium:** Supporting evidence exists without a definitive manifest. Examples include a lone `Cargo.lock` (a potential sub-crate) or a standalone `requirements.txt`.
- **Low:** Weak signals (e.g., extensions). Currently reserved for future heuristics.

## Self-Healing Garbage Collection

Every scan (`Custodian::scan`) performs a self-healing garbage collection pass. If a tracked project's path no longer exists on disk, Rustodian purges it from the datastore.

This purge naturally propagates via SQLite `ON DELETE CASCADE` foreign keys attached to the `project_languages` and `project_logs` tables. Thus, when the core `projects` record is deleted, all dependent language metadata, audit history, and execution logs are cascade-deleted simultaneously. This ensures no orphaned logs remain, keeping the relational schema perfectly clean without manual bookkeeping.
