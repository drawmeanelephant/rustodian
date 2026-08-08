# Scanner and Detection

## Directory Traversal (`ignore` vs `walkdir`)

Rustodian uses the `ignore` crate for filesystem traversal rather than `walkdir` to respect `.gitignore` and `.ignore` rules automatically. Without this, scanning would waste substantial I/O stat calls descending into large, irrelevant build artifact folders. By automatically skipping directories like `node_modules/` (Node) or `target/` (Rust), `ignore` prevents traversing tens of thousands of generated files. This avoids severe performance bottlenecks, focusing discovery entirely on tracked source code.

## Recursion Limits

`ScanConfig.max_depth` limits recursion depth. Scanning deeply nested monorepos can cause prohibitively long execution times. Enforcing a maximum depth restricts scan time while still capturing typical project structures. A depth of `0` halts traversal immediately.

## Language Detection and Polyglot Projects

Language detection utilizes pure functions (like `detect_rust`) to evaluate directories for specific marker files. Detectors are evaluated independently. When a directory contains competing manifests (e.g., both `Cargo.toml` and `package.json`), Rustodian recognizes a polyglot project. It yields independent, High-confidence detections for both languages, without reducing the confidence level of either.

## Project-Root Markers (Cloudflare Wrangler)

A directory counts as a project when it has language evidence **or** project-root evidence. Project-root markers are tracked separately from language detection: a marker identifies a project or deployment root without making any claim about the implementation language.

Rustodian recognizes these Cloudflare Wrangler configuration files as project-root evidence:

- `wrangler.jsonc`
- `wrangler.json`
- `wrangler.toml`

Wrangler is a deployment tool, not a programming language — its presence never produces a `Language` detection. The file's contents are never parsed; the existence of the config file alone is sufficient, so even a malformed `wrangler.jsonc` marks the directory as a project root.

The table below summarizes the semantics:

| Directory contents | Discovered? | Language detection |
|--------------------|-------------|--------------------|
| `wrangler.jsonc` only | Yes | None — no language is claimed |
| `wrangler.jsonc` + `package.json` | Yes | Node, detected normally (High) |
| `wrangler.toml` + `pyproject.toml` | Yes | Python, detected normally (High) |
| `package.json` without Wrangler | Yes | Node, unchanged |
| empty directory | No | — |

For a Wrangler-only project, the `languages` list stays empty (the implementation language is unknown and Rustodian does not invent one). The platform is captured in `ProjectMetadata.extra["platform"]` as `"cloudflare-wrangler"`, using the extensible metadata bag — no schema changes required.

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

## Stale Records and Explicit Pruning

Scans are purely additive: `Custodian::scan` discovers and updates projects but never deletes a tracked project merely because its filesystem path no longer exists. For an observatory, temporarily unavailable disks, mounts, or directories must never erase project history or command logs, so there is no automatic garbage-collection pass.

Stale *database* records — tracked projects whose stored paths no longer exist — are removed explicitly with the `prune` command:

```bash
rustodian prune            # dry run: list stale records, mutate nothing
rustodian prune --purge    # delete stale database records
rustodian prune --format json
```

`prune` defaults to a dry run that prints the project name, ID, and path without mutating the database. `--purge` deletes only the database rows for projects whose paths are currently missing. Dependent tables handle cascading deletions: the `project_logs` (audit history) and `project_languages` tables define foreign keys referencing `projects(id)` with `ON DELETE CASCADE`, so SQLite automatically removes the associated records. `prune` never touches the filesystem.
