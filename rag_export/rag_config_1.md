# RAG Export - Config (Part 1)

### Path: ./deny.toml
```
[advisories]
# Scope: "all" = entire dep graph, "workspace" = direct deps only
unmaintained = "workspace"
yanked = "deny"
ignore = [
    "RUSTSEC-2026-0194",
    "RUSTSEC-2026-0195",
]

[licenses]
allow = [
    "MIT",
    "MPL-2.0",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unicode-3.0",
    "Zlib",
    "BSL-1.0",
    "OFL-1.1",
    "Ubuntu-font-1.0",
    "CC0-1.0",
    "GPL-3.0-only",
    "LicenseRef-Slint-Royalty-free-2.0",
    "LicenseRef-Slint-Software-3.0",
    "Unlicense",
    "NCSA",
    "CDLA-Permissive-2.0",
]

[bans]
multiple-versions = "warn"
wildcards = "allow"

[sources]
unknown-registry = "deny"
unknown-git = "deny"
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
allow-git = []

```

### Path: ./clippy.toml
```
too-many-arguments-threshold = 8
type-complexity-threshold = 350

```

### Path: ./Cargo.toml
```
[workspace]
members = [
    "crates/rustodian-desktop","crates/*", "crates/rustodian-remote", "xtask"]
resolver = "3"

[workspace.package]
edition = "2024"
version = "0.1.0"
authors = ["drawmeanelephant"]
license = "MIT OR Apache-2.0"
repository = "https://github.com/drawmeanelephant/rustodian"
rust-version = "1.85"

[workspace.dependencies]
# Internal crates
rustodian-types = { path = "crates/rustodian-types" }
rustodian-core = { path = "crates/rustodian-core" }
rustodian-storage = { path = "crates/rustodian-storage" }
rustodian-scanner = { path = "crates/rustodian-scanner" }
rustodian-git = { path = "crates/rustodian-git" }

# CLI
clap = { version = "4.6", features = ["derive", "env"] }
comfy-table = "7.2"

# Error handling
anyhow = "1.0"
thiserror = "2.0"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1.0", features = ["v4", "serde"] }

# Database
rusqlite = { version = "0.32", features = ["bundled"] }
r2d2 = "0.8"
r2d2_sqlite = "0.25.0"

# Git
git2 = { version = "0.21", default-features = false, features = ["vendored-libgit2"] }

# Filesystem
ignore = "0.4"

# Dev/Test
tempfile = "3.27"
assert_cmd = "2.2"
predicates = "3.1"
insta = "1.48"

[workspace.lints.clippy]
pedantic = { level = "warn", priority = -1 }
# Allow these common pedantic lints
module_name_repetitions = "allow"
must_use_candidate = "allow"
missing_errors_doc = "allow"
missing_panics_doc = "allow"

```

### Path: ./xtask/Cargo.toml
```
[package]
name = "xtask"
description = "Workspace automation for Rustodian"
edition = "2024"
version = "0.1.0"
publish = false
license = "MIT OR Apache-2.0"
repository = "https://github.com/drawmeanelephant/rustodian"

[dependencies]
ignore.workspace = true
rustodian-core.workspace = true
rustodian-git.workspace = true

# xtask intentionally does not use workspace inheritance
# to keep it self-contained

```

### Path: ./crates/rustodian-types/Cargo.toml
```
[package]
name = "rustodian-types"
description = "Shared types and data structures for Rustodian"
edition.workspace = true
version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
chrono.workspace = true
uuid.workspace = true

[lints]
workspace = true

```

### Path: ./crates/rustodian-remote/Cargo.toml
```
[package]
name = "rustodian-remote"
version = "0.1.0"
edition = "2024"
license.workspace = true

[dependencies]
rustodian-types = { workspace = true }
rustodian-core = { workspace = true }
tokio = { version = "1.52", features = ["full"] }
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
flate2 = "1.0"
tar = "0.4"
globset = "0.4"
tracing = { workspace = true }
thiserror = { workspace = true }
anyhow = { workspace = true }
async-trait = "0.1"
serde = { workspace = true, features = ["derive"] }
chrono.workspace = true

[dev-dependencies]
mockito = "1.7.2"
tempfile.workspace = true

```

