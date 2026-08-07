use crate::OutputFormat;
use anyhow::{Context, Result};
use rustodian_core::Custodian;
use rustodian_core::traits::{RemoteDownloader, RemoteProjectStore};
use rustodian_storage::SqliteStore;
use rustodian_types::{RemoteProject, ScanConfig};
use tokio::runtime::Runtime;
use tracing::info;

pub fn execute_add(store: &SqliteStore, repo_slug: &str, preserve: &[String]) -> Result<()> {
    let project = RemoteProject {
        repo_slug: repo_slug.to_string(),
        preserve_patterns: preserve.to_vec(),
    };
    store
        .save_remote_project(&project)
        .context("failed to save remote project")?;
    info!("Added remote project {}", repo_slug);
    println!("Added remote project: {repo_slug}");
    Ok(())
}

pub fn execute_list(store: &SqliteStore, format: &OutputFormat) -> Result<()> {
    let projects = store
        .list_remote_projects()
        .context("failed to list remote projects")?;
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&projects)?);
        }
        OutputFormat::Table => {
            if projects.is_empty() {
                println!("No remote projects tracked.");
                return Ok(());
            }
            let mut table = comfy_table::Table::new();
            table.set_header(vec!["Repo Slug", "Preserve Patterns"]);
            for p in projects {
                let patterns = if p.preserve_patterns.is_empty() {
                    "(none)".to_string()
                } else {
                    p.preserve_patterns.join(", ")
                };
                table.add_row(vec![p.repo_slug, patterns]);
            }
            println!("{table}");
        }
    }
    Ok(())
}

