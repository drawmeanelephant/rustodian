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
1. **Component Verification:** Extraction is rejected if any path component is not a normal file/directory or the current directory (`.`). `..` components trigger an immediate security error.
2. **Prefix Stripping:** Top-level archive directories are discarded via component iterator manipulation (`strip_prefix`) to prevent unnecessary nesting.
3. **Canonicalization Checks:** It uses `canonicalize` on the target directory parent of each entry, strictly validating that the resolved extraction path begins exactly with the intended extraction root.
4. **Symlink Mitigation:** If an archive contains a symlink pointing outside the root and a subsequent entry attempts to write to it, the canonicalization check intercepts the operation and aborts extraction, preventing arbitrary file overwrites.

## Preserve Patterns
To prevent overwriting local configurations or files when refreshing an archive, the downloader supports a `preserve_patterns` glob mechanism. During extraction, each archive entry's stripped path is matched against a compiled `globset`. If an entry matches a preserve pattern (e.g., `config.json`, `*.local`), it is safely skipped, leaving the local file intact.

## Rate Limit Handling
When fetching PRs, `GithubDownloader` monitors the HTTP response. A `403 Forbidden` with an `X-RateLimit-Remaining` header of `"0"` is mapped to `CoreError::RateLimitExceeded`. This enables upper layers to handle rate limits gracefully.

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
Could not find the project in database by path: ./my_remotes/octocat/Hello-World
```
