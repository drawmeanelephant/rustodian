# Digital Janitor

The Digital Janitor is an autonomous workspace cruft purger that inspects tracked projects for bloated build artifacts and temporary directories, calculates reclaimable bytes, and optionally purges them.

## Language-Aware Targets

The Janitor targets specific well-known artifact directories that are generally safe to remove because they can be easily reconstructed by standard build tools.

| Detected language | Target directories | Why they're safe to delete |
|-------------------|--------------------|----------------------------|
| Rust              | `target`           | Rebuilt by Cargo           |
| Node              | `node_modules`, `.next` | Restored by package install or Next.js |
| Python            | `.venv`, `__pycache__` anywhere below the project root | Recreated by Python tooling |
| Go                | `.gopath`          | Dependencies can be fetched again |

Generic `build` and `dist` directories are intentionally never selected: their contents are too project-specific to treat as universally disposable.

## Dry-run vs Purge

By default, the Janitor operates in **dry-run** mode, calculating potential space savings via a recursive directory walk (`dirsize`) without deleting anything. On deep or file-heavy directories, this calculation may take a noticeable amount of time. To execute the actual deletion, you must explicitly provide the `--purge` flag.

Before sizing or deletion, the Janitor validates the resolved project root and verifies each candidate remains lexically and canonically inside it. Cleanup-target symlinks are refused, and nested symlinks are excluded from size calculations. Each target has an outcome (`reclaimable`, `removed`, `skipped`, or `failed`) plus a size when available and a reason for skips or failures.

Every purge attempt is fully auditable. The Janitor writes one `janitor:clean` log with exact targets, outcomes, reclaimed bytes, failures, and overall success. Dry runs never write logs or mutate the filesystem/database. A purge that has a failed target prints the full report and exits with code 1.

## Worked Example

Suppose you have a project with a stale Rust `target/` directory taking up about 850 MB.

**Dry-run inspection (default):**
```bash
$ rustodian janitor example-rust-app
+--------------+-------------+-----------+--------+
| Cruft Target | Outcome     | Size      | Reason |
+==============+=============+===========+========+
| target       | reclaimable | 850.9 MiB |        |
+--------------+-------------+-----------+--------+
| Total        | reclaimable | 850.9 MiB |        |
+--------------+-------------+-----------+--------+
```

**Actual Purge operation:**
```bash
$ rustodian janitor example-rust-app --purge
+--------------+---------+-----------+--------+
| Cruft Target | Outcome | Size      | Reason |
+==============+=========+===========+========+
| target       | removed | 850.9 MiB |        |
+--------------+---------+-----------+--------+
| Total        | reclaimed | 850.9 MiB |      |
+--------------+---------+-----------+--------+
```

## Gotchas

* **Permission Denied Errors:** A target that cannot be measured or deleted is reported as `failed`; its bytes are never added to the reclaim total. Purges with failures exit nonzero after printing the complete report.