/// Refresh (download, extract, and index) all tracked remote projects.
///
/// # Safety
///
/// This is a pure synchronization operation: it downloads the archive, extracts it,
/// preserves configured files, and updates the project index. It **never** bootstraps
/// the downloaded project or executes code from it (no builds, tests, package managers,
/// virtualenvs, or Justfile recipes). Downloading an untrusted repository must not
/// imply executing it.
pub fn execute_refresh(
    custodian: &Custodian,
    store: &SqliteStore,
    downloader: &dyn RemoteDownloader,
    dest_dir: &std::path::Path,
) -> Result<()> {
    let projects = store
        .list_remote_projects()
        .context("failed to list remote projects")?;
    if projects.is_empty() {
        println!("No remote projects to refresh.");
        return Ok(());
    }
    let rt = Runtime::new().context("failed to create tokio runtime")?;

    for project in projects {
        println!("Refreshing {}...", project.repo_slug);
        let project_dest = dest_dir.join(&project.repo_slug);
        let download_res = rt.block_on(async {
            downloader
                .download_and_extract(&project, &project_dest, &project.preserve_patterns)
                .await
        });

        match download_res {
            Ok(()) => {
                println!("Successfully refreshed {}", project.repo_slug);
                println!("Scanning project {}...", project.repo_slug);
                let scan_config = ScanConfig {
                    max_depth: rustodian_types::scan::DEFAULT_MAX_DEPTH,
                    follow_symlinks: false,
                    exclude_patterns: vec![],
                };
                match custodian.scan(&project_dest, &scan_config) {
                    Ok(report) => {
                        println!("Scan completed. Found {} projects.", report.projects_found);
                        println!(
                            "Index updated for {} (download, extract, and scan only — no code was executed).",
                            project.repo_slug
                        );
                    }
                    Err(e) => {
                        println!("Failed to scan project {}: {}", project.repo_slug, e);
                    }
                }
            }
            Err(e) => println!("Failed to refresh {}: {}", project.repo_slug, e),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustodian_core::runner::CommandSpec;
    use rustodian_core::traits::{CommandRunner, GitInspector, ProjectStore, RunningProcess};
    use rustodian_scanner::FsScanner;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    /// Fake downloader that simulates extracting an untrusted Rust project.
    /// Mimics the real downloader's contract: it extracts archive contents
    /// directly into `dest_dir` (which `execute_refresh` has already scoped to
    /// the repo slug). No network and no package tools are involved.
    struct FakeDownloader;

    #[async_trait::async_trait]
    impl RemoteDownloader for FakeDownloader {
        async fn download_and_extract(
            &self,
            _project: &RemoteProject,
            dest_dir: &Path,
            _preserve_patterns: &[String],
        ) -> Result<(), rustodian_core::CoreError> {
            std::fs::create_dir_all(dest_dir)
                .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;
            std::fs::write(dest_dir.join("Cargo.toml"), "[package]\nname = \"fake\"\n")
                .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;
            Ok(())
        }
    }

    struct MockRunningProcess {
        exit_code: Option<i32>,
    }

    impl RunningProcess for MockRunningProcess {
        fn id(&self) -> u32 {
            1
        }
        fn wait(&mut self) -> Result<Option<i32>, rustodian_core::CoreError> {
            Ok(self.exit_code)
        }
        fn try_wait(&mut self) -> Result<Option<Option<i32>>, rustodian_core::CoreError> {
            Ok(Some(self.exit_code))
        }
        fn kill(&mut self) -> Result<(), rustodian_core::CoreError> {
            Ok(())
        }
        fn stdout(&mut self) -> Option<Box<dyn std::io::Read + Send + Sync>> {
            Some(Box::new(std::io::Cursor::new(Vec::new())))
        }
        fn stderr(&mut self) -> Option<Box<dyn std::io::Read + Send + Sync>> {
            Some(Box::new(std::io::Cursor::new(Vec::new())))
        }
    }

    /// Records every program the `CommandRunner` is asked to spawn.
    struct RecordingRunner {
        spawned: Arc<Mutex<Vec<String>>>,
    }

    impl CommandRunner for RecordingRunner {
        fn spawn(
            &self,
            spec: CommandSpec,
        ) -> Result<Box<dyn RunningProcess>, rustodian_core::CoreError> {
            self.spawned.lock().unwrap().push(spec.program);
            Ok(Box::new(MockRunningProcess { exit_code: Some(0) }))
        }
    }

    struct MockGit;

    impl GitInspector for MockGit {
        fn inspect(
            &self,
            _path: &Path,
        ) -> Result<Option<rustodian_types::VcsInfo>, rustodian_core::CoreError> {
            Ok(None)
        }
        fn get_dirty_files(
            &self,
            _path: &Path,
        ) -> Result<Vec<std::path::PathBuf>, rustodian_core::CoreError> {
            Ok(vec![])
        }
    }

    /// Refreshing a remote project must download, extract, and index it —
    /// never bootstrap or execute any code from the downloaded repository.
    #[test]
    fn refresh_completes_without_invoking_command_runner() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        store.migrate().unwrap();
        store
            .save_remote_project(&RemoteProject {
                repo_slug: "octocat/Hello-World".to_string(),
                preserve_patterns: vec![],
            })
            .unwrap();

        let spawned = Arc::new(Mutex::new(Vec::new()));
        let custodian = Custodian::new(
            Box::new(store.clone()),
            Box::new(FsScanner),
            Box::new(MockGit),
            Box::new(RecordingRunner {
                spawned: spawned.clone(),
            }),
        );

        let result = execute_refresh(&custodian, &store, &FakeDownloader, tmp.path());

        // Refresh completes successfully.
        assert!(result.is_ok(), "refresh should complete: {result:?}");

        // The downloaded project was extracted and indexed at the refresh destination.
        let expected_path = tmp.path().join("octocat/Hello-World");
        let projects = store.list_projects().unwrap();
        assert_eq!(
            projects.len(),
            1,
            "refresh must index the downloaded project"
        );
        assert_eq!(
            projects[0].path, expected_path,
            "indexed path must be exactly the refresh destination"
        );

        // The CommandRunner must never have been invoked.
        let spawned = spawned.lock().unwrap();
        assert!(
            spawned.is_empty(),
            "refresh must not execute any commands, got: {spawned:?}"
        );
    }
}
