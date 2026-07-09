# Environment Isolation in Rustodian

When Rustodian bootstraps and verifies projects, it isolates operations to prevent host system pollution, ensuring reproducible environments across codebases.

## Language Command Mapping

| Language | Isolation Mechanism | Setup Command | Verify Command |
| :--- | :--- | :--- | :--- |
| Rust | No isolation (native `target/`) | `cargo build` | `cargo test` |
| Node | Local `node_modules` directory | `npm install` [^1] | `npm test` [^1] |
| Go | `GOPATH` env var override to `.gopath` | `go mod download` | `go test ./...` |
| Python | Virtual Env (`.venv`) directory | `pip install -r requirements.txt`<br>and/or `pip install .` [^2] | `pytest -v`<br>(fallback: `python -m unittest discover`) [^3] |

[^1]: Node dynamically detects lockfiles to substitute `npm` with the correct package manager (`yarn`, `pnpm`, or `bun`).
[^2]: Python creates `.venv` via `python3 -m venv .venv` (fallback: `python`). It then installs dependencies sequentially. The path differs by OS (Unix: `.venv/bin/pip`, Windows: `.venv\Scripts\pip`).
[^3]: Uses `.venv/bin/pytest -v` (Windows: `.venv\Scripts\pytest -v`) if present. Otherwise falls back to `.venv/bin/python -m unittest discover` (Windows: `.venv\Scripts\python -m unittest discover`).

## Example: Mixed-Language Monorepo

```text
my-monorepo/
├── frontend/ (Node)
│   ├── pnpm-lock.yaml
│   └── package.json
└── backend/ (Python)
    ├── pyproject.toml
    ├── requirements.txt
    └── main.py
```

When Rustodian scans this directory:
1. **Frontend:** Detects `pnpm-lock.yaml`, isolating dependencies in `frontend/node_modules/` via `pnpm install`, and verifies using `pnpm test`.
2. **Backend:** Creates `backend/.venv`. On Unix, it sequentially runs `backend/.venv/bin/pip install -r requirements.txt` followed by `backend/.venv/bin/pip install .`. It verifies with `backend/.venv/bin/pytest -v` (or `backend/.venv/bin/python -m unittest discover`). Windows uses `backend\.venv\Scripts\pip` and `backend\.venv\Scripts\pytest -v` (or `backend\.venv\Scripts\python -m unittest discover`).

Neither project affects the host system's global state or each other.
