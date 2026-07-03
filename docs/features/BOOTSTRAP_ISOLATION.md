# Environment Isolation in Rustodian

## Why Isolation Matters
When Rustodian bootstraps and verifies projects, it ensures that operations do not leak into or pollute the host system's global environment. This is critical for maintaining a clean development environment across heterogeneous projects containing Rust, Node, Go, and Python codebases. By isolating dependencies and tools locally, Rustodian avoids version conflicts and unintended side-effects.

## Rust
Rust provides excellent out-of-the-box isolation via Cargo. No special environment redirection is needed. Rustodian runs standard `cargo build` and `cargo test` commands, which naturally isolate dependencies in the `target/` directory and use the `Cargo.lock` to ensure reproducible builds.

## Node.js
Node.js dependencies are typically isolated in the project's `node_modules` directory. Rustodian determines the correct package manager setup and testing commands dynamically by detecting lockfiles:
- If `yarn.lock` is present: Uses `yarn install` / `yarn test`
- If `pnpm-lock.yaml` is present: Uses `pnpm install` / `pnpm test`
- If `bun.lockb` is present: Uses `bun install` / `bun test`
- Fallback: Uses `npm install` / `npm test`

## Go
Go modules globally cache packages, which can leak state across projects. To prevent this, Rustodian injects a project-local `GOPATH` overriding the global environment. The `GOPATH` is redirected to a `.gopath` folder within the project directory before running `go mod download` and `go test ./...`.

## Python
Python global installations are easily corrupted by disparate project requirements. Rustodian establishes a Virtual Environment (`.venv`) to strictly isolate packages:
1. It attempts to create the environment by falling back across common commands (`python3 -m venv .venv`, then `python -m venv .venv`).
2. Rustodian dynamically resolves paths for `pip`, `pytest`, and `python` executables inside the `.venv` depending on the host OS (`.venv\Scripts\` on Windows, `.venv/bin/` on Unix).
3. Dependencies are installed locally into the `.venv` using either `requirements.txt`, `pyproject.toml`, or `setup.py`.

## Language Command Mapping

| Language | Setup Command | Verify Command |
| --- | --- | --- |
| Rust | `cargo build` | `cargo test` |
| Node | `[yarn/pnpm/bun/npm] install` | `[yarn/pnpm/bun/npm] test` |
| Go | `go mod download` | `go test ./...` |
| Python | `.venv/[bin\|Scripts]/pip install [deps]` | `.venv/[bin\|Scripts]/pytest -v` or `python -m unittest discover` |
