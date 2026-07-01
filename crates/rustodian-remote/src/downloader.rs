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
        let mut response = self
            .client
            .get(format!(
                "https://github.com/{}/archive/refs/heads/main.tar.gz",
                project.repo_slug
            ))
            .send()
            .await
            .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            response = self
                .client
                .get(format!(
                    "https://github.com/{}/archive/refs/heads/master.tar.gz",
                    project.repo_slug
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

            if preserve_set.is_match(stripped_path) {
                debug!("Preserving file matching pattern: {:?}", stripped_path);
                continue;
            }

            let dest_path = dest_dir.join(stripped_path);
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;
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
    async fn fetch_open_prs(&self, repo_slug: &str) -> Result<Vec<rustodian_types::PullRequest>, rustodian_core::CoreError> {
        let url = format!("{}/repos/{}/pulls?state=open", self.api_base_url, repo_slug);

        let mut req = self.client.get(&url).header(reqwest::header::USER_AGENT, "rustodian");

        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            req = req.bearer_auth(token);
        }

        let response = req
            .send()
            .await
            .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;

        if response.status() == reqwest::StatusCode::FORBIDDEN
            && let Some(limit) = response.headers().get("X-RateLimit-Remaining")
            && limit.to_str().unwrap_or("") == "0" {
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

        Ok(gh_prs.into_iter().map(|pr| rustodian_types::PullRequest {
            number: pr.number,
            title: pr.title,
            author: pr.user.login,
            branch: pr.head.ref_name,
            url: pr.html_url,
            updated_at: pr.updated_at,
            is_draft: pr.draft,
        }).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustodian_core::traits::PullRequestFetcher;
    use mockito::Server;

    #[tokio::test]
    async fn test_fetch_open_prs_success() {
        let mut server = Server::new_async().await;

        let m = server.mock("GET", "/repos/drawmeanelephant/rustodian/pulls?state=open")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"
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
            "#)
            .create_async()
            .await;

        let downloader = GithubDownloader::new().with_api_base_url(server.url());
        let prs = downloader.fetch_open_prs("drawmeanelephant/rustodian").await.unwrap();

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

        let m = server.mock("GET", "/repos/drawmeanelephant/rustodian/pulls?state=open")
            .with_status(403)
            .with_header("X-RateLimit-Remaining", "0")
            .create_async()
            .await;

        let downloader = GithubDownloader::new().with_api_base_url(server.url());
        let err = downloader.fetch_open_prs("drawmeanelephant/rustodian").await.unwrap_err();

        assert!(matches!(err, rustodian_core::CoreError::RateLimitExceeded));
        m.assert_async().await;
    }
}
