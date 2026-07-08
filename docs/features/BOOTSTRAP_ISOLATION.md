# Environment Isolation in Rustodian

When Rustodian bootstraps and verifies projects, it isolates operations to prevent host system pollution, ensuring reproducible environments.

## Language Command Mapping

| Language | Isolation Mechanism | Setup Command | Verify Command |
|---|---|---|---|
| **Rust** | Native `target/` directory | `cargo build` | `cargo test` |
| **Node.js** | Local `node_modules` | `<mgr> install` [^1] | `<mgr> test` [^1] |
| **Go** | `GOPATH` override to `.gopath` | `go mod download` | `go test ./...` |
| **Python** | Virtual Environment (`.venv`) | Sequential `pip install` [^2] | `pytest -v` or `unittest` [^3] |

[^1]: Rustodian selects the package manager (`yarn`, `pnpm`, `bun`, or `npm` as fallback) dynamically based on detected lockfiles.
[^2]: After creating the virtual environment (attempting `python3 -m venv .venv`, falling back to `python -m venv .venv`), dependencies are installed sequentially. It executes `pip install -r requirements.txt` if the file exists, followed by `pip install .` if `pyproject.toml` or `setup.py` exists. The `pip` executable resolves to `.venv/bin/pip` (Unix) or `.venv\Scripts\pip` (Windows).
[^3]: Verification executes `pytest -v` if the `pytest` executable exists in the `.venv`, otherwise it falls back to `python -m unittest discover`. The path prefix is `.venv/bin/` (Unix) or `.venv\Scripts\` (Windows).

## Example: Mixed-Language Monorepo

Consider a monorepo containing a Node.js frontend and a Python backend:

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

During bootstrap and verification:
1. **Frontend**: Rustodian detects `pnpm-lock.yaml`, installing dependencies into `frontend/node_modules/` via `pnpm install`, and verifies the project using `pnpm test`.
2. **Backend**: It creates a virtual environment at `backend/.venv`. Using the isolated executables (e.g., `backend/.venv/bin/pip` on Unix or `backend\.venv\Scripts\pip` on Windows), it sequentially runs `pip install -r requirements.txt` and `pip install .`. It then verifies the project using the localized `pytest -v` (or `python -m unittest discover` if `pytest` is absent).

Each project's environment remains fully localized, ensuring they neither interfere with one another nor pollute the host system's global state.