### Path: ./crates/rustodian-storage/Cargo.toml
```
[package]
name = "rustodian-storage"
description = "SQLite storage backend for Rustodian"
edition.workspace = true
version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
rustodian-types.workspace = true
rustodian-core.workspace = true
rusqlite.workspace = true
tracing.workspace = true
thiserror.workspace = true
serde.workspace = true
serde_json.workspace = true
chrono.workspace = true
uuid.workspace = true
r2d2.workspace = true
r2d2_sqlite.workspace = true

[dev-dependencies]
tempfile.workspace = true

[lints]
workspace = true

```

### Path: ./crates/rustodian-desktop/Cargo.toml
```
[package]
name = "rustodian-desktop"
description = "Desktop graphical interface for Rustodian"
edition.workspace = true
version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true

[[bin]]
name = "rustodian-desktop"
path = "src/main.rs"

[dependencies]
rustodian-types = { workspace = true }
rustodian-core = { workspace = true }
rustodian-storage = { workspace = true }
rustodian-scanner = { workspace = true }
rustodian-git = { workspace = true }
rustodian-remote = { path = "../rustodian-remote" }
anyhow = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
uuid = { workspace = true }
chrono = { workspace = true }
dirs = "6.0.0"
rfd = "0.17.2"

# High-octane UI integration replace eframe
slint = "1.9"
tokio = { version = "1.52.3", features = ["rt-multi-thread"] }

[build-dependencies]
slint-build = "1.9"

[lints]
workspace = true

[dev-dependencies]
insta = { workspace = true }

```

### Path: ./crates/rustodian-git/Cargo.toml
```
[package]
name = "rustodian-git"
description = "Git repository inspection for Rustodian"
edition.workspace = true
version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
rustodian-types.workspace = true
rustodian-core.workspace = true
git2.workspace = true
tracing.workspace = true
thiserror.workspace = true
chrono.workspace = true

[dev-dependencies]
anyhow.workspace = true
tempfile.workspace = true

[lints]
workspace = true

```

### Path: ./crates/rustodian-cli/Cargo.toml
```
[package]
name = "rustodian-cli"
description = "Command-line interface for Rustodian"
edition.workspace = true
version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true

[[bin]]
name = "rustodian"
path = "src/main.rs"

[dependencies]
rustodian-types.workspace = true
rustodian-core.workspace = true
rustodian-storage.workspace = true
rustodian-scanner.workspace = true
rustodian-git.workspace = true
clap.workspace = true
anyhow.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
comfy-table.workspace = true
serde_json.workspace = true
serde.workspace = true
rustodian-remote = { path = "../rustodian-remote" }
tokio = { version = "1.52", features = ["rt", "rt-multi-thread", "macros"] }
dirs = "6.0.0"

[dev-dependencies]
assert_cmd.workspace = true
predicates.workspace = true
tempfile.workspace = true

[lints]
workspace = true

```

### Path: ./crates/rustodian-core/Cargo.toml
```
[package]
name = "rustodian-core"
description = "Domain logic, traits, and orchestration for Rustodian"
edition.workspace = true
version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
async-trait = "0.1"
chrono.workspace = true
rustodian-types.workspace = true
shlex = "2.0.1"
thiserror.workspace = true
tracing.workspace = true
uuid.workspace = true

[target.'cfg(unix)'.dependencies]
nix = { version = "0.31.3", features = ["process", "signal"] }

[dev-dependencies]
tempfile.workspace = true

[lints]
workspace = true

```

### Path: ./crates/rustodian-scanner/Cargo.toml
```
[package]
name = "rustodian-scanner"
description = "Filesystem project discovery for Rustodian"
edition.workspace = true
version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
rustodian-types.workspace = true
rustodian-core.workspace = true
ignore.workspace = true
tracing.workspace = true
thiserror.workspace = true
toml = "1.1.2"
serde_json.workspace = true
globset = "0.4.18"

[dev-dependencies]
tempfile.workspace = true

[lints]
workspace = true

```

