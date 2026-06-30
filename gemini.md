# Project Brief: Rustodian (Department of Project Custodianship) 🦀🏛️

## 1. Project Identity & Vision

Rustodian is a personal project observatory and local code portfolio manager. It autonomously walks developer directories, indexes software project roots, manages build/test commands, and captures runtime histories in a local database. It is built with a minimalist, brutalist, zero-hydration mindset—eschewing heavy async runtimes or frontend bloat in favor of native Unix performance, flat-file RAG indexing, and clean system abstractions.

* **Current Target Status**: Core workspace is fully updated, verified **green**, and compiles with zero Clippy errors or warnings.


* **The Big Objective**: Transition from a passive metadata indexer to an active digital janitor capable of safe environment isolation, workspace artifact cleanup, and git-focused change tracking.



---

## 2. Crate Architecture & Layout

The workspace adheres to a strict layered boundary design. Infrastructure modules are decoupled and *never* depend on each other; they only communicate using core traits.

```
                    rustodian-cli (Composition Root)[cite: 2]
                   /       |        \
                  /        |         \
    rustodian-storage  rustodian-scanner  rustodian-git[cite: 2]
                  \        |         /
                   \       |        /
                    rustodian-core (Domain Contracts)[cite: 2]
                          |
                    rustodian-types (Shared Data Bag)[cite: 2]

```

### Core Workspace Packages

* **`rustodian-types`**: Zero behavior. Pure data serialization bags, newtypes, and enums (`Project`, `Language`, `VcsInfo`).


* **`rustodian-core`**: The orchestrator (`Custodian`). Defines the core domain traits (`ProjectStore`, `ProjectScanner`, `GitInspector`) and houses the execution runners.


* **`rustodian-storage`**: A synchronous SQLite backend backed by `rusqlite` and an `r2d2` connection pool. Runs database migrations automatically.


* **`rustodian-scanner`**: A parallel filesystem explorer leveraging the `ignore` crate for fast, `.gitignore`-aware project root identification.


* **`rustodian-git`**: Repository metadata inspector using native `git2` bindings.


* **`rustodian-cli`**: Command-line interface composition root.


* **`rustodian-desktop`**: Graphical interface leveraging `eframe` (egui) with a background channel worker pipeline for non-blocking operations.


* **`xtask`**: Independent workspace automation workspace member containing custom project tasks.



---

## 3. Persistent Data Layer (SQLite Schema)

The system leverages raw SQL schema migrations managed via a custom tracker table. Data is isolated across four target tables:

1. **`projects`**: Holds core path maps, discovery timestamps, and an extensible, flattened `metadata_json` column to prevent migration friction.


2. **`project_languages`**: Tracks detected workspace languages and confidence layers.


3. **`scans`**: Historical audit tracking of filesystem traversal batches.


4. **`project_logs`**: Stores captured command run outputs, execution timestamps, and process return codes.




