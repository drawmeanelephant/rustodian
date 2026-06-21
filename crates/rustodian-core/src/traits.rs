//! Core trait definitions — the contracts of Rustodian.
//!
//! Infrastructure crates implement these traits.
//! The [`Custodian`](crate::custodian::Custodian) orchestrator consumes them via `Box<dyn Trait>`.

use std::path::Path;

use rustodian_types::{
    Project, ProjectId, ScanConfig, ScanId, ScanRecord, VcsInfo,
};

use crate::error::CoreError;

/// A discovered but not-yet-stored project from a scan.
#[derive(Debug, Clone)]
pub struct DiscoveredProject {
    pub name: String,
    pub path: std::path::PathBuf,
    pub languages: Vec<rustodian_types::LanguageDetection>,
}

/// Contract for project persistence.
///
/// Implementors provide the actual storage mechanism (e.g., SQLite).
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
}

/// Contract for filesystem project discovery.
///
/// Implementors walk the filesystem to find software projects.
pub trait ProjectScanner: Send + Sync {
    /// Scan a directory tree for software projects.
    fn scan(
        &self,
        root: &Path,
        config: &ScanConfig,
    ) -> Result<Vec<DiscoveredProject>, CoreError>;
}

/// Contract for VCS inspection.
///
/// Implementors extract version control information from a project directory.
pub trait GitInspector: Send + Sync {
    /// Inspect a project directory for git information.
    /// Returns `None` if the directory is not a git repository.
    fn inspect(&self, project_path: &Path) -> Result<Option<VcsInfo>, CoreError>;
}
