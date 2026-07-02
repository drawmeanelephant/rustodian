use std::fs;
use std::path::Path;

use flate2::read::GzDecoder;
use globset::{Glob, GlobSetBuilder};
use reqwest::Client;
use tar::Archive;
use tracing::{debug, info};

use rustodian_core::traits::RemoteDownloader;
use rustodian_types::RemoteProject;

#[derive(Clone)]
pub struct GithubDownloader {
    client: Client,
    api_base_url: String,
}

impl GithubDownloader {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_base_url: "https://api.github.com".to_string(),
        }
    }

    pub fn with_api_base_url(mut self, url: String) -> Self {
        self.api_base_url = url;
        self
    }
}

impl Default for GithubDownloader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl RemoteDownloader for GithubDownloader {
    async fn download_and_extract(
        &self,
        project: &RemoteProject,
        dest_dir: &Path,
        preserve_patterns: &[String],
    ) -> Result<(), rustodian_core::CoreError> {
        info!("Downloading project {}", project.repo_slug);

        let canonical_dest = dest_dir
            .canonicalize()
            .unwrap_or_else(|_| dest_dir.to_path_buf());
        let mut builder = GlobSetBuilder::new();
        for pat in preserve_patterns {
            if let Ok(glob) = Glob::new(pat) {
                builder.add(glob);
            }
        }
        let preserve_set = builder
            .build()
            .unwrap_or_else(|_| GlobSetBuilder::new().build().unwrap());

        // Try main then master
        let dl_base = if self.api_base_url == "https://api.github.com" {
            "https://github.com".to_string()
        } else {
            self.api_base_url.clone()
        };
        let mut response = self
            .client
            .get(format!(
                "{}/{}/archive/refs/heads/main.tar.gz",
                dl_base, project.repo_slug
            ))
            .send()
            .await
            .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            response = self
                .client
                .get(format!(
                    "{}/{}/archive/refs/heads/master.tar.gz",
                    dl_base, project.repo_slug
                ))
                .send()
                .await
                .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;
        }

        if !response.status().is_success() {
            return Err(rustodian_core::CoreError::Internal(format!(
                "Failed to download {}: status {}",
                project.repo_slug,
                response.status()
            )));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;

        let tar = GzDecoder::new(std::io::Cursor::new(bytes));
        let mut archive = Archive::new(tar);

        let entries = archive
            .entries()
            .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;

        for entry in entries {
            let mut entry =
                entry.map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;
            let path = entry
                .path()
                .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;

            let mut components = path.components();
            components.next();
            let stripped_path = components.as_path();

            if stripped_path.as_os_str().is_empty() {
                continue;
            }

            // Security Fix: Prevent Path Traversal (Zip Slip)
            // Ensure the path does not contain components that escape the intended directory
            if stripped_path.components().any(|c| {
                !matches!(
                    c,
                    std::path::Component::Normal(_) | std::path::Component::CurDir
                )
            }) {
                return Err(rustodian_core::CoreError::Internal(format!(
                    "Security violation: Path traversal detected in archive entry {:?}",
                    path
                )));
            }

            if preserve_set.is_match(stripped_path) {
                debug!("Preserving file matching pattern: {:?}", stripped_path);
                continue;
            }

            let dest_path = dest_dir.join(stripped_path);
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;

                // Security Fix: Prevent Zip Slip via symlinks
                let canonical_parent = parent
                    .canonicalize()
                    .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;

                if !canonical_parent.starts_with(&canonical_dest) {
                    return Err(rustodian_core::CoreError::Internal(format!(
                        "Security violation: Zip Slip path traversal detected in archive entry {:?}",
                        path
                    )));
                }
            }

            entry
                .unpack(&dest_path)
                .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;
        }

        info!(
            "Successfully downloaded and extracted {}",
            project.repo_slug
        );
        Ok(())
    }
}

#[async_trait::async_trait]
impl rustodian_core::traits::PullRequestFetcher for GithubDownloader {
    async fn fetch_open_prs(
        &self,
        repo_slug: &str,
    ) -> Result<Vec<rustodian_types::PullRequest>, rustodian_core::CoreError> {
        let url = format!("{}/repos/{}/pulls?state=open", self.api_base_url, repo_slug);

        let mut req = self
            .client
            .get(&url)
            .header(reqwest::header::USER_AGENT, "rustodian");

        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            req = req.bearer_auth(token);
        }

        let response = req
            .send()
            .await
            .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;

        if response.status() == reqwest::StatusCode::FORBIDDEN
            && let Some(limit) = response.headers().get("X-RateLimit-Remaining")
            && limit.to_str().unwrap_or("") == "0"
        {
            return Err(rustodian_core::CoreError::RateLimitExceeded);
        }

        if !response.status().is_success() {
            return Err(rustodian_core::CoreError::Internal(format!(
                "Failed to fetch PRs for {}: status {}",
                repo_slug,
                response.status()
            )));
        }

        #[derive(serde::Deserialize)]
        struct GithubPR {
            number: u64,
            title: String,
            user: GithubUser,
            head: GithubHead,
            html_url: String,
            updated_at: chrono::DateTime<chrono::Utc>,
            draft: bool,
        }

