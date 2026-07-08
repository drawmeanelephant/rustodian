# Remote Repository Tracking

This document outlines the remote repository tracking features implemented in the `rustodian-remote` crate, specifically focusing on `GithubDownloader` in `crates/rustodian-remote/src/downloader.rs`.

## Pull Requests
The `PullRequestFetcher` trait defines the interface for retrieving open PRs. `GithubDownloader` implements this trait to fetch PR metadata (number, title, author, branch, URL, update time, and draft status) directly from the GitHub API.

In `rustodian-desktop`, the Pull Requests tab is **fully operational and no longer a placeholder**. The Slint UI safely interacts with the asynchronous `PullRequestFetcher` trait through a background messaging protocol. Upon receiving a UI request via the worker channel, the background worker spawns a short-lived `Tokio` runtime. This elegantly bridges the synchronous UI event loop with the async PR fetching logic, preventing main thread blocking.

## GithubDownloader Flow
When fetching an archive, `GithubDownloader` first requests the `main` branch tarball (`/archive/refs/heads/main.tar.gz`). If it encounters a `404 Not Found` error, it gracefully falls back to `master`, ensuring broad compatibility with varying branch naming conventions.

## Zip Slip and Path Traversal Protections
Extracting untrusted archives carries critical "Zip Slip" risks, where malicious entries might use path traversal (`../`) or symlinks to overwrite files outside the target directory. The downloader implements strict mitigation mechanisms:

1. **Component Verification:** Extraction immediately aborts with a security error if any path component is a `..` (parent directory). Only normal file, directory, or current directory (`.`) components are allowed.
2. **Prefix Stripping:** To prevent unnecessary nesting, the top-level archive directory is seamlessly discarded by advancing the path's component iterator.
3. **Canonicalization Checks:** The downloader calls `canonicalize` on the parent directory of each extraction target. It strictly verifies that the fully resolved path begins exactly with the intended extraction root.
4. **Symlink Mitigation:** This canonicalization check simultaneously neutralizes symlink attacks. If an archive entry attempts to write through a symlink pointing outside the root, the check intercepts the violation and aborts the extraction.

## Preserve Patterns
To safeguard local configurations from being overwritten during a refresh, the downloader integrates a `preserve_patterns` mechanism. Powered by the `globset` crate, it matches each entry's stripped path against a compiled set of globs (e.g., `config.json`, `*.local`). Matching entries are safely skipped.

## Rate Limit Handling
When fetching PRs, `GithubDownloader` monitors the HTTP response. A `403 Forbidden` with an `X-RateLimit-Remaining` header of `"0"` is mapped to `CoreError::RateLimitExceeded`. This enables upper layers to handle rate limits gracefully.

## Example CLI Usage
You can use the `rustodian` CLI to manage remote repositories. Here is a realistic end-to-end example showing how to add a repo, list it, and refresh with `--preserve` behavior active:

```bash
$ mkdir -p ./my_remotes
$ rustodian remote add octocat/Hello-World --preserve "config.json"
Added remote project: octocat/Hello-World

$ rustodian remote list
+---------------------+-------------------+
| Repo Slug           | Preserve Patterns |
+=========================================+
| octocat/Hello-World | config.json       |
+---------------------+-------------------+

$ rustodian remote refresh --dest ./my_remotes --preserve "config.json"
Refreshing octocat/Hello-World...
Successfully downloaded and extracted octocat/Hello-World
Scanning project octocat/Hello-World...
Scan completed. Found 1 projects.
```
