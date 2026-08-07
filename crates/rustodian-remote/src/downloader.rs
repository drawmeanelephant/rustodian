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

        // Ensure the destination directory exists and resolve it once up front.
        // Never fall back to an uncanonicalized path: every extracted entry is
        // validated against this resolved root.
        fs::create_dir_all(dest_dir).map_err(|e| {
            rustodian_core::CoreError::Internal(format!(
                "Failed to create extraction destination {}: {e}",
                dest_dir.display()
            ))
        })?;
        let canonical_dest = dest_dir.canonicalize().map_err(|e| {
            rustodian_core::CoreError::Internal(format!(
                "Failed to canonicalize extraction destination {}: {e}",
                dest_dir.display()
            ))
        })?;

        let entries = archive
            .entries()
            .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;

        for entry in entries {
            let mut entry =
                entry.map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;
            let path = entry
                .path()
                .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;

            // Refuse archive links and exotic entry types outright. Extraction
            // only supports regular files and directories; a single link entry
            // rejects the entire archive before anything is written.
            match entry.header().entry_type() {
                tar::EntryType::Regular | tar::EntryType::Directory => {}
                tar::EntryType::Symlink => {
                    return Err(rustodian_core::CoreError::Internal(format!(
                        "Security violation: archive entry {path:?} is a symbolic link; \
                         symbolic links are not supported for extraction"
                    )));
                }
                tar::EntryType::Link => {
                    return Err(rustodian_core::CoreError::Internal(format!(
                        "Security violation: archive entry {path:?} is a hard link; \
                         hard links are not supported for extraction"
                    )));
                }
                other => {
                    return Err(rustodian_core::CoreError::Internal(format!(
                        "Security violation: archive entry {path:?} has unsupported entry \
                         type {other:?}"
                    )));
                }
            }

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

#[cfg(test)]
mod extraction_tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    struct ExtractionFixture {
        _temp_dir: tempfile::TempDir,
        extract_dir: PathBuf,
        outside_dir: PathBuf,
        temp_root: PathBuf,
    }

    impl ExtractionFixture {
        fn new() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            let extract_dir = temp_dir.path().join("extract");
            let outside_dir = temp_dir.path().join("outside");
            std::fs::create_dir_all(&extract_dir).unwrap();
            std::fs::create_dir_all(&outside_dir).unwrap();
            let temp_root = temp_dir.path().to_path_buf();
            Self {
                _temp_dir: temp_dir,
                extract_dir,
                outside_dir,
                temp_root,
            }
        }
    }

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    fn append_dir(builder: &mut tar::Builder<Vec<u8>>, path: &str) {
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_entry_type(tar::EntryType::Directory);
        header.set_mode(0o755);
        builder.append_data(&mut header, path, &[][..]).unwrap();
    }

    fn append_regular(builder: &mut tar::Builder<Vec<u8>>, path: &str, data: &[u8]) {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_mode(0o644);
        builder.append_data(&mut header, path, data).unwrap();
    }

    fn append_symlink(builder: &mut tar::Builder<Vec<u8>>, path: &str, target: &str) {
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_link_name(target).unwrap();
        builder.append_data(&mut header, path, &[][..]).unwrap();
    }

    fn append_hardlink(builder: &mut tar::Builder<Vec<u8>>, path: &str, target: &str) {
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_entry_type(tar::EntryType::Link);
        header.set_link_name(target).unwrap();
        builder.append_data(&mut header, path, &[][..]).unwrap();
    }

    /// Hand-crafts a single-entry ustar archive with an unvalidated entry name so
    /// tests can exercise paths that `tar::Builder` refuses to write (e.g. `..`).
    fn raw_tar_entry(name: &str, data: &[u8]) -> Vec<u8> {
        let mut block = [0_u8; 512];
        let name_bytes = name.as_bytes();
        block[..name_bytes.len()].copy_from_slice(name_bytes);
        block[100..108].copy_from_slice(b"0000644\0");
        block[108..116].copy_from_slice(b"0000000\0");
        block[116..124].copy_from_slice(b"0000000\0");
        let size = format!("{:011o}\0", data.len());
        block[124..136].copy_from_slice(size.as_bytes());
        block[136..148].copy_from_slice(b"00000000000\0");
        block[156] = b'0';
        block[257..263].copy_from_slice(b"ustar\0");
        block[263..265].copy_from_slice(b"00");

        block[148..156].copy_from_slice(b"        ");
        let sum: u32 = block.iter().map(|&b| u32::from(b)).sum();
        let checksum = format!("{:06o}\0 ", sum);
        block[148..156].copy_from_slice(checksum.as_bytes());

        let mut out = block.to_vec();
        let padded_len = data.len().next_multiple_of(512);
        out.extend_from_slice(data);
        out.resize(padded_len, 0);
        out
    }

    async fn download_into(
        fixture: &ExtractionFixture,
        tar_gz: Vec<u8>,
        preserve: &[String],
    ) -> Result<(), rustodian_core::CoreError> {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock(
                "GET",
                "/drawmeanelephant/rustodian/archive/refs/heads/main.tar.gz",
            )
            .with_status(200)
            .with_body(tar_gz)
            .create_async()
            .await;

        let downloader = GithubDownloader::new().with_api_base_url(server.url());
        let project = rustodian_types::RemoteProject {
            repo_slug: "drawmeanelephant/rustodian".to_string(),
            preserve_patterns: vec![],
        };
        downloader
            .download_and_extract(&project, &fixture.extract_dir, preserve)
            .await
    }

    #[tokio::test]
    async fn test_download_and_extract_normal_archive() {
        let fixture = ExtractionFixture::new();
        let mut builder = tar::Builder::new(Vec::new());
        append_dir(&mut builder, "repo/");
        append_dir(&mut builder, "repo/src/");
        append_regular(&mut builder, "repo/src/main.rs", b"fn main() {}\n");
        append_regular(&mut builder, "repo/README.md", b"# readme\n");
        let tar_gz = gzip(&builder.into_inner().unwrap());

        let result = download_into(&fixture, tar_gz, &[]).await;
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            std::fs::read_to_string(fixture.extract_dir.join("src/main.rs")).unwrap(),
            "fn main() {}\n"
        );
        assert_eq!(
            std::fs::read_to_string(fixture.extract_dir.join("README.md")).unwrap(),
            "# readme\n"
        );
        assert_eq!(
            std::fs::read_dir(&fixture.outside_dir).unwrap().count(),
            0,
            "no files may be written outside the destination"
        );
    }

    #[tokio::test]
    async fn test_download_and_extract_rejects_parent_traversal() {
        let fixture = ExtractionFixture::new();
        let tar_gz = gzip(&raw_tar_entry("repo/../escape.txt", b"pwned"));

        let result = download_into(&fixture, tar_gz, &[]).await;
        let err = result.expect_err("extraction must reject parent traversal");
        assert!(err.to_string().contains("Path traversal"), "{err}");
        assert!(
            !fixture.temp_root.join("escape.txt").exists(),
            "traversal entry must not be written anywhere"
        );
        assert_eq!(std::fs::read_dir(&fixture.outside_dir).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn test_download_and_extract_rejects_symlink_entry() {
        let fixture = ExtractionFixture::new();
        let mut builder = tar::Builder::new(Vec::new());
        append_dir(&mut builder, "repo/");
        append_regular(&mut builder, "repo/ok.txt", b"ok");
        append_symlink(
            &mut builder,
            "repo/link",
            fixture.outside_dir.to_str().unwrap(),
        );
        let tar_gz = gzip(&builder.into_inner().unwrap());

        let result = download_into(&fixture, tar_gz, &[]).await;
        let err = result.expect_err("extraction must reject symbolic links");
        assert!(err.to_string().contains("symbolic link"), "{err}");
        assert!(
            !fixture.extract_dir.join("link").exists(),
            "symlink must not be created"
        );
        assert_eq!(std::fs::read_dir(&fixture.outside_dir).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn test_download_and_extract_rejects_hardlink_entry() {
        let fixture = ExtractionFixture::new();
        let mut builder = tar::Builder::new(Vec::new());
        append_dir(&mut builder, "repo/");
        append_regular(&mut builder, "repo/original.txt", b"data");
        append_hardlink(&mut builder, "repo/hardlink.txt", "repo/original.txt");
        let tar_gz = gzip(&builder.into_inner().unwrap());

        let result = download_into(&fixture, tar_gz, &[]).await;
        let err = result.expect_err("extraction must reject hard links");
        assert!(err.to_string().contains("hard link"), "{err}");
        assert!(
            !fixture.extract_dir.join("hardlink.txt").exists(),
            "hard link must not be created"
        );
        assert_eq!(std::fs::read_dir(&fixture.outside_dir).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn test_download_and_extract_symlink_then_nested_file_cannot_escape() {
        let fixture = ExtractionFixture::new();
        let mut builder = tar::Builder::new(Vec::new());
        append_dir(&mut builder, "repo/");
        append_symlink(
            &mut builder,
            "repo/foo",
            fixture.outside_dir.to_str().unwrap(),
        );
        append_regular(&mut builder, "repo/foo/bar", b"pwned content");
        let tar_gz = gzip(&builder.into_inner().unwrap());

        let result = download_into(&fixture, tar_gz, &[]).await;
        assert!(
            result.is_err(),
            "extraction must reject the symlink before any nested file is written"
        );
        assert!(
            !fixture.outside_dir.join("bar").exists(),
            "zip slip attack must not write outside the destination"
        );
        assert!(!fixture.extract_dir.join("foo").exists());
    }

    #[tokio::test]
    async fn test_download_and_extract_preserves_matching_files() {
        let fixture = ExtractionFixture::new();
        std::fs::write(fixture.extract_dir.join(".env"), "keep me").unwrap();

        let mut builder = tar::Builder::new(Vec::new());
        append_dir(&mut builder, "repo/");
        append_regular(&mut builder, "repo/.env", b"from archive");
        append_regular(&mut builder, "repo/README.md", b"# readme\n");
        let tar_gz = gzip(&builder.into_inner().unwrap());

        let preserve = vec![".env".to_string()];
        let result = download_into(&fixture, tar_gz, &preserve).await;
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            std::fs::read_to_string(fixture.extract_dir.join(".env")).unwrap(),
            "keep me"
        );
        assert_eq!(
            std::fs::read_to_string(fixture.extract_dir.join("README.md")).unwrap(),
            "# readme\n"
        );
    }
}