        #[derive(serde::Deserialize)]
        struct GithubUser {
            login: String,
        }

        #[derive(serde::Deserialize)]
        struct GithubHead {
            #[serde(rename = "ref")]
            ref_name: String,
        }

        let gh_prs: Vec<GithubPR> = response
            .json()
            .await
            .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;

        Ok(gh_prs
            .into_iter()
            .map(|pr| rustodian_types::PullRequest {
                number: pr.number,
                title: pr.title,
                author: pr.user.login,
                branch: pr.head.ref_name,
                url: pr.html_url,
                updated_at: pr.updated_at,
                is_draft: pr.draft,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;
    use rustodian_core::traits::PullRequestFetcher;

    #[tokio::test]
    async fn test_fetch_open_prs_success() {
        let mut server = Server::new_async().await;

        let m = server
            .mock("GET", "/repos/drawmeanelephant/rustodian/pulls?state=open")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"
            [
                {
                    "number": 42,
                    "title": "Add Pull Request fetching",
                    "user": { "login": "jules" },
                    "head": { "ref": "feature/pr-fetch" },
                    "html_url": "https://github.com/drawmeanelephant/rustodian/pull/42",
                    "updated_at": "2023-10-01T12:00:00Z",
                    "draft": false
                }
            ]
            "#,
            )
            .create_async()
            .await;

        let downloader = GithubDownloader::new().with_api_base_url(server.url());
        let prs = downloader
            .fetch_open_prs("drawmeanelephant/rustodian")
            .await
            .unwrap();

        assert_eq!(prs.len(), 1);
        assert_eq!(prs[0].number, 42);
        assert_eq!(prs[0].title, "Add Pull Request fetching");
        assert_eq!(prs[0].author, "jules");
        assert_eq!(prs[0].branch, "feature/pr-fetch");
        assert!(!prs[0].is_draft);

        m.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_open_prs_rate_limit() {
        let mut server = Server::new_async().await;

        let m = server
            .mock("GET", "/repos/drawmeanelephant/rustodian/pulls?state=open")
            .with_status(403)
            .with_header("X-RateLimit-Remaining", "0")
            .create_async()
            .await;

        let downloader = GithubDownloader::new().with_api_base_url(server.url());
        let err = downloader
            .fetch_open_prs("drawmeanelephant/rustodian")
            .await
            .unwrap_err();

        assert!(matches!(err, rustodian_core::CoreError::RateLimitExceeded));
        m.assert_async().await;
    }
}

#[tokio::test]
async fn test_download_and_extract_zip_slip_symlink() {
    let temp_dir = tempfile::tempdir().unwrap();
    let extract_dir = temp_dir.path().join("extract");
    std::fs::create_dir_all(&extract_dir).unwrap();

    // Target directory outside the extraction path (simulating a system dir)
    let system_dir = temp_dir.path().join("system");
    std::fs::create_dir_all(&system_dir).unwrap();

    // Create a malicious tarball in memory
    let mut tar_builder = tar::Builder::new(Vec::new());

    // 1. Add a directory (this is usually stripped as root dir)
    let mut header = tar::Header::new_gnu();
    header.set_size(0);
    header.set_entry_type(tar::EntryType::Directory);
    tar_builder
        .append_data(&mut header, "root/", &[][..])
        .unwrap();

    // 2. Add a symlink named 'foo' pointing to our system_dir
    let mut header = tar::Header::new_gnu();
    header.set_size(0);
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_link_name(system_dir.to_str().unwrap()).unwrap();
    tar_builder
        .append_data(&mut header, "root/foo", &[][..])
        .unwrap();

    // 3. Add a file 'bar' inside the symlinked directory 'foo'
    // If Zip Slip is possible, this will extract to system_dir/bar
    let mut header = tar::Header::new_gnu();
    header.set_size(12);
    header.set_entry_type(tar::EntryType::Regular);
    tar_builder
        .append_data(&mut header, "root/foo/bar", &b"pwned content"[..])
        .unwrap();

    let tar_data = tar_builder.into_inner().unwrap();

    // Gzip it
    use std::io::Write;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&tar_data).unwrap();
    let tar_gz_data = encoder.finish().unwrap();

    // Mock the server
    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock(
            "GET",
            "/drawmeanelephant/rustodian/archive/refs/heads/main.tar.gz",
        )
        .with_status(200)
        .with_body(tar_gz_data)
        .create_async()
        .await;

    let downloader = GithubDownloader::new().with_api_base_url(server.url());

    // Try to download and extract
    let project = rustodian_types::RemoteProject {
        repo_slug: "drawmeanelephant/rustodian".to_string(),
        preserve_patterns: vec![],
    };

    let result = downloader
        .download_and_extract(&project, &extract_dir, &[])
        .await;

    // Ensure it failed with a security error
    println!("Result: {:?}", result);
    assert!(
        result.is_err(),
        "Extraction should have failed due to Zip Slip protection"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Security violation")
            || err_msg.contains("Zip Slip")
            || err_msg.contains("already exists")
            || err_msg.contains("Cannot create a file")
            || err_msg.contains("os error 183")
    );

    // Ensure the file was NOT written to the system dir
    assert!(
        !system_dir.join("bar").exists(),
        "Zip slip attack succeeded!"
    );
}