### Path: ./.editorconfig
```
root = true

[*]
end_of_line = lf
insert_final_newline = true
trim_trailing_whitespace = true
charset = utf-8

[*.rs]
indent_style = space
indent_size = 4

[*.toml]
indent_style = space
indent_size = 4

[*.{yml,yaml}]
indent_style = space
indent_size = 2

[*.md]
trim_trailing_whitespace = false

[Makefile]
indent_style = tab

```

### Path: ./justfile
```
# Rustodian Justfile — Developer convenience commands
# Usage: just <recipe>

set dotenv-load

# Default: run all checks
default: fmt clippy test

# Format all code
fmt:
    cargo fmt --all

# Check formatting (CI mode)
fmt-check:
    cargo fmt --all -- --check

# Run clippy lints
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Run all tests
test:
    cargo test --workspace

# Run tests with output
test-verbose:
    cargo test --workspace -- --nocapture

# Build all crates
build:
    cargo build --workspace

# Build in release mode
build-release:
    cargo build --workspace --release

# Run the CLI
run *ARGS:
    cargo run -p rustodian-cli -- {{ARGS}}

# Check documentation builds
doc:
    RUSTDOCFLAGS="-Dwarnings" cargo doc --workspace --no-deps

# Open documentation in browser
doc-open:
    cargo doc --workspace --no-deps --open

# Run cargo deny checks
deny:
    cargo deny check

# Run all CI checks locally
ci: fmt-check clippy test doc deny

# Clean build artifacts
clean:
    cargo clean

# Run xtask commands
xtask *ARGS:
    cargo run -p xtask -- {{ARGS}}

```

### Path: ./.gitignore
```
# Rust build artifacts
/target/
**/*.rs.bk

# IDE
.idea/
.vscode/
*.swp
*.swo
*~
.DS_Store

# Environment
.env
.env.local

# Database (local dev)
*.db
*.db-journal
*.db-wal
*.db-shm

# Coverage
lcov.info
tarpaulin-report.html

# OS
Thumbs.db
# rag_export/
*.log

# Generated log files
clippy.log
test.log

# Working/scratch docs
gemini.md
rag_*.md

```

### Path: ./rustfmt.toml
```
edition = "2024"
max_width = 100
use_field_init_shorthand = true
use_try_shorthand = true

```

### Path: ./cliff.toml
```
[changelog]
header = """# Changelog\n\nAll notable changes to Rustodian.\n"""
body = """
{% if version %}\
    ## [{{ version | trim_start_matches(pat="v") }}] - {{ timestamp | date(format="%Y-%m-%d") }}
{% else %}\
    ## [unreleased]
{% endif %}\
{% for group, commits in commits | group_by(attribute="group") %}
    ### {{ group | striptags | trim | upper_first }}
    {% for commit in commits %}
        - {% if commit.scope %}*({{ commit.scope }})* {% endif %}\
            {% if commit.breaking %}[**breaking**] {% endif %}\
            {{ commit.message | upper_first }}\
    {% endfor %}
{% endfor %}\n
"""
trim = true

[git]
conventional_commits = true
filter_unconventional = true
commit_parsers = [
    { message = "^feat", group = "Features" },
    { message = "^fix", group = "Bug Fixes" },
    { message = "^doc", group = "Documentation" },
    { message = "^perf", group = "Performance" },
    { message = "^refactor", group = "Refactor" },
    { message = "^style", group = "Styling" },
    { message = "^test", group = "Testing" },
    { message = "^chore", group = "Miscellaneous" },
    { message = "^ci", group = "CI" },
]
filter_commits = false
tag_pattern = "v[0-9]*"

```

### Path: ./.github/dependabot.yml
```
version: 2
updates:
  - package-ecosystem: cargo
    directory: /
    schedule:
      interval: weekly
    groups:
      rust-dependencies:
        patterns:
          - '*'
  - package-ecosystem: github-actions
    directory: /
    schedule:
      interval: weekly

```

