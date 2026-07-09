# Remote Repository Tracking

This document outlines the remote repository tracking features implemented in the `rustodian-remote` crate, specifically focusing on `GithubDownloader` in `crates/rustodian-remote/src/downloader.rs`.

## Pull Requests

The `PullRequestFetcher` trait defines the interface for fetching open PRs. `GithubDownloader` implements this trait, fetching PR metadata (number, title, author, branch, url, update time, and draft status) from the GitHub API.

In the `rustodian-desktop` application, the remote Pull Requests tab is now fully operational (no longer a placeholder). The Slint UI interacts with the async `PullRequestFetcher` trait via background thread messaging. When the UI dispatches a message over the worker channel, the background worker creates a short-lived, local `Tokio` runtime to bridge the synchronous event loop and the async PR fetching logic without blocking the main thread.

## GithubDownloader Flow

When downloading an archive, `GithubDownloader` requests the `main` branch tarball (`/archive/refs/heads/main.tar.gz`). If it receives a `404 Not Found`, it automatically falls back to `master` (`/archive/refs/heads/master.tar.gz`), ensuring compatibility with both new and legacy branch naming conventions.

## Zip Slip and Path Traversal Protections

Extracting untrusted archives carries "Zip Slip" risks, where malicious entries use path traversal (`../`) or symlinks to overwrite files outside the intended destination.

To mitigate this, the downloader employs a strict four-step protection strategy:
1. **Component Verification:** Extraction is rejected if any path component is not a normal file, directory, or the current directory (`.`). Any `..` component triggers an immediate security error.
2. **Prefix Stripping:** Top-level archive directories are safely discarded via component iterator manipulation (advancing the iterator with `.next()`) to prevent unnecessary directory nesting.
3. **Canonicalization Checks:** The downloader uses `canonicalize` on the target directory parent of each entry, strictly validating that the resolved extraction path begins exactly with the intended extraction root.
4. **Symlink Mitigation:** If an archive contains a symlink pointing outside the extraction root and a subsequent entry attempts to write to it, the canonicalization check intercepts the operation and aborts the extraction.

## Preserve Patterns

To prevent overwriting local configurations or files during an archive refresh, the downloader supports a `preserve_patterns` mechanism. During extraction, the stripped path of each archive entry is matched against a compiled `globset`. If an entry matches a preserve pattern (e.g., `config.json`, `*.local`), it is safely skipped, leaving the local file intact.

## Rate Limit Handling

When fetching PRs, `GithubDownloader` monitors the HTTP responses. A `403 Forbidden` status with an `X-RateLimit-Remaining` header of `"0"` is mapped to `CoreError::RateLimitExceeded`. This enables upper layers to handle rate limits gracefully.

## Example CLI Usage

You can use the `rustodian` CLI to manage remote repositories. Below is a realistic end-to-end example demonstrating how to add a project with a preserve pattern, list tracked projects, and refresh the repository:

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
Scan completed. Found 1 projects.
Bootstrapping and verifying project Hello-World...
Successfully bootstrapped and verified Hello-World!
```
