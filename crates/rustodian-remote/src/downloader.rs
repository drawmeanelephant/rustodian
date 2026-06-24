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
}

impl GithubDownloader {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
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
