# Remote Repository Tracking

This document outlines the remote repository tracking features implemented in the `rustodian-remote` crate, specifically focusing on the `GithubDownloader` in `crates/rustodian-remote/src/downloader.rs`.

## Pull Requests (Not Yet Implemented in Desktop)

**Note to Contributors:** While the backend fetching logic in `rustodian-remote` is fully functional and tested, the Pull Requests tab in `rustodian-desktop` is currently a placeholder. It is **Not Yet Implemented** and not wired up to display the fetched PR data.

The `PullRequestFetcher` trait defines the interface for fetching open PRs. The `GithubDownloader` implements this trait, fetching PR metadata (number, title, author, branch, url, update time, and draft status) from the GitHub API.

## GithubDownloader Flow

When downloading an archive, `GithubDownloader` defaults to requesting the `main` branch tarball (`/archive/refs/heads/main.tar.gz`). If it receives a `404 Not Found`, it automatically falls back to `master` (`/archive/refs/heads/master.tar.gz`), ensuring compatibility with both new and legacy branch naming conventions.

## Zip Slip and Path Traversal Protections

Extracting untrusted archives carries a risk of "Zip Slip" vulnerabilities, where malicious entries use path traversal (`../../`) or symlinks to overwrite files outside the intended directory.

The downloader implements robust protections:
1. **Component Verification:** Extraction is rejected if any path component is anything other than a normal file/directory or current directory reference (`.`). `..` components trigger an error.
2. **Prefix Stripping:** Top-level archive directories are discarded via component iterator manipulation (`strip_prefix`), preventing unnecessary nesting.
3. **Canonicalization Checks:** It uses `canonicalize` on the destination directory, strictly validating that the resolved extraction path starts exactly with the intended target root.
4. **Symlink Mitigation:** As verified in our test suite, if an archive extracts a symlink pointing outside the root and attempts to write to it, the canonicalization check aborts the extraction with a security violation.

## Preserve Patterns

To prevent overwriting local configurations when refreshing an archive, the downloader supports a `preserve_patterns` glob mechanism.

During extraction, each archive entry's path is matched against a `globset`. If an entry matches a preserve pattern (e.g., `*.json`, `config/*`), it is safely skipped, leaving the local file intact.

## Rate Limit Handling

When fetching data, the `GithubDownloader` checks the HTTP response. A `403 Forbidden` with the `X-RateLimit-Remaining` header at `"0"` is mapped to `CoreError::RateLimitExceeded`. This allows upper layers to handle rate limit exhaustion gracefully.

## Example CLI Usage

Use the `rustodian` CLI to manage remote repositories.

```bash
$ cargo run --bin rustodian -- remote add octocat/Hello-World --preserve "config.json"
Added remote project: octocat/Hello-World

$ cargo run --bin rustodian -- remote list
+---------------------+-------------------+
| Repo Slug           | Preserve Patterns |
+=========================================+
| octocat/Hello-World | config.json       |
+---------------------+-------------------+

$ cargo run --bin rustodian -- remote refresh
Refreshing octocat/Hello-World...
```
