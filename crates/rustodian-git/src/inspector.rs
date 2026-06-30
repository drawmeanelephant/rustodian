//! Git2-based implementation of [`GitInspector`].

use std::path::Path;

use git2::{Repository, StatusOptions};
use tracing::{debug, instrument};

use rustodian_core::CoreError;
use rustodian_core::traits::GitInspector;
use rustodian_types::{CommitInfo, VcsInfo, VcsType};

/// Git inspector using libgit2.
#[derive(Debug, Default)]
pub struct Git2Inspector;

impl GitInspector for Git2Inspector {
    #[instrument(skip(self), fields(path = %project_path.display()))]
    fn inspect(&self, project_path: &Path) -> Result<Option<VcsInfo>, CoreError> {
        debug!("Inspecting git repository");

        let Ok(repo) = Repository::open(project_path) else {
            return Ok(None);
        };

        let branch = match repo.head() {
            Ok(head) => {
                if head.is_branch() {
                    head.shorthand().ok().map(std::string::ToString::to_string)
                } else {
                    None
                }
            }
            Err(_) => None,
        };

        let remote_url = repo
            .find_remote("origin")
            .ok()
            .and_then(|r| r.url().ok().map(std::string::ToString::to_string));

        let is_dirty = self
            .get_dirty_files(project_path)
            .is_ok_and(|files| !files.is_empty());

        let last_commit = match repo.head().and_then(|head| head.peel_to_commit()) {
            Ok(commit) => {
                let author = commit.author();
                let time = chrono::DateTime::from_timestamp(commit.time().seconds(), 0)
                    .unwrap_or_default();

                Some(CommitInfo {
                    sha: commit.id().to_string(),
                    message: commit
                        .summary()
                        .unwrap_or(Some(""))
                        .unwrap_or("")
                        .to_string(),
                    author: author.name().unwrap_or("").to_string(),
                    timestamp: time,
                })
            }
            Err(_) => None,
        };

        Ok(Some(VcsInfo {
            vcs_type: VcsType::Git,
            branch,
            remote_url,
            is_dirty,
            last_commit,
        }))
    }

    fn get_dirty_files(&self, project_path: &Path) -> Result<Vec<std::path::PathBuf>, CoreError> {
        let repo = Repository::open(project_path).map_err(|e| CoreError::Git(e.to_string()))?;

        let mut status_opts = StatusOptions::new();
        status_opts.include_untracked(true);
        let statuses = repo
            .statuses(Some(&mut status_opts))
            .map_err(|e| CoreError::Git(e.to_string()))?;

        let mut dirty_files = Vec::new();
        for entry in statuses.iter() {
            if let Ok(path) = entry.path() {
                dirty_files.push(std::path::PathBuf::from(path));
            }
        }
        Ok(dirty_files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_inspect_not_a_repo() {
        let dir = TempDir::new().unwrap();
        let inspector = Git2Inspector;
        let result = inspector.inspect(dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_inspect_repo() {
        let dir = TempDir::new().unwrap();

        let _repo = Repository::init(dir.path()).unwrap();

        let inspector = Git2Inspector;
        let result = inspector.inspect(dir.path()).unwrap();

        assert!(result.is_some());
        let info = result.unwrap();
        assert_eq!(info.vcs_type, VcsType::Git);
        assert!(!info.is_dirty);
        assert!(info.branch.is_none());
    }

    #[test]
    fn test_get_dirty_files_clean_repo() {
        let dir = TempDir::new().unwrap();
        let _repo = Repository::init(dir.path()).unwrap();

        let inspector = Git2Inspector;
        let dirty = inspector.get_dirty_files(dir.path()).unwrap();
        assert!(dirty.is_empty());
    }

    #[test]
    fn test_get_dirty_files_with_untracked() {
        let dir = TempDir::new().unwrap();
        let _repo = Repository::init(dir.path()).unwrap();

        std::fs::write(dir.path().join("new_file.txt"), "hello").unwrap();

        let inspector = Git2Inspector;
        let dirty = inspector.get_dirty_files(dir.path()).unwrap();
        assert_eq!(dirty.len(), 1);
        assert_eq!(dirty[0], std::path::PathBuf::from("new_file.txt"));
    }
}
