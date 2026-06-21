# Architecture

Rustodian is a Cargo workspace with 6 library/binary crates organized for clean separation of concerns.

## Crate Dependency Graph

```
                    rustodian-cli (binary)
                   /       |        \
                  /        |         \
    rustodian-storage  rustodian-scanner  rustodian-git
                  \        |         /
                   \       |        /
                    rustodian-core (traits)
                          |
                    rustodian-types (data)
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

## Future Extension Points

| Feature | How It Fits |
|---------|------------|
| Desktop UI | New binary crate parallel to cli, consuming the same core |
| Plugin system | New trait + crate for plugin loading |
| Remote repos | New crate with async + tokio (only crate that needs it) |
| Code search | New crate implementing a SearchIndex trait |
| Dependency graphs | New crate implementing a DependencyAnalyzer trait |
