# Remote Repository Tracking

This document outlines the remote repository tracking features implemented in the `rustodian-remote` crate, specifically focusing on the `GithubDownloader` in `crates/rustodian-remote/src/downloader.rs`.

## GithubDownloader Flow

When downloading a repository archive, `GithubDownloader` defaults to requesting the `main` branch tarball (`/archive/refs/heads/main.tar.gz`). If the server responds with a `404 Not Found`, the downloader automatically falls back to requesting the `master` branch tarball (`/archive/refs/heads/master.tar.gz`). This ensures compatibility with both newer and legacy repository default branch naming conventions.

## Zip Slip and Path Traversal Protections

Extracting untrusted archives carries a risk of "Zip Slip" vulnerabilities, where malicious entries contain path traversal components (e.g., `../../`) or symbolic links designed to overwrite files outside the intended extraction directory.

The `rustodian-remote` downloader implements robust protections against these attacks:
1. **Component Verification:** Before extracting any entry, its path components are inspected. The extraction is immediately rejected if the path contains anything other than normal directory/file components or current directory references (`.`). Any `..` components trigger an error.
2. **Prefix Stripping (`strip_prefix`):** Top-level directories within archives are discarded using component iterator manipulation (acting as a `strip_prefix`). This ensures extracted files do not nest unnecessarily under a repository's root folder name.
3. **Canonicalization Checks:** The downloader uses `canonicalize` to determine the absolute, resolved path of the destination directory prior to writing. It strictly validates that the resolved extraction path starts exactly with the intended target root path.
4. **Symlink Attack Mitigation:** As proven in our test suite (`test_download_and_extract_zip_slip_symlink`), if a malicious archive attempts to extract a symlink that points outside the extraction root and then writes a file into that symlinked directory, the canonicalization check successfully detects the boundary violation and aborts the extraction with a security violation error.

## Preserve Patterns

To prevent local configuration or custom files from being overwritten when an archive is refreshed, the downloader supports a `preserve_patterns` glob mechanism. Users can define a list of glob patterns (e.g., `*.json`, `config/*`).

During extraction, the path of each archive entry is matched against a `globset`. If an entry matches a preserve pattern, the file is safely skipped, leaving the existing local file intact.

## Rate Limit Handling

The GitHub API enforces strict rate limits. When fetching data (such as pull requests), the `GithubDownloader` checks the HTTP response status. If it receives a `403 Forbidden` and the `X-RateLimit-Remaining` header is exactly `"0"`, this is explicitly mapped to a `CoreError::RateLimitExceeded`. This allows upper layers of the application to gracefully handle rate limit exhaustion (e.g., by informing the user or backing off).

## Pull Requests (Not Yet Implemented in Desktop)

The `PullRequestFetcher` trait defines the interface for fetching open PRs. The `GithubDownloader` implements this trait, successfully fetching PR metadata (number, title, author, branch, url, update time, and draft status) from the GitHub API.

**Note to Contributors:** While the backend fetching logic in `rustodian-remote` is fully functional and tested, the Pull Requests tab in `rustodian-desktop` is currently a placeholder. It is **Not Yet Implemented** and not wired up to display the fetched PR data.

## Example CLI Usage

You can use the `rustodian` CLI to manage remote repositories.

```bash
# Add a remote repository to track
cargo run --bin rustodian -- remote add drawmeanelephant/rustodian

# List all tracked remote repositories
cargo run --bin rustodian -- remote list

# Refresh a remote repository, preserving files matching a pattern
cargo run --bin rustodian -- remote refresh drawmeanelephant/rustodian --preserve "config.json"
```
