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
    pub projects_purged: usize,
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

    scanner: Box<dyn ProjectScanner>,

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

    /// Access the underlying project store.
    pub fn store(&self) -> &dyn ProjectStore {
        self.store.as_ref()
    }

    pub(crate) fn git_inspector(&self) -> &dyn GitInspector {
        self.git.as_ref()
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

        // ── Self-Healing Garbage Collection Pass ──────────────────────
        // Purge tracked projects whose paths no longer exist on disk.
        let mut projects_purged = 0usize;
        let all_tracked = self.store.list_projects()?;
        for tracked in &all_tracked {
            if !tracked.path.exists() {
                self.store.delete_project(&tracked.id)?;
                info!(
                    project = %tracked.name,
                    path = %tracked.path.display(),
                    "Garbage-collected dead project path"
                );
                projects_purged += 1;
            }
        }

        Ok(ScanReport {
            scan_id,
            projects_found: discovered.len(),
            projects_new,
            projects_updated,
            projects_purged,
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

        // Logging already happened inside `run_and_log_command`; the child's exit
        // status now decides whether the overall command succeeded. Only a clean
        // exit code of 0 counts as success — any nonzero code or termination
        // without a code is a command failure that must propagate to the caller.
        let exit_code = self.run_and_log_command(
            &project,
            command_name,
            &cmd.command,
            cmd.use_shell,
            HashMap::new(),
        )?;

        match exit_code {
            Some(0) => Ok(()),
            Some(code) => Err(CoreError::CommandFailed {
                command_name: command_name.to_string(),
                exit_code: code,
            }),
            None => Err(CoreError::CommandTerminated {
                command_name: command_name.to_string(),
            }),
        }
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

        if let Some(h) = stdout_handle {
            h.join().expect("reader thread panicked");
        }
        if let Some(h) = stderr_handle {
            h.join().expect("reader thread panicked");
        }

        let exit_code = child.wait()?;

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
        let _ = self.store.prune_logs(&project.id.to_string(), 50);

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
    use super::*;
    use crate::runner::DefaultCommandRunner;
    use crate::traits::{
        CommandRunner, DiscoveredProject, GitInspector, ProjectScanner, ProjectStore,
        RunningProcess,
    };
    use rustodian_types::{ProjectId, ProjectLog, ScanConfig, ScanId, ScanRecord, VcsInfo};
    use std::path::Path;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    struct MockStore;
    impl ProjectStore for MockStore {
        fn save_project(&self, _project: &Project) -> Result<ProjectId, CoreError> {
            Ok(ProjectId::new())
        }
        fn get_project(&self, _id: &ProjectId) -> Result<Option<Project>, CoreError> {
            Ok(None)
        }
        fn list_projects(&self) -> Result<Vec<Project>, CoreError> {
            Ok(vec![])
        }
        fn delete_project(&self, _id: &ProjectId) -> Result<bool, CoreError> {
            Ok(true)
        }
        fn find_by_path(&self, _path: &Path) -> Result<Option<Project>, CoreError> {
            Ok(None)
        }
        fn save_scan(&self, _scan: &ScanRecord) -> Result<ScanId, CoreError> {
            Ok(ScanId::new())
        }
        fn get_latest_scan(&self) -> Result<Option<ScanRecord>, CoreError> {
            Ok(None)
        }
        fn save_log(&self, _log: &ProjectLog) -> Result<(), CoreError> {
            Ok(())
        }
        fn list_logs(
            &self,
            _project_id: &str,
            _limit: usize,
        ) -> Result<Vec<ProjectLog>, CoreError> {
            Ok(vec![])
        }
        fn get_log(&self, _id: &str) -> Result<Option<ProjectLog>, CoreError> {
            Ok(None)
        }
        fn get_latest_log(&self, _project_id: &str) -> Result<Option<ProjectLog>, CoreError> {
            Ok(None)
        }
        fn prune_logs(&self, _project_id: &str, _limit: usize) -> Result<usize, CoreError> {
            Ok(0)
        }
    }

    struct MockScanner;
    impl ProjectScanner for MockScanner {
        fn scan(
            &self,
            _root: &Path,
            _config: &ScanConfig,
        ) -> Result<Vec<DiscoveredProject>, CoreError> {
            Ok(vec![])
        }
    }

    struct MockGit;
    impl GitInspector for MockGit {
        fn inspect(&self, _path: &Path) -> Result<Option<VcsInfo>, CoreError> {
            Ok(None)
        }
        fn get_dirty_files(&self, _project_path: &Path) -> Result<Vec<PathBuf>, CoreError> {
            Ok(vec![])
        }
    }

    #[test]
    fn test_large_output_no_deadlock() {
        let store = MockStore;
        let scanner = MockScanner;
        let git = MockGit;
        let runner = DefaultCommandRunner;

        let custodian = Custodian::new(
            Box::new(store),
            Box::new(scanner),
            Box::new(git),
            Box::new(runner),
        );

        let project = Project {
            id: ProjectId::new(),
            name: "test_deadlock".to_string(),
            path: PathBuf::from("."),
            languages: vec![],
            vcs: None,
            discovered_at: chrono::Utc::now(),
            last_scanned_at: None,
            metadata: rustodian_types::ProjectMetadata::default(),
        };

        // Generate > 100KB of stdout to trigger the pipe buffer limit
        // Use a simpler test program string
        let spec_program = if cfg!(unix) {
            "for i in $(seq 1 15000); do echo '1234567890'; done"
        } else {
            "FOR /L %i IN (1,1,15000) DO echo 1234567890"
        };

        let result = custodian.run_and_log_command(
            &project,
            "test_cmd",
            spec_program,
            true, // use_shell = true
            std::collections::HashMap::new(),
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(0));
    }

    // ── run_command exit-status propagation ───────────────────────────────

    struct MockRunProcess {
        exit_code: Option<i32>,
    }

    impl RunningProcess for MockRunProcess {
        fn id(&self) -> u32 {
            1234
        }
        fn wait(&mut self) -> Result<Option<i32>, CoreError> {
            Ok(self.exit_code)
        }
        fn try_wait(&mut self) -> Result<Option<Option<i32>>, CoreError> {
            Ok(Some(self.exit_code))
        }
        fn kill(&mut self) -> Result<(), CoreError> {
            Ok(())
        }
        fn stdout(&mut self) -> Option<Box<dyn std::io::Read + Send + Sync>> {
            Some(Box::new(std::io::Cursor::new("mock stdout line\n")))
        }
        fn stderr(&mut self) -> Option<Box<dyn std::io::Read + Send + Sync>> {
            Some(Box::new(std::io::Cursor::new("mock stderr line\n")))
        }
    }

    struct ExitCodeRunner {
        exit_code: Option<i32>,
    }

    impl CommandRunner for ExitCodeRunner {
        fn spawn(&self, _spec: CommandSpec) -> Result<Box<dyn RunningProcess>, CoreError> {
            Ok(Box::new(MockRunProcess {
                exit_code: self.exit_code,
            }))
        }
    }

    /// Store that serves one project and records every persisted log.
    struct RecordingStore {
        project: Project,
        saved_logs: Arc<Mutex<Vec<ProjectLog>>>,
    }

    impl ProjectStore for RecordingStore {
        fn save_project(&self, _project: &Project) -> Result<ProjectId, CoreError> {
            Ok(ProjectId::new())
        }
        fn get_project(&self, _id: &ProjectId) -> Result<Option<Project>, CoreError> {
            Ok(Some(self.project.clone()))
        }
        fn list_projects(&self) -> Result<Vec<Project>, CoreError> {
            Ok(vec![self.project.clone()])
        }
        fn delete_project(&self, _id: &ProjectId) -> Result<bool, CoreError> {
            Ok(true)
        }
        fn find_by_path(&self, _path: &Path) -> Result<Option<Project>, CoreError> {
            Ok(None)
        }
        fn save_scan(&self, _scan: &ScanRecord) -> Result<ScanId, CoreError> {
            Ok(ScanId::new())
        }
        fn get_latest_scan(&self) -> Result<Option<ScanRecord>, CoreError> {
            Ok(None)
        }
        fn save_log(&self, log: &ProjectLog) -> Result<(), CoreError> {
            self.saved_logs.lock().unwrap().push(log.clone());
            Ok(())
        }
        fn list_logs(
            &self,
            _project_id: &str,
            _limit: usize,
        ) -> Result<Vec<ProjectLog>, CoreError> {
            Ok(vec![])
        }
        fn get_log(&self, _id: &str) -> Result<Option<ProjectLog>, CoreError> {
            Ok(None)
        }
        fn get_latest_log(&self, _project_id: &str) -> Result<Option<ProjectLog>, CoreError> {
            Ok(None)
        }
        fn prune_logs(&self, _project_id: &str, _limit: usize) -> Result<usize, CoreError> {
            Ok(0)
        }
    }

    fn demo_project() -> Project {
        Project {
            id: ProjectId::new(),
            name: "demo-project".to_string(),
            path: PathBuf::from("."),
            languages: vec![],
            vcs: None,
            discovered_at: chrono::Utc::now(),
            last_scanned_at: None,
            metadata: rustodian_types::ProjectMetadata {
                commands: vec![rustodian_types::ProjectCommand {
                    name: "demo-cmd".to_string(),
                    description: None,
                    command: "mock-command".to_string(),
                    source: ".rustodian.toml".to_string(),
                    use_shell: true,
                }],
                ..Default::default()
            },
        }
    }

    fn run_demo_command_with_runner(
        runner: Box<dyn CommandRunner>,
    ) -> (Result<(), CoreError>, Arc<Mutex<Vec<ProjectLog>>>) {
        let saved_logs = Arc::new(Mutex::new(Vec::new()));
        let store = RecordingStore {
            project: demo_project(),
            saved_logs: saved_logs.clone(),
        };
        let custodian = Custodian::new(
            Box::new(store),
            Box::new(MockScanner),
            Box::new(MockGit),
            runner,
        );
        let result = custodian.run_command("demo-project", "demo-cmd");
        (result, saved_logs)
    }

    fn run_demo_command(
        exit_code: Option<i32>,
    ) -> (Result<(), CoreError>, Arc<Mutex<Vec<ProjectLog>>>) {
        run_demo_command_with_runner(Box::new(ExitCodeRunner { exit_code }))
    }

    #[test]
    fn test_run_command_exit_zero_succeeds() {
        let (result, saved_logs) = run_demo_command(Some(0));
        assert!(result.is_ok());

        let logs = saved_logs.lock().unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].command_name, "demo-cmd");
        assert_eq!(logs[0].exit_code, Some(0));
    }

    #[test]
    fn test_run_command_exit_one_fails() {
        let (result, saved_logs) = run_demo_command(Some(1));
        let err = result.unwrap_err();
        match &err {
            CoreError::CommandFailed {
                command_name,
                exit_code,
            } => {
                assert_eq!(command_name, "demo-cmd");
                assert_eq!(*exit_code, 1);
            }
            other => panic!("expected CommandFailed, got {other:?}"),
        }
        assert!(err.to_string().contains("exit code 1"));

        let logs = saved_logs.lock().unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].exit_code, Some(1));
    }

    #[test]
    fn test_run_command_arbitrary_nonzero_exit_fails() {
        let (result, saved_logs) = run_demo_command(Some(42));
        let err = result.unwrap_err();
        assert!(matches!(
            &err,
            CoreError::CommandFailed { exit_code: 42, .. }
        ));
        assert!(err.to_string().contains("exit code 42"));

        let logs = saved_logs.lock().unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].exit_code, Some(42));
    }

    #[test]
    fn test_run_command_termination_without_exit_code_fails() {
        let (result, saved_logs) = run_demo_command(None);
        let err = result.unwrap_err();
        assert!(matches!(
            &err,
            CoreError::CommandTerminated { command_name }
                if command_name == "demo-cmd"
        ));

        // The log is still persisted even though the process reported no exit code.
        let logs = saved_logs.lock().unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].exit_code, None);
    }

    #[test]
    fn test_run_command_captures_stdout_and_stderr() {
        let (result, saved_logs) = run_demo_command(Some(1));
        assert!(result.is_err());

        // Output capture is unaffected by the failure: both streams land in the log.
        let logs = saved_logs.lock().unwrap();
        assert_eq!(logs.len(), 1);
        assert!(logs[0].log_text.contains("mock stdout line"));
        assert!(logs[0].log_text.contains("mock stderr line"));
    }

    struct FailingSpawnRunner;

    impl CommandRunner for FailingSpawnRunner {
        fn spawn(&self, _spec: CommandSpec) -> Result<Box<dyn RunningProcess>, CoreError> {
            Err(CoreError::Storage(
                "Failed to spawn process: mock failure".to_string(),
            ))
        }
    }

    #[test]
    fn test_run_command_spawn_failure_propagates_unchanged() {
        let (result, saved_logs) = run_demo_command_with_runner(Box::new(FailingSpawnRunner));
        let err = result.unwrap_err();
        assert!(matches!(
            &err,
            CoreError::Storage(msg) if msg.contains("Failed to spawn process")
        ));
        assert!(err.to_string().contains("Failed to spawn process"));

        // A process that never spawned has nothing to log.
        assert!(saved_logs.lock().unwrap().is_empty());
    }
}
