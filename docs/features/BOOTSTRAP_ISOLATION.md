# Environment Isolation in Rustodian

## Why Isolation Matters
When Rustodian bootstraps and verifies projects, it isolates operations to prevent polluting the host system. This ensures clean, reproducible environments across codebases and avoids version conflicts.

## Language Command Mapping

| Language | Isolation Mechanism | Setup Command | Verify Command |
| --- | --- | --- | --- |
| Rust | Cargo (`target/`) | `cargo build` | `cargo test` |
| Node | Local `node_modules` | `[yarn/pnpm/bun/npm] install` | `[yarn/pnpm/bun/npm] test` |
| Go | Local `GOPATH` override to `.gopath` | `go mod download` | `go test ./...` |
| Python | Virtual Env (`.venv`) | Unix: `.venv/bin/pip install [-r requirements.txt\|.]`<br>Win: `.venv\Scripts\pip install [-r requirements.txt\|.]` | Unix: `.venv/bin/pytest -v` (fallback: `.venv/bin/python -m unittest discover`)<br>Win: `.venv\Scripts\pytest -v` (fallback: `.venv\Scripts\python -m unittest discover`) |

## Isolation Strategies

**Rust:** Cargo isolates builds natively in the `target/` directory.

**Node.js:** Dependencies are isolated in the local `node_modules` directory. Rustodian detects lockfiles (`yarn.lock`, `pnpm-lock.yaml`, `bun.lockb`) to use the correct package manager (with `npm` as fallback).

**Go:** To prevent global module cache pollution, Rustodian overrides the global `GOPATH` environment variable with a project-local `.gopath` directory before running.

**Python:** Rustodian strictly uses a Virtual Environment (`.venv`) to isolate packages:
1. Attempts creation with `python3 -m venv .venv`, falling back to `python -m venv .venv`.
2. Dependencies are installed using the local `pip` (`install -r requirements.txt` or `install .`), with paths resolving dynamically by OS (Unix: `.venv/bin/pip`, Windows: `.venv\Scripts\pip`).
3. Verification runs via the local `pytest -v`, falling back to `python -m unittest discover` using OS-specific executable paths.

## Example: Mixed-Language Monorepo
Consider a monorepo containing a frontend and backend:
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
2. **Backend:** It creates `backend/.venv`. On Unix, it installs dependencies using `backend/.venv/bin/pip install -r requirements.txt` and verifies with `backend/.venv/bin/pytest -v` (or `unittest` if `pytest` is absent). Windows uses `backend\.venv\Scripts\pip` and `backend\.venv\Scripts\pytest`.

Neither project affects the host system's global state or each other.
