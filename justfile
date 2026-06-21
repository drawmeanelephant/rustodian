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
