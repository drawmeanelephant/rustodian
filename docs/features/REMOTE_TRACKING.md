# Remote Repository Tracking

This document outlines the remote repository tracking features implemented in the `rustodian-remote` crate, specifically focusing on `GithubDownloader` in `crates/rustodian-remote/src/downloader.rs`.

## Pull Requests
The `PullRequestFetcher` trait defines the interface for fetching open PRs. `GithubDownloader` implements this trait, fetching PR metadata (number, title, author, branch, url, update time, and draft status) from the GitHub API.

In the desktop application (`rustodian-desktop`), the remote Pull Requests tab is fully operational. The Slint UI interacts with the async `PullRequestFetcher` trait via background thread messaging. When the UI dispatches a message over the worker channel, the background worker creates a short-lived, local `Tokio` runtime to bridge the synchronous event loop and the async PR fetching logic without blocking the main thread.

## GithubDownloader Flow
When downloading an archive, `GithubDownloader` requests the `main` branch tarball (`/archive/refs/heads/main.tar.gz`). If it receives a `404 Not Found`, it automatically falls back to `master` (`/archive/refs/heads/master.tar.gz`), ensuring compatibility with both new and legacy branch naming conventions.

## Zip Slip and Path Traversal Protections
Extracting untrusted archives carries "Zip Slip" risks, where malicious entries use path traversal (`../`) or symlinks to overwrite files outside the intended directory.

To mitigate this, the downloader implements strict protections:
1. **Entry Type Refusal:** Only regular files and directories may be extracted. Any symbolic link or hard link entry rejects the entire archive up front — before anything is written — and exotic entry types (FIFOs, devices, etc.) are refused the same way.
2. **Component Verification:** Extraction is rejected if any path component is not a normal file/directory or the current directory (`.`). `..` components trigger an immediate security error.
3. **Prefix Stripping:** Top-level archive directories are discarded via component iterator manipulation (`strip_prefix`) to prevent unnecessary nesting.
4. **Canonical Destination:** The destination directory is created if needed and canonicalized exactly once before extraction; a failure to resolve it is a hard error, and extraction never falls back to an uncanonicalized path.
5. **Canonicalization Checks:** For each entry, the target directory parent is canonicalized and strictly validated to begin exactly with the resolved extraction root, defending against pre-existing symlinks in the destination.

The combination means a single link entry in an archive aborts the whole extraction, so a symlink can never be created first and then followed by a nested entry to write outside the destination.

## Preserve Patterns
To prevent overwriting local configurations or files when refreshing an archive, the downloader supports a `preserve_patterns` glob mechanism. During extraction, each archive entry's stripped path is matched against a compiled `globset`. If an entry matches a preserve pattern (e.g., `config.json`, `*.local`), it is safely skipped, leaving the local file intact.

## Rate Limit Handling
When fetching PRs, `GithubDownloader` monitors the HTTP response. A `403 Forbidden` with an `X-RateLimit-Remaining` header of `"0"` is mapped to `CoreError::RateLimitExceeded`. This enables upper layers to handle rate limits gracefully.

## Safety: Refresh Never Executes Downloaded Code

`rustodian remote refresh` is a **synchronization** operation, not a build or test operation. It downloads the archive, extracts it (applying `preserve_patterns`), and scans/indexes the resulting project. It **never** bootstraps the downloaded project and **never** executes code from it:

- no `cargo build` / `cargo test`
- no `npm` / `yarn` / `pnpm` / `bun` install or test commands
- no `go mod download` / `go test`
- no Python virtualenv creation or `pip` installs
- no Justfile recipes or other discovered project commands

Downloading an untrusted repository must not imply executing it. If you want to build or test a project, run the dedicated `rustodian run` command explicitly after you have reviewed the code.

## Example CLI Usage
You can use the `rustodian` CLI to manage remote repositories. Here is a realistic end-to-end example: adding a project with a preserve pattern, listing tracked projects, and refreshing the repository.

```bash
$ rustodian remote add octocat/Hello-World --preserve "config.json"
Added remote project: octocat/Hello-World

$ rustodian remote list
+---------------------+-------------------+
| Repo Slug           | Preserve Patterns |
+=========================================+
| octocat/Hello-World | config.json       |
+---------------------+-------------------+

$ rustodian remote refresh --dest ./my_remotes
Refreshing octocat/Hello-World...
Successfully refreshed octocat/Hello-World
Scanning project octocat/Hello-World...
Scan completed. Found 0 projects.
Index updated for octocat/Hello-World (download, extract, and scan only — no code was executed).
```
