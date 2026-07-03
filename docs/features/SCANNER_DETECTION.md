# Scanner and Detection

## Directory Traversal (`ignore` vs `walkdir`)

Rustodian uses the `ignore` crate instead of `walkdir` for filesystem traversal. This is crucial because `ignore` automatically respects `.gitignore` and `.ignore` rules out of the box. By doing so, the scanner effortlessly skips massive, irrelevant generated directories. For example, without `ignore`, the scanner would waste significant I/O and processing time descending into deeply nested `node_modules`, `target`, or `.venv` folders, parsing thousands of build artifacts. This design choice prevents performance bottlenecks and ensures discovery focuses exclusively on tracked source code without requiring manual exclusion lists.

## Performance: `ScanConfig.max_depth`

The `ScanConfig.max_depth` setting bounds traversal recursion depth. In deep monorepos with hundreds of nested directories, scanning the entire tree can be prohibitively slow. Limiting maximum depth prevents unbounded scan times while reliably capturing typical project structures. A depth of 0 halts traversal entirely, returning empty results.

## Language Detection Pattern

Language detection is handled by pure functions in `detect_languages` (e.g., `detect_rust`, `detect_python`). Each directory is examined for specific markers to identify its primary language(s). These detectors run independently, meaning a single directory containing both `Cargo.toml` and `package.json` will be correctly recognized as a polyglot project, yielding independent detections for both languages without reducing either's confidence.

### Markers and Confidence Table

| Language | Marker File(s) | Confidence Rules |
|----------|----------------|------------------|
| **Rust** | `Cargo.toml`, `Cargo.lock` | **High:** `Cargo.toml` exists. **Medium:** Only `Cargo.lock` exists. |
| **Python**| `pyproject.toml`, `setup.py`, `setup.cfg`, `poetry.lock`, `Pipfile.lock`, `uv.lock`, `requirements.txt` | **High:** Manifest (`pyproject.toml`, `setup.py`, `setup.cfg`) exists. **Medium:** Only lock/config exists. |
| **Node** | `package.json`, `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`, `bun.lockb` | **High:** Always High if any marker matched (including lockfiles only). |
| **Go**   | `go.mod`, `go.sum` | **High:** Always High if any marker matched. |
| **Ruby** | `Gemfile`, `*.gemspec`, `Gemfile.lock` | **High:** `Gemfile` or `*.gemspec` exists. **Medium:** Only `Gemfile.lock` exists.|
| **Zig**  | `build.zig`, `build.zig.zon` | **High:** `build.zig` exists. **Medium:** Only `build.zig.zon` exists. |

## Detection Confidence Levels

The `DetectionConfidence` enum categorizes the strength of evidence:

- **High:** A definitive manifest file is present (e.g., `Cargo.toml`, `package.json`, `go.mod`). This strongly indicates the directory is a project root. Note that for Node and Go, confidence is always high even if only lockfiles are found.
- **Medium:** Supporting evidence exists, but is not definitive. Finding a `Cargo.lock` without a `Cargo.toml` might indicate a sub-crate, while a standalone `requirements.txt` might just be a loosely tracked dependency list.
- **Low:** Reserved for weak signals (like file extensions). Currently unused by primary detectors, but designed for future heuristics such as when only source files are present without standard manifests.

## Self-Healing Garbage Collection

During every scan (`Custodian::scan`), Rustodian performs a self-healing garbage collection pass to keep the database synchronized with the filesystem. It iterates over all tracked projects; if a project's path no longer exists on disk, it is purged from the database.

Because the `project_logs` table (which stores audit history like janitor runs) is defined with a foreign key featuring `ON DELETE CASCADE` referencing the `projects` table, removing the purged project's row automatically deletes all associated historical logs. This ensures no orphaned log records are left behind. Running this process implicitly during every scan allows the database to seamlessly self-correct without requiring a manual cleanup step.
