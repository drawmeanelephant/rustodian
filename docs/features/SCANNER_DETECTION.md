## Directory Traversal (`ignore` vs `walkdir`)

Rustodian uses the `ignore` crate rather than `walkdir` for filesystem traversal. This is crucial because `ignore` automatically respects `.gitignore` rules. Without this, the scanner would waste significant I/O and processing time descending into massive generated directories like `node_modules`, `target`, or `.venv`. This design choice avoids the need to maintain hardcoded manual exclusion lists, ensuring discovery focuses only on tracked source code.

## Performance: `ScanConfig.max_depth`

The `ScanConfig.max_depth` setting bounds the recursion depth of the traversal. In deep monorepos with hundreds of nested directories, traversing the entire tree can be prohibitively slow. By limiting the max depth, we avoid unbounded scan times while still capturing typical project structures. A depth of 0 halts traversal entirely, returning empty results.

## Language Detection Pattern

Language detection is handled by pure functions in `detect_languages` (e.g., `detect_rust`, `detect_python`). Each directory is examined for specific markers to identify its primary language(s). These detectors run independently, allowing a single directory to be recognized as a polyglot project.

### Markers and Confidence Table

| Language | Marker File(s) | Confidence Rules |
|----------|----------------|------------------|
| **Rust** | `Cargo.toml`, `Cargo.lock` | **High:** `Cargo.toml` exists. **Medium:** Only `Cargo.lock` exists. |
| **Python**| `pyproject.toml`, `setup.py`, `setup.cfg`, `poetry.lock`, `Pipfile.lock`, `uv.lock`, `requirements.txt` | **High:** Manifest (`pyproject.toml`, `setup.py`, `setup.cfg`) exists. **Medium:** Only lock/config exists. |
| **Node** | `package.json`, `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`, `bun.lockb` | **High:** Always High if any marker matched. |
| **Go**   | `go.mod`, `go.sum` | **High:** Always High if any marker matched. |
| **Ruby** | `Gemfile`, `*.gemspec`, `Gemfile.lock` | **High:** `Gemfile` or `*.gemspec` exists. **Medium:** Only `Gemfile.lock` exists.|
| **Zig**  | `build.zig`, `build.zig.zon` | **High:** `build.zig` exists. **Medium:** Only `build.zig.zon` exists. |

## Detection Confidence Levels

The `DetectionConfidence` enum categorizes the strength of evidence:

- **High:** A definitive manifest file is present (e.g., `Cargo.toml`, `package.json`, `go.mod`). This strongly indicates the directory is the root of a project.
- **Medium:** Supporting evidence exists, but it's not definitive. For example, finding a `Cargo.lock` without a `Cargo.toml` might indicate a sub-crate, or a `requirements.txt` might just be a loosely tracked list of dependencies rather than a full project structure.
- **Low:** Weak signals like just file extensions (though currently not utilized in the primary detectors, the type exists for future heuristics where multiple competing manifest files or only source files might provide low confidence).

## Self-Healing Garbage Collection

During every scan (`Custodian::scan`), Rustodian performs a self-healing garbage collection pass. It iterates through all tracked projects in the database. If a tracked project's path no longer exists on disk, it is purged from the database.

This runs on every scan, rather than as a separate command, because a primary goal of a scan is to synchronize the database with the reality of the filesystem. By piggybacking on the scan operation, the database seamlessly self-corrects when projects are moved or deleted, ensuring the index remains accurate without requiring the user to run a separate manual cleanup step.
