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

The desktop application (`rustodian-desktop`) includes a project browser, command runner, doc viewer, and several tabs. Note that the **Pull Requests** tab in the desktop app is currently a placeholder and is not yet implemented.

## Future Extension Points

| Feature | How It Fits |
|---------|------------|
| Plugin system | New trait + crate for plugin loading |
| Code search | New crate implementing a SearchIndex trait |
| Dependency graphs | New crate implementing a DependencyAnalyzer trait |
