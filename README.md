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
```

## Features

- 🔍 **Smart Discovery** — Walks directory trees respecting `.gitignore` rules
- 🦀 **Language Detection** — Identifies Rust, Python, Node.js, and Go projects via manifest files
- 🌿 **Git Integration** — Extracts branch, remote, dirty status, and last commit info
- 💾 **Local Storage** — SQLite database for fast queries with zero configuration
- 📊 **Multiple Outputs** — Table and JSON output formats

## Installation

### From Source

```bash
git clone https://github.com/drawmeanelephant/rustodian.git
cd rustodian
cargo install --path crates/rustodian-cli
```

### Requirements

- Rust 1.85+ (edition 2024)

## Using with LLMs / RAG Context

Rustodian provides a built-in xtask to export your entire codebase into AI-friendly markdown files, perfect for RAG (Retrieval-Augmented Generation) or large context windows.

Follow these 3 simple steps to generate context:

1. **Set your paths** (navigate to the project you want to export).
2. **Run `just run scan .`** to build the initial project index.
3. **Run `cargo xtask export-rag`** to compile context assets into the `/rag_export` folder.

**Sample Prompt Snippet:**

```markdown
I have exported the relevant context of my codebase.
Please review the files in the `rag_export` directory (provided below)
and help me implement [Feature X / Fix Bug Y].

<Paste contents of rag_export files here>
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
```

See [DEVELOPMENT.md](docs/DEVELOPMENT.md) for the full guide.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
