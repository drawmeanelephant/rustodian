<div align="center">

# 🏛️ Rustodian

### Department of Project Custodianship

*A personal project observatory that discovers, indexes, and monitors your software projects.*

[![CI](https://github.com/drawmeanelephant/rustodian/actions/workflows/ci.yml/badge.svg)](https://github.com/drawmeanelephant/rustodian/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)

</div>

---

## What is Rustodian?

Rustodian scans your development directories, detects software projects (Rust, Python, Node.js, Go), and maintains a searchable index of their metadata. Think of it as `ls` for your entire project portfolio.

```bash
# Scan your projects directory
rustodian scan ~/projects

# List all discovered projects
rustodian list

# Filter by language
rustodian list --language rust

# Get detailed info about a project
rustodian info my-awesome-project

# Observatory status
rustodian status

# Remote Project Tracking
rustodian remote add my-org/my-repo --preserve "config.json"
rustodian remote list
rustodian remote refresh --dest ~/projects

```

## Features

- 🔍 **Smart Discovery** — Walks directory trees respecting `.gitignore` rules
- 🦀 **Language Detection** — Identifies Rust, Python, Node.js, and Go projects via manifest files
- 🌿 **Git Integration** — Extracts branch, remote, dirty status, and last commit info
- 💾 **Local Storage** — SQLite database for fast queries with zero configuration
- 📊 **Multiple Outputs** — Table and JSON output formats
- 🧹 **Digital Janitor** — Reclaims disk space by purging workspace cruft (e.g., `target/`, `node_modules/`). Supports dry-run for inspection and purge mode.
- 🌐 **Remote Project Tracking** — Track and refresh repositories from remote sources like GitHub directly into your local workspace.


## Desktop GUI

Rustodian includes a desktop graphical interface built with `eframe`/`egui`. It features a project browser, command runner, a document viewer (for rendering `README.md`, `CHANGELOG.md`, `TODO.md`), and dedicated tabs for Details, Git Context, Tasks, and Runner Logs.

To run the desktop app:

```bash
cargo run -p rustodian-desktop
```

## Installation

### From Source

```bash
git clone https://github.com/drawmeanelephant/rustodian.git
cd rustodian
cargo install --path crates/rustodian-cli
```

### Requirements

- Rust 1.85+ (edition 2024)

## Environment Variables

Rustodian supports the following environment variables to configure its behavior:

- `RUSTODIAN_DB`: Specifies the absolute path to the SQLite database file. If not set, it defaults to `~/.local/share/rustodian/rustodian.db` (or the equivalent data directory for your OS).
- `RUSTODIAN_SCAN_ROOT`: Specifies the default root directory for the `scan` command if no path is provided.

Add the following to your `~/.bashrc` or `~/.zshrc` for reproducible setups:

```bash
export RUSTODIAN_DB="$HOME/.config/rustodian/rustodian.db"
export RUSTODIAN_SCAN_ROOT="$HOME/projects"
```

## Architecture

Rustodian is built as a Cargo workspace with strict crate boundaries:

| Crate | Purpose |
|-------|--------|
| `rustodian-types` | Shared data structures (zero behavior) |
| `rustodian-core` | Domain traits and orchestration |
| `rustodian-storage` | SQLite persistence |
| `rustodian-scanner` | Filesystem project discovery |
| `rustodian-git` | Git repository inspection |
| `rustodian-cli` | CLI entry point |

See [ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full dependency graph and boundary rules.

## Development

```bash
# Run all checks
just ci

# Or individually
just fmt          # Format code
just clippy       # Run lints
just test         # Run tests
just build        # Build all crates
just run scan .   # Run the CLI
cargo xtask export-rag # Export codebase to RAG-friendly markdown files
```

See [DEVELOPMENT.md](docs/DEVELOPMENT.md) for the full guide.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
