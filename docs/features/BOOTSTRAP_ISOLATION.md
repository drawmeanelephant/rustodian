# Environment Isolation in Rustodian

## Why Isolation Matters
When Rustodian bootstraps and verifies projects, it strictly isolates operations to prevent polluting the host system. This ensures a clean, reproducible environment across heterogeneous codebases and avoids version conflicts.

## Isolation Strategies

**Rust**
Cargo natively isolates builds in the `target/` directory. Rustodian runs standard `cargo build` and `cargo test`.

**Node.js**
Dependencies are isolated in the local `node_modules` directory. Rustodian dynamically detects lockfiles (`yarn.lock`, `pnpm-lock.yaml`, `bun.lockb`) to use the correct package manager (`yarn`, `pnpm`, `bun`, or `npm` fallback).

**Go**
To prevent global module cache pollution, Rustodian overrides the global `GOPATH` with a project-local `.gopath` directory before running `go mod download` and `go test ./...`.

**Python**
Rustodian creates a strict Virtual Environment (`.venv`) to isolate packages.
1. It attempts environment creation using `python3 -m venv .venv`, falling back to `python -m venv .venv`.
2. Paths are dynamically resolved based on the OS: `.venv\Scripts\` on Windows, and `.venv/bin/` on Unix.
3. Dependencies are installed using the `.venv`'s `pip` (`pip install -r requirements.txt` or `pip install .`).
4. Verification runs via `.venv`'s `pytest` or falls back to `python -m unittest discover`.

## Language Command Mapping

| Language | Isolation Mechanism | Setup Command | Verify Command |
| --- | --- | --- | --- |
| Rust | Cargo (`target/`) | `cargo build` | `cargo test` |
| Node | `node_modules` | `[yarn/pnpm/bun/npm] install` | `[yarn/pnpm/bun/npm] test` |
| Go | Local `GOPATH` (`.gopath/`) | `go mod download` | `go test ./...` |
| Python | Virtual Env (`.venv/`) | `.venv/[bin\|Scripts]/pip install [-r requirements.txt\|.]` | `.venv/[bin\|Scripts]/[pytest -v\|python -m unittest discover]` |

## Example: Mixed-Language Monorepo
Consider a monorepo containing a frontend app and a backend API:
```text
my-monorepo/
├── frontend/ (Node)
│   ├── pnpm-lock.yaml
│   └── package.json
└── backend/ (Python)
    ├── requirements.txt
    └── main.py
```
When Rustodian scans this directory:
1. **Frontend:** It detects `pnpm-lock.yaml`, isolating dependencies in `frontend/node_modules/` via `pnpm install`, and verifies with `pnpm test`.
2. **Backend:** It creates `backend/.venv`, uses OS-specific paths (e.g., `backend/.venv/bin/pip install -r requirements.txt`) to install dependencies, and verifies using `backend/.venv/bin/pytest -v` (falling back to `unittest` if `pytest` is absent).

Neither project affects the host system's global state or each other.