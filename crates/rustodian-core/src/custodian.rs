//! The Custodian — Rustodian's core orchestrator.
//!
//! Coordinates scanning, storage, and git inspection through trait objects.
//! Uses `Box<dyn Trait>` for simplicity — dynamic dispatch overhead is
//! irrelevant when every call hits the filesystem or database.

use std::path::Path;

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
                existing.name = d.name.clone();
                existing.languages = d.languages.clone();
                existing.vcs = vcs;
                existing.last_scanned_at = Some(now);
                projects_updated += 1;
                existing
            } else {
                projects_new += 1;
                Project {
                    id: rustodian_types::ProjectId::new(),
                    name: d.name.clone(),
                    path: d.path.clone(),
                    languages: d.languages.clone(),
                    vcs,
                    discovered_at: now,
                    last_scanned_at: Some(now),
                    metadata: rustodian_types::ProjectMetadata::default(),
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
}

#[cfg(test)]
mod tests {
    // Future: mock-based tests for Custodian orchestration
}
