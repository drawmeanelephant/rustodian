//! The Custodian — Rustodian's core orchestrator.
//!
//! Coordinates scanning, storage, and git inspection through trait objects.
//! Uses `Box<dyn Trait>` for simplicity — dynamic dispatch overhead is
//! irrelevant when every call hits the filesystem or database.

use std::collections::HashMap;
use std::path::Path;

use tracing::{info, instrument};

use rustodian_types::{Project, ProjectId, ProjectLog, ScanConfig, ScanId, ScanRecord};

use crate::error::CoreError;
use crate::runner::CommandSpec;
use crate::traits::{CommandRunner, GitInspector, ProjectScanner, ProjectStore};

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
    runner: Box<dyn CommandRunner>,
}

impl Custodian {
    /// Create a new Custodian with the given infrastructure implementations.
    pub fn new(
        store: Box<dyn ProjectStore>,
        scanner: Box<dyn ProjectScanner>,
        git: Box<dyn GitInspector>,
        runner: Box<dyn CommandRunner>,
    ) -> Self {
        Self {
            store,
            scanner,
            git,
            runner,
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

        self.run_and_log_command(&project, command_name, &cmd.command, cmd.use_shell, HashMap::new())?;
        Ok(())
    }

    /// Runs a command for a project, streams output in real-time, and logs it to the database.
    pub fn run_and_log_command(
        &self,
        project: &Project,
        command_name: &str,
        program: &str,
        use_shell: bool,
        env: HashMap<String, String>,
    ) -> Result<Option<i32>, CoreError> {
        let spec = CommandSpec {
            program: program.to_string(),
            args: vec![],
            working_dir: project.path.clone(),
            env,
            use_shell,
            capture_output: true,
        };

        let mut child = self.runner.spawn(spec)?;

        let log_buffer = crate::log_buffer::LogBuffer::new();

        let stdout_log = log_buffer.clone();
        let mut stdout_handle = None;
        if let Some(so) = child.stdout() {
            stdout_handle = Some(std::thread::spawn(move || {
                use std::io::{BufRead, BufReader};
                let reader = BufReader::new(so);
                for line in reader.lines().map_while(Result::ok) {
                    println!("{line}");
                    stdout_log.push_line(line);
                }
            }));
        }

        let stderr_log = log_buffer.clone();
        let mut stderr_handle = None;
        if let Some(se) = child.stderr() {
            stderr_handle = Some(std::thread::spawn(move || {
                use std::io::{BufRead, BufReader};
                let reader = BufReader::new(se);
                for line in reader.lines().map_while(Result::ok) {
                    eprintln!("{line}");
                    stderr_log.push_line(line);
                }
            }));
        }

        let exit_code = child.wait()?;

        if let Some(h) = stdout_handle {
            let _ = h.join();
        }
        if let Some(h) = stderr_handle {
            let _ = h.join();
        }

        let full_log = log_buffer.snapshot();

        let log_record = ProjectLog {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: project.id.to_string(),
            command_name: command_name.to_string(),
            exit_code,
            log_text: full_log,
            run_at: chrono::Utc::now(),
        };

        self.store.save_log(&log_record)?;

        Ok(exit_code)
    }

    /// Automatically bootstrap (environment setup/isolation) and verify (run test suite) a project.
    pub fn bootstrap_and_verify(&self, project_id: &ProjectId) -> Result<(), CoreError> {
        let project = self.info(project_id)?;
        let bootstrapper = crate::bootstrapper::ProjectBootstrapper::new(self);
        bootstrapper.bootstrap_and_verify(&project)
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
        let projects = self.store.list_projects()?;
        let last_scan = self.store.get_latest_scan()?;

        let mut lang_counts = HashMap::new();
        for p in &projects {
            if let Some(primary) = p.languages.first() {
                *lang_counts.entry(primary.language.clone()).or_insert(0) += 1;
            }
        }

        let mut languages: Vec<(String, usize)> = lang_counts
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        // Sort by count descending, then name alphabetically
        languages.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        Ok(StatusReport {
            total_projects: projects.len(),
            last_scan,
            languages,
        })
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

    /// Find a project by its filesystem path.
    #[instrument(skip(self))]
    pub fn find_by_path(&self, path: &Path) -> Result<Option<Project>, CoreError> {
        self.store.find_by_path(path)
    }
}

#[cfg(test)]
mod tests {
    // Future: mock-based tests for Custodian orchestration
}
