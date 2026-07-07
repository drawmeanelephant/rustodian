# Environment Isolation in Rustodian

Rustodian isolates operations during project bootstrapping and verification to prevent host system pollution and ensure reproducible environments.

## Language Command Mapping

| Language | Isolation Mechanism | Setup Command | Verify Command |
| :--- | :--- | :--- | :--- |
| **Rust** | None (native `target/`) | `cargo build` | `cargo test` |
| **Node** | Local `node_modules` | `{pkg_mgr} install` [^1] | `{pkg_mgr} test` [^1] |
| **Go** | `GOPATH` env override to `.gopath` | `go mod download` | `go test ./...` |
| **Python** | Virtual Environment (`.venv`) | `{pip} install -r requirements.txt`<br>and/or `{pip} install .` [^2] | `{pytest} -v`<br>or `{python} -m unittest discover` [^3] |

[^1]: `{pkg_mgr}` is dynamically selected based on lockfile detection (`yarn`, `pnpm`, `bun`, or fallback to `npm`).
[^2]: Python tries to create `.venv` via `python3 -m venv` (fallback to `python -m venv`). It then installs dependencies sequentially: first `requirements.txt` if present, then `.` if `pyproject.toml` or `setup.py` exists. The `{pip}` path is OS-specific (`.venv/bin/pip` on Unix, `.venv\Scripts\pip` on Windows).
[^3]: Uses `{pytest}` if the executable exists, otherwise falls back to `{python} -m unittest discover`. Paths are OS-specific (`.venv/bin/pytest` and `.venv/bin/python` on Unix, `.venv\Scripts\pytest` and `.venv\Scripts\python` on Windows).

## Example: Mixed-Language Monorepo

```text
my-monorepo/
├── frontend/
│   ├── pnpm-lock.yaml
│   └── package.json
└── backend/
    ├── pyproject.toml
    ├── requirements.txt
    └── main.py
```

When Rustodian scans this directory:
1. **Frontend (Node):** Detects `pnpm-lock.yaml`, isolating dependencies in `frontend/node_modules/`. It runs `pnpm install` and verifies with `pnpm test`.
2. **Backend (Python):** Creates `backend/.venv`. It sequentially installs dependencies using the local `.venv` executables (e.g., `{pip} install -r requirements.txt` then `{pip} install .`) and verifies with `{pytest} -v` (or the `unittest` fallback).
   - **Unix:** Runs `backend/.venv/bin/pip` and `backend/.venv/bin/pytest`.
   - **Windows:** Runs `backend\.venv\Scripts\pip` and `backend\.venv\Scripts\pytest`.

Neither project affects the host system's global state or each other.