### Path: ./.github/ISSUE_TEMPLATE/config.yml
```
blank_issues_enabled: true
contact_links:
  - name: Discussions
    url: https://github.com/drawmeanelephant/rustodian/discussions
    about: Ask questions and discuss ideas

```

### Path: ./.github/workflows/security-audit.yml
```
name: Security Audit

on:
  schedule:
    - cron: '0 6 * * 1'  # Every Monday at 6 AM UTC
  push:
    paths:
      - '**/Cargo.toml'
      - '**/Cargo.lock'

jobs:
  audit:
    name: Audit Dependencies
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: actions-rust-lang/audit@v1
        with:
          ignore: RUSTSEC-2026-0195,RUSTSEC-2026-0194,RUSTSEC-2026-0192

```

### Path: ./.github/workflows/release.yml
```
name: Release

on:
  push:
    tags:
      - 'v*'

permissions:
  contents: write

jobs:
  build:
    name: Build (${{ matrix.target }})
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-latest
          - target: aarch64-apple-darwin
            os: macos-latest
          - target: x86_64-pc-windows-msvc
            os: windows-latest
    steps:
      - uses: actions/checkout@v7
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - uses: Swatinem/rust-cache@v2
      - name: Build
        run: cargo build --release --target ${{ matrix.target }} -p rustodian-cli
      - name: Upload artifact
        uses: actions/upload-artifact@v7
        with:
          name: rustodian-${{ matrix.target }}
          path: |
            target/${{ matrix.target }}/release/rustodian-cli
            target/${{ matrix.target }}/release/rustodian-cli.exe

  release:
    name: Create Release
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
        with:
          fetch-depth: 0
      - name: Download artifacts
        uses: actions/download-artifact@v8
      - name: Create Release
        uses: softprops/action-gh-release@v3
        with:
          generate_release_notes: true
          files: |
            rustodian-*/rustodian-cli*

```

### Path: ./.github/workflows/ci.yml
```
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: -Dwarnings

jobs:
  fmt:
    name: Formatting
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --all -- --check

  clippy:
    name: Clippy
    runs-on: ubuntu-latest
    env:
      SLINT_BACKEND: headless
    steps:
      - uses: actions/checkout@v7
      - name: Install dependencies
        run: sudo apt update && sudo apt install -y pkg-config libfontconfig1-dev libx11-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libegl1-mesa-dev libwayland-dev
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --workspace --all-targets -- -D warnings

  test:
    name: Test (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    env:
      SLINT_BACKEND: headless
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
    steps:
      - uses: actions/checkout@v7
      - name: Install dependencies (Linux)
        if: matrix.os == 'ubuntu-latest'
        run: sudo apt update && sudo apt install -y pkg-config libfontconfig1-dev libx11-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libegl1-mesa-dev libwayland-dev
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --workspace

  doc:
    name: Documentation
    runs-on: ubuntu-latest
    env:
      SLINT_BACKEND: headless
    steps:
      - uses: actions/checkout@v7
      - name: Install dependencies
        run: sudo apt update && sudo apt install -y pkg-config libfontconfig1-dev libx11-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libegl1-mesa-dev libwayland-dev
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo doc --workspace --no-deps
        env:
          RUSTDOCFLAGS: -Dwarnings

  deny:
    name: Cargo Deny
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: EmbarkStudios/cargo-deny-action@v2

  rag_export:
    name: RAG Export
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v7
        with:
          ref: ${{ github.head_ref || github.ref_name }}
      - name: Install dependencies
        run: sudo apt update && sudo apt install -y pkg-config libfontconfig1-dev libx11-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libegl1-mesa-dev libwayland-dev
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo run -p xtask -- export-rag
      - uses: stefanzweifel/git-auto-commit-action@v7
        with:
          commit_message: "Auto-update RAG export"
          file_pattern: 'rag_export/*'

```

