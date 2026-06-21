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
