//! The Custodian — Rustodian's core orchestrator.
//!
//! Coordinates scanning, storage, and git inspection through trait objects.
//! Uses `Box<dyn Trait>` for simplicity — dynamic dispatch overhead is
//! irrelevant when every call hits the filesystem or database.

use std::path::Path;
use std::process::Command;

use tracing::{info, instrument};

use rustodian_types::{Project, ProjectId, ScanConfig, ScanId, ScanRecord};

use crate::error::CoreError;
use crate::traits::{GitInspector, ProjectScanner, ProjectStore};

/// Report from a scan operation.
#[derive(Debug)]
pub struct ScanReport {
    pub scan_id: ScanId,
    pub projects_found: usize,
    pub projects_new: usize,
    pub projects_updated: usize,
}

/// Overall status summary.
#[derive(Debug)]
pub struct StatusReport {
    pub total_projects: usize,
    pub last_scan: Option<ScanRecord>,
    pub languages: Vec<(String, usize)>,
}

/// The core orchestrator for Rustodian.
///
/// Wires together storage, scanning, and git inspection.
/// This is the primary API surface for any frontend (CLI, GUI, etc.).
pub struct Custodian {
    store: Box<dyn ProjectStore>,
    #[allow(dead_code)]
    scanner: Box<dyn ProjectScanner>,
    #[allow(dead_code)]
    git: Box<dyn GitInspector>,
}

impl Custodian {
    /// Create a new Custodian with the given infrastructure implementations.
    pub fn new(
        store: Box<dyn ProjectStore>,
        scanner: Box<dyn ProjectScanner>,
        git: Box<dyn GitInspector>,
    ) -> Self {
        Self {
            store,
            scanner,
            git,
        }
    }

    /// Scan a directory tree for projects and store the results.
    #[instrument(skip(self), fields(root = %root.display()))]
    pub fn scan(&self, root: &Path, config: &ScanConfig) -> Result<ScanReport, CoreError> {
        info!("Starting scan");
        let start_time = chrono::Utc::now();

        let discovered = self.scanner.scan(root, config)?;

        let mut projects_new = 0;
        let mut projects_updated = 0;

        for d in &discovered {
            let vcs = self.git.inspect(&d.path)?;
            let now = chrono::Utc::now();

            let project = if let Some(mut existing) = self.store.find_by_path(&d.path)? {
                existing.name.clone_from(&d.name);
                existing.languages.clone_from(&d.languages);
                existing.metadata.commands.clone_from(&d.commands);
                existing.vcs = vcs;
                existing.last_scanned_at = Some(now);
                projects_updated += 1;
                existing
            } else {
                projects_new += 1;

                let mut metadata = rustodian_types::ProjectMetadata::default();
                metadata.commands.clone_from(&d.commands);

                Project {
                    id: ProjectId::new(),
                    name: d.name.clone(),
                    path: d.path.clone(),
                    languages: d.languages.clone(),
                    vcs,
                    discovered_at: now,
                    last_scanned_at: Some(now),
                    metadata,
                }
            };

            self.store.save_project(&project)?;
        }

        let scan_record = ScanRecord {
            id: ScanId::new(),
            root_path: root.to_path_buf(),
            started_at: start_time,
            completed_at: Some(chrono::Utc::now()),
            projects_found: discovered.len(),
            status: rustodian_types::ScanStatus::Completed,
        };

        let scan_id = self.store.save_scan(&scan_record)?;

        Ok(ScanReport {
            scan_id,
            projects_found: discovered.len(),
            projects_new,
            projects_updated,
        })
    }

    /// Finds a project and executes the given command name if discovered.
    pub fn run_command(&self, project_query: &str, command_name: &str) -> Result<(), CoreError> {
        let project = self
            .find_project(project_query)?
            .ok_or_else(|| CoreError::Storage(format!("Project not found: {project_query}")))?;

        let cmd = project
            .metadata
            .commands
            .iter()
            .find(|c| c.name == command_name)
            .ok_or_else(|| {
                CoreError::Storage(format!(
                    "Command '{}' not found in project '{}'",
                    command_name, project.name
                ))
            })?;

        // Note: the command string might contain spaces (e.g., "npm run build")
        // we'll use sh -c to execute it cleanly on unix systems, which is adequate for now.
        let status = Command::new("sh")
            .arg("-c")
            .arg(&cmd.command)
            .current_dir(&project.path)
            .status()
            .map_err(|e| CoreError::Storage(format!("Failed to execute command: {e}")))?;

        if !status.success() {
            return Err(CoreError::Storage(format!(
                "Command exited with non-zero status: {status}"
            )));
        }

        Ok(())
    }

    /// List all tracked projects.
    #[instrument(skip(self))]
    pub fn list(&self) -> Result<Vec<Project>, CoreError> {
        info!("Listing projects");
        self.store.list_projects()
    }

    /// Get overall observatory status.
    #[instrument(skip(self))]
    pub fn status(&self) -> Result<StatusReport, CoreError> {
        info!("Getting status");
        todo!("Aggregate status from store")
    }

    /// Get detailed info about a specific project.
    #[instrument(skip(self))]
    pub fn info(&self, id: &ProjectId) -> Result<Project, CoreError> {
        info!(%id, "Getting project info");
        self.store
            .get_project(id)?
            .ok_or_else(|| CoreError::ProjectNotFound(id.clone()))
    }

    /// Find a project by name or ID string.
    #[instrument(skip(self))]
    pub fn find_project(&self, query: &str) -> Result<Option<Project>, CoreError> {
        let all = self.store.list_projects()?;
        if let Some(p) = all.iter().find(|p| p.name == query) {
            return Ok(Some(p.clone()));
        }
        if let Some(p) = all.iter().find(|p| p.id.to_string() == query) {
            return Ok(Some(p.clone()));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    // Future: mock-based tests for Custodian orchestration
}
