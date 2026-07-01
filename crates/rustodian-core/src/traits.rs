use std::path::Path;

use rustodian_types::{Project, ProjectId, ProjectLog, ScanConfig, ScanId, ScanRecord, VcsInfo};

use crate::error::CoreError;

/// A discovered but not-yet-stored project from a scan.
#[derive(Debug, Clone)]
pub struct DiscoveredProject {
    pub name: String,
    pub path: std::path::PathBuf,
    pub languages: Vec<rustodian_types::LanguageDetection>,
    pub commands: Vec<rustodian_types::ProjectCommand>,
}

/// Contract for project persistence.
///
/// Implementors provide the actual storage mechanism (e.g., `SQLite`).
pub trait ProjectStore: Send + Sync {
    /// Persist a project, returning its ID.
    fn save_project(&self, project: &Project) -> Result<ProjectId, CoreError>;

    /// Retrieve a project by ID.
    fn get_project(&self, id: &ProjectId) -> Result<Option<Project>, CoreError>;

    /// List all known projects.
    fn list_projects(&self) -> Result<Vec<Project>, CoreError>;

    /// Delete a project by ID. Returns true if it existed.
    fn delete_project(&self, id: &ProjectId) -> Result<bool, CoreError>;

    /// Find a project by its filesystem path.
    fn find_by_path(&self, path: &Path) -> Result<Option<Project>, CoreError>;

    /// Record a scan operation.
    fn save_scan(&self, scan: &ScanRecord) -> Result<ScanId, CoreError>;

    /// Get the most recent scan record.
    fn get_latest_scan(&self) -> Result<Option<ScanRecord>, CoreError>;

    /// Persist a command execution log.
    fn save_log(&self, log: &ProjectLog) -> Result<(), CoreError>;

    /// List execution logs for a project, ordered by most recent first.
    fn list_logs(&self, project_id: &str, limit: usize) -> Result<Vec<ProjectLog>, CoreError>;

    /// Get a specific log entry by ID.
    fn get_log(&self, id: &str) -> Result<Option<ProjectLog>, CoreError>;

    /// Get the most recent log entry for a project.
    fn get_latest_log(&self, project_id: &str) -> Result<Option<ProjectLog>, CoreError>;
}

/// Contract for filesystem project discovery.
///
/// Implementors walk the filesystem to find software projects.
pub trait ProjectScanner: Send + Sync {
    /// Scan a directory tree for software projects.
    fn scan(&self, root: &Path, config: &ScanConfig) -> Result<Vec<DiscoveredProject>, CoreError>;
}

/// Contract for VCS inspection.
///
/// Implementors extract version control information from a project directory.
pub trait GitInspector: Send + Sync {
    /// Inspect a project directory for git information.
    /// Returns `None` if the directory is not a git repository.
    fn inspect(&self, project_path: &Path) -> Result<Option<VcsInfo>, CoreError>;

    /// Query the repository status for untracked, modified, or staged files.
    /// Returns an empty vec if the path is not a git repository.
    fn get_dirty_files(&self, project_path: &Path) -> Result<Vec<std::path::PathBuf>, CoreError>;
}
use rustodian_types::RemoteProject;

#[async_trait::async_trait]
pub trait RemoteDownloader: Send + Sync {
    async fn download_and_extract(
        &self,
        project: &RemoteProject,
        dest_dir: &std::path::Path,
        preserve_patterns: &[String],
    ) -> Result<(), crate::error::CoreError>;
}

pub trait RemoteProjectStore: Send + Sync {
    fn save_remote_project(&self, project: &RemoteProject) -> Result<(), crate::error::CoreError>;
    fn list_remote_projects(&self) -> Result<Vec<RemoteProject>, crate::error::CoreError>;
    fn delete_remote_project(&self, repo_slug: &str) -> Result<bool, crate::error::CoreError>;
}

use crate::runner::CommandSpec;

pub trait RunningProcess: Send + Sync {
    fn id(&self) -> u32;
    fn wait(&mut self) -> Result<Option<i32>, CoreError>;
    fn try_wait(&mut self) -> Result<Option<Option<i32>>, CoreError>;
    fn kill(&mut self) -> Result<(), CoreError>;
    fn stdout(&mut self) -> Option<Box<dyn std::io::Read + Send + Sync>>;
    fn stderr(&mut self) -> Option<Box<dyn std::io::Read + Send + Sync>>;
}

#[async_trait::async_trait]
pub trait PullRequestFetcher: Send + Sync {
    async fn fetch_open_prs(
        &self,
        repo_slug: &str,
    ) -> Result<Vec<rustodian_types::PullRequest>, CoreError>;
}

pub trait CommandRunner: Send + Sync {
    fn spawn(&self, spec: CommandSpec) -> Result<Box<dyn RunningProcess>, CoreError>;
}
