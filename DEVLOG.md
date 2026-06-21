# Rustodian Devlog 🦀🏛️

> Running log of the Rustodian build — what we did, what we decided, and why.
> This file lives in the repo so progress is always visible.

---

## Session 1 — 2026-06-21

### 11:21 AM — Project Kickoff

**Starting point**: Empty repo at `github.com/drawmeanelephant/rustodian`. Clean slate.

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

A CLI tool that walks filesystems and writes local SQLite has zero need for async. If remote APIs appear later, async gets added surgically to those crates only.

---

### 11:22 AM — Key Decision: `ignore` over `walkdir`

The `ignore` crate (by BurntSushi / ripgrep) respects `.gitignore` automatically. Developer directories are full of `node_modules/`, `target/`, `.venv/` — `ignore` skips all of that out of the box. No-brainer.

---

### 11:23 AM — Architecture Plan v1

Created the first implementation plan. Crate structure:

```
rustodian-types     → shared serializable data types
rustodian-core      → domain logic, traits, orchestration
rustodian-storage   → SQLite persistence (rusqlite)
rustodian-scanner   → filesystem project discovery (ignore)
rustodian-git       → git repo inspection (git2)
rustodian-cli       → CLI entry point (clap)
xtask               → workspace automation
```

**Open questions posed**:
1. Merge `rustodian-projects` into `rustodian-core`?
2. Justfile + xtask combo?
3. License?
4. Confirm `rusqlite` over `sqlx`?

---

### 11:41 AM — Architecture Review (Round 1)

Got external review feedback. All four questions resolved:

| Question | Decision | Rationale |
|----------|----------|-----------|
| Projects crate | **Merged into core** | Domain too small to justify a separate crate. Can extract later if it grows. |
| Justfile + xtask | **Both** | `just` = developer convenience, `xtask` = project automation |
| License | **MIT OR Apache-2.0** | Standard Rust dual-license, removes friction |
| rusqlite | **Confirmed** | "I'd be suspicious if the architecture included async before there was a concrete reason" |

**New feedback items to address**:

#### The Generic Hydra Problem 🐉
Reviewer flagged `Custodian<S, Sc, G>` as a potential over-Rust-ification. For a CLI tool that spends its life waiting on the filesystem, dynamic dispatch costs "approximately one molecule of CPU."

**Decision**: Switch to `Box<dyn Trait>`:
```rust
struct Custodian {
    storage: Box<dyn ProjectStore>,
    scanner: Box<dyn ProjectScanner>,
    git: Box<dyn GitInspector>,
}
```
Simpler, more readable, still testable with mocks. Generics only if profiling later demands it.

#### Dependency Graph Rules
Reviewer asked for explicit "what can / must never depend on what" rules. Creating a formal dependency boundary document.

#### Future Crate Extraction Plan
Reviewer asked: "If Rustodian reaches 50k LOC, what splits first?"

Good answers: `search`, `graph`, `todo-indexer`, `desktop-ui`, `plugins`
Bad answers: `common`, `shared`, `utils`, `helpers` (the cursed crates 🌿☠️)

---

### 11:41 AM — GitHub Repo Created

Repo live at: https://github.com/drawmeanelephant/rustodian

Devlog moved to repo root so it's always visible. Architecture plan v2 in progress incorporating all review feedback.

---

### Architecture Plan v2 — In Progress

Incorporating:
- [x] Resolved all open questions
- [x] `Box<dyn Trait>` over generics for Custodian
- [x] Explicit dependency boundary rules
- [x] Future crate extraction plan
- [ ] Generate scaffold code
- [ ] Push to GitHub
- [ ] Verify CI passes

---

*More entries will be added as we build.* 🦀🏛️📂
