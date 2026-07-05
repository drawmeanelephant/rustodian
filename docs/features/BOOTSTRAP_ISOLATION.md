# Environment Isolation in Rustodian

When Rustodian bootstraps and verifies projects, it isolates operations to prevent host system pollution, ensuring reproducible environments across codebases.

## Language Command Mapping

| Language | Isolation Mechanism | Setup Command | Verify Command |
| --- | --- | --- | --- |
| Rust | No isolation (native `target/`) | `cargo build` | `cargo test` |
| Node | Local `node_modules` directory | `[yarn/pnpm/bun/npm] install` | `[yarn/pnpm/bun/npm] test` |
| Go | `GOPATH` env var override to `.gopath` | `go mod download` | `go test ./...` |
| Python | Virtual Env (`.venv`) directory | Unix: `.venv/bin/pip install -r requirements.txt`<br>and/or `.venv/bin/pip install .`<br>Win: `.venv\Scripts\pip install -r requirements.txt`<br>and/or `.venv\Scripts\pip install .` | Unix: `.venv/bin/pytest -v`<br>(fallback: `.venv/bin/python -m unittest discover`)<br>Win: `.venv\Scripts\pytest -v`<br>(fallback: `.venv\Scripts\python -m unittest discover`) |

## Isolation Strategies

**Rust:** Builds natively in the `target/` directory with no additional isolation.

**Node.js:** Dependencies are localized in `node_modules`. Rustodian detects lockfiles (`yarn.lock`, `pnpm-lock.yaml`, `bun.lockb`) to select the package manager, with `npm` as the fallback.

**Go:** Rustodian overrides the global `GOPATH` environment variable to a project-local `.gopath` directory, protecting the global module cache.

**Python:** Rustodian uses a Virtual Environment (`.venv`):
1. Attempts creation via `python3 -m venv .venv`, falling back to `python -m venv .venv`.
2. Installs dependencies sequentially: runs `pip install -r requirements.txt` if `requirements.txt` exists; runs `pip install .` if `pyproject.toml` or `setup.py` exists (using `.venv/bin/pip` on Unix or `.venv\Scripts\pip` on Windows).
3. Verifies via local `pytest -v` if the executable exists, falling back to `python -m unittest discover` (using `.venv/bin/` on Unix or `.venv\Scripts\` on Windows).

## Example: Mixed-Language Monorepo

```text
my-monorepo/
├── frontend/ (Node)
│   ├── pnpm-lock.yaml
│   └── package.json
└── backend/ (Python)
    ├── pyproject.toml
    └── main.py
```

When Rustodian scans this directory:
1. **Frontend:** It detects `pnpm-lock.yaml`, isolating dependencies in `frontend/node_modules/` via `pnpm install`, and verifies using `pnpm test`.
2. **Backend:** It creates `backend/.venv`. On Unix, it runs `backend/.venv/bin/pip install .` and verifies with `backend/.venv/bin/pytest -v` (or `unittest`). Windows uses `backend\.venv\Scripts\pip` and `backend\.venv\Scripts\pytest`.

Neither project affects the host system's global state or each other.
