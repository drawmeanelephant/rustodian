# Development Guide

## Prerequisites

- **Rust**: 1.85+ (install via [rustup](https://rustup.rs))
- **just**: Task runner (install via `cargo install just` or `brew install just`)

## Quick Start

```bash
git clone https://github.com/drawmeanelephant/rustodian.git
cd rustodian

# Run all checks
just ci

# Build and run
just run --help
just run scan ~/projects
```

## Common Tasks

| Command | Description |
|---------|------------|
| `just fmt` | Format all code |
| `just clippy` | Run clippy lints |
| `just test` | Run all tests |
| `just test-verbose` | Run tests with output |
| `just build` | Build all crates |
| `just doc-open` | Build and open docs |
| `just ci` | Run full CI locally |
| `just run <args>` | Run the CLI |

## Adding a New Language Detector

1. Open `crates/rustodian-scanner/src/detection.rs`
2. Add a new `detect_<language>` function following the existing pattern
3. Register it in the `detect_languages` function
4. Add the language variant to `Language` enum in `crates/rustodian-types/src/language.rs`
5. Add tests

## Adding a New CLI Command

1. Create `crates/rustodian-cli/src/commands/<name>.rs`
2. Add to `crates/rustodian-cli/src/commands/mod.rs`
3. Add the subcommand variant to `Commands` enum in `main.rs`
4. Wire up in the match block

## Adding a New Crate

1. Create `crates/rustodian-<name>/`
2. Add `Cargo.toml` using workspace inheritance
3. The workspace auto-discovers via `members = ["crates/*"]`
4. Update `docs/ARCHITECTURE.md` with new boundary rules

## Conventions

- **Commits**: Use [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `docs:`, etc.)
- **Errors**: Use `thiserror` in libraries, `anyhow` in the CLI binary
- **Logging**: Use `tracing` macros (`info!`, `debug!`, `warn!`)
- **Testing**: Unit tests in the same file, integration tests in `tests/`
