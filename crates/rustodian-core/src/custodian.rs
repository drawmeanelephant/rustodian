//! The Custodian — Rustodian's core orchestrator.
//!
//! Coordinates scanning, storage, and git inspection through trait objects.
//! Uses `Box<dyn Trait>` for simplicity — dynamic dispatch overhead is
//! irrelevant when every call hits the filesystem or database.

use std::collections::HashMap;
use std::path::Path;

use tracing::{info, instrument, warn};

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
    /// Always zero: scans are additive and never delete tracked projects,
    /// even when their paths no longer exist. Stale database records are
    /// removed explicitly with [`Custodian::prune`].
    pub projects_purged: usize,
}

/// Disposition of one stale project during a prune.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PruneOutcome {
    /// The tracked path is missing; the record was kept (dry run).
    Detected,
    /// The tracked path is missing; the database record was deleted.
    Purged,
}

impl PruneOutcome {
    /// Stable string label used by CLI formatting.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Detected => "detected",
            Self::Purged => "purged",
        }
    }
}

/// Result for one stale project during a prune.
#[derive(Debug, Clone)]
pub struct PruneProjectResult {
    pub id: ProjectId,
    pub name: String,
    pub path: std::path::PathBuf,
    pub outcome: PruneOutcome,
}

/// Report from a prune operation.
#[derive(Debug, Clone)]
pub struct PruneReport {
    /// True when the operation only inspected and mutated nothing.
    pub dry_run: bool,
    /// Number of tracked projects whose stored path no longer exists.
    pub stale_project_count: usize,
    /// Per-project results for every stale project.
    pub projects: Vec<PruneProjectResult>,
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
            // A single malformed, inaccessible, or otherwise broken repository
            // must not prevent unrelated projects from being indexed. Log the
            // failure and store the project without VCS info instead of
            // aborting the whole scan.
            let vcs = match self.git.inspect(&d.path) {
                Ok(vcs) => vcs,
                Err(err) => {
                    warn!(
                        path = %d.path.display(),
                        error = %err,
                        "Git inspection failed; indexing project without VCS info"
                    );
                    None
                }
            };
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

                // Record the platform a project-root marker belongs to (e.g.
                // Cloudflare Wrangler) in the extensible metadata bag. This is
                // independent of language identity: a Wrangler-only project has
                // no language detection, only this platform marker.
                if let Some(platform) = d
                    .project_roots
                    .first()
                    .map(rustodian_types::ProjectRootMarker::platform)
                {
                    metadata.set_platform(platform);
                }

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

        // No deletion pass: a tracked project is never removed just because its
        // filesystem path is temporarily missing. Explicit `prune` handles
        // stale database records.
        Ok(ScanReport {
            scan_id,
            projects_found: discovered.len(),
            projects_new,
            projects_updated,
            projects_purged: 0,
        })
    }

    /// Find tracked projects whose stored paths no longer exist on disk.
    ///
    /// Defaults to a dry run that only reports stale records. With `purge`,
    /// the database records of stale projects are deleted, relying on existing
    /// foreign-key cascades for associated relational data. This never touches
    /// the filesystem.
    #[instrument(skip(self), fields(purge))]
    pub fn prune(&self, purge: bool) -> Result<PruneReport, CoreError> {
        info!(purge, "Pruning stale project records");
        let all_tracked = self.store.list_projects()?;

        let mut projects = Vec::new();
        for tracked in &all_tracked {
            if !tracked.path.exists() {
                let outcome = if purge {
                    self.store.delete_project(&tracked.id)?;
                    info!(
                        project = %tracked.name,
                        path = %tracked.path.display(),
                        "Purged stale project record"
                    );
                    PruneOutcome::Purged
                } else {
                    info!(
                        project = %tracked.name,
                        path = %tracked.path.display(),
                        "Detected stale project record (dry run)"
                    );
                    PruneOutcome::Detected
                };
                projects.push(PruneProjectResult {
                    id: tracked.id.clone(),
                    name: tracked.name.clone(),
                    path: tracked.path.clone(),
                    outcome,
                });
            }
        }

        Ok(PruneReport {
            dry_run: !purge,
            stale_project_count: projects.len(),
            projects,
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
    use rustodian_types::{
        ProjectId, ProjectLog, ScanConfig, ScanId, ScanRecord, VcsInfo, VcsType,
    };
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

    // ── scan resilience: per-project git inspection failures ─────────────

    /// Scanner that returns a fixed set of discovered projects.
    struct FixedScanner {
        discovered: Vec<DiscoveredProject>,
    }

    impl ProjectScanner for FixedScanner {
        fn scan(
            &self,
            _root: &Path,
            _config: &ScanConfig,
        ) -> Result<Vec<DiscoveredProject>, CoreError> {
            Ok(self.discovered.clone())
        }
    }

    /// Git inspector that fails for one specific path and succeeds elsewhere.
    struct SelectiveGit {
        failing_path: PathBuf,
    }

    impl GitInspector for SelectiveGit {
        fn inspect(&self, path: &Path) -> Result<Option<VcsInfo>, CoreError> {
            if path == self.failing_path {
                Err(CoreError::Git("corrupt repository".to_string()))
            } else {
                Ok(Some(VcsInfo {
                    vcs_type: VcsType::Git,
                    branch: Some("main".to_string()),
                    remote_url: None,
                    is_dirty: false,
                    last_commit: None,
                }))
            }
        }
        fn get_dirty_files(&self, _project_path: &Path) -> Result<Vec<PathBuf>, CoreError> {
            Ok(vec![])
        }
    }

    /// Store that records every saved project and can fail on demand.
    struct RecordingScanStore {
        saved: Arc<Mutex<Vec<Project>>>,
        fail_save: bool,
    }

    impl ProjectStore for RecordingScanStore {
        fn save_project(&self, project: &Project) -> Result<ProjectId, CoreError> {
            if self.fail_save {
                return Err(CoreError::Storage("injected storage failure".to_string()));
            }
            self.saved.lock().unwrap().push(project.clone());
            Ok(project.id.clone())
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

    fn discovered_project(name: &str) -> DiscoveredProject {
        DiscoveredProject {
            name: name.to_string(),
            path: PathBuf::from(format!("/projects/{name}")),
            languages: vec![],
            commands: vec![],
            project_roots: vec![],
        }
    }

    #[test]
    fn test_scan_indexes_project_without_vcs_when_git_inspection_fails() {
        let good = discovered_project("good");
        let broken = discovered_project("broken");

        let saved = Arc::new(Mutex::new(Vec::new()));
        let store = RecordingScanStore {
            saved: saved.clone(),
            fail_save: false,
        };
        let scanner = FixedScanner {
            discovered: vec![good.clone(), broken.clone()],
        };
        let git = SelectiveGit {
            failing_path: broken.path.clone(),
        };

        let custodian = Custodian::new(
            Box::new(store),
            Box::new(scanner),
            Box::new(git),
            Box::new(DefaultCommandRunner),
        );

        let report = custodian
            .scan(Path::new("/projects"), &ScanConfig::default())
            .expect("scan should complete despite git inspection failure");

        // Both projects count toward the report as discovered/new.
        assert_eq!(report.projects_found, 2);
        assert_eq!(report.projects_new, 2);

        let saved = saved.lock().unwrap();
        assert_eq!(saved.len(), 2, "both projects must be saved");

        let saved_broken = saved
            .iter()
            .find(|p| p.path == broken.path)
            .expect("broken project should still be saved");
        assert!(
            saved_broken.vcs.is_none(),
            "broken project must be indexed with vcs: None"
        );

        let saved_good = saved
            .iter()
            .find(|p| p.path == good.path)
            .expect("good project should be saved");
        assert!(
            saved_good.vcs.is_some(),
            "successfully inspected project must keep its VCS info"
        );
    }

    #[test]
    fn test_scan_storage_failure_aborts_operation() {
        let store = RecordingScanStore {
            saved: Arc::new(Mutex::new(Vec::new())),
            fail_save: true,
        };
        let scanner = FixedScanner {
            discovered: vec![discovered_project("a"), discovered_project("b")],
        };

        let custodian = Custodian::new(
            Box::new(store),
            Box::new(scanner),
            Box::new(SelectiveGit {
                failing_path: PathBuf::from("/nonexistent"),
            }),
            Box::new(DefaultCommandRunner),
        );

        let err = custodian
            .scan(Path::new("/projects"), &ScanConfig::default())
            .expect_err("storage failure should abort the scan");
        assert!(matches!(
            &err,
            CoreError::Storage(msg) if msg.contains("injected storage failure")
        ));
    }

    // ── project-root markers → platform metadata ────────────────────────

    #[test]
    fn test_scan_stores_platform_marker_for_wrangler_root() {
        let mut discovered = discovered_project("worker");
        discovered.project_roots = vec![rustodian_types::ProjectRootMarker::CloudflareWrangler(
            "wrangler.jsonc".to_string(),
        )];

        let saved = Arc::new(Mutex::new(Vec::new()));
        let store = RecordingScanStore {
            saved: saved.clone(),
            fail_save: false,
        };
        let scanner = FixedScanner {
            discovered: vec![discovered],
        };
        let custodian = Custodian::new(
            Box::new(store),
            Box::new(scanner),
            Box::new(SelectiveGit {
                failing_path: PathBuf::from("/nonexistent"),
            }),
            Box::new(DefaultCommandRunner),
        );

        let report = custodian
            .scan(Path::new("/projects"), &ScanConfig::default())
            .expect("scan should succeed");
        assert_eq!(report.projects_new, 1);

        let saved = saved.lock().unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].metadata.extra["platform"], "cloudflare-wrangler");
        assert!(
            saved[0].languages.is_empty(),
            "Wrangler-only projects must not claim a language"
        );
    }

    #[test]
    fn test_scan_without_project_roots_has_no_platform_marker() {
        let discovered = discovered_project("plain");

        let saved = Arc::new(Mutex::new(Vec::new()));
        let store = RecordingScanStore {
            saved: saved.clone(),
            fail_save: false,
        };
        let scanner = FixedScanner {
            discovered: vec![discovered],
        };
        let custodian = Custodian::new(
            Box::new(store),
            Box::new(scanner),
            Box::new(SelectiveGit {
                failing_path: PathBuf::from("/nonexistent"),
            }),
            Box::new(DefaultCommandRunner),
        );

        custodian
            .scan(Path::new("/projects"), &ScanConfig::default())
            .expect("scan should succeed");

        let saved = saved.lock().unwrap();
        assert_eq!(saved.len(), 1);
        assert!(
            saved[0].metadata.extra.get("platform").is_none(),
            "no platform marker expected without project-root evidence"
        );
    }

    // ── explicit prune replaces implicit scan deletion ──────────────────

    /// In-memory store seeded with tracked projects; records every deletion.
    struct TrackedStore {
        projects: Arc<Mutex<Vec<Project>>>,
        deleted: Arc<Mutex<Vec<ProjectId>>>,
    }

    impl ProjectStore for TrackedStore {
        fn save_project(&self, project: &Project) -> Result<ProjectId, CoreError> {
            let mut projects = self.projects.lock().unwrap();
            match projects.iter_mut().find(|p| p.path == project.path) {
                Some(existing) => *existing = project.clone(),
                None => projects.push(project.clone()),
            }
            Ok(project.id.clone())
        }
        fn get_project(&self, id: &ProjectId) -> Result<Option<Project>, CoreError> {
            Ok(self
                .projects
                .lock()
                .unwrap()
                .iter()
                .find(|p| p.id == *id)
                .cloned())
        }
        fn list_projects(&self) -> Result<Vec<Project>, CoreError> {
            Ok(self.projects.lock().unwrap().clone())
        }
        fn delete_project(&self, id: &ProjectId) -> Result<bool, CoreError> {
            let mut projects = self.projects.lock().unwrap();
            let before = projects.len();
            projects.retain(|p| p.id != *id);
            self.deleted.lock().unwrap().push(id.clone());
            Ok(projects.len() != before)
        }
        fn find_by_path(&self, path: &Path) -> Result<Option<Project>, CoreError> {
            Ok(self
                .projects
                .lock()
                .unwrap()
                .iter()
                .find(|p| p.path == path)
                .cloned())
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

    fn tracked_project(name: &str, path: &str) -> Project {
        Project {
            id: ProjectId::new(),
            name: name.to_string(),
            path: PathBuf::from(path),
            languages: vec![],
            vcs: None,
            discovered_at: chrono::Utc::now(),
            last_scanned_at: None,
            metadata: rustodian_types::ProjectMetadata::default(),
        }
    }

    fn custodian_with_tracked(projects: Vec<Project>) -> (Custodian, Arc<Mutex<Vec<ProjectId>>>) {
        let deleted = Arc::new(Mutex::new(Vec::new()));
        let store = TrackedStore {
            projects: Arc::new(Mutex::new(projects)),
            deleted: deleted.clone(),
        };
        let custodian = Custodian::new(
            Box::new(store),
            Box::new(MockScanner),
            Box::new(MockGit),
            Box::new(DefaultCommandRunner),
        );
        (custodian, deleted)
    }

    #[test]
    fn test_scan_does_not_delete_missing_tracked_project() {
        let (custodian, deleted) =
            custodian_with_tracked(vec![tracked_project("gone", "/does/not/exist")]);

        let report = custodian
            .scan(Path::new("/projects"), &ScanConfig::default())
            .expect("scan should succeed");

        // The scan discovers nothing but must not purge the tracked project
        // merely because its path is missing.
        assert_eq!(
            report.projects_purged, 0,
            "scan reports zero implicit purges"
        );
        assert!(
            deleted.lock().unwrap().is_empty(),
            "scan must never call delete_project"
        );
        assert_eq!(
            custodian.list().unwrap().len(),
            1,
            "missing-path project must survive the scan"
        );
    }

    #[test]
    fn test_prune_dry_run_detects_stale_but_keeps_record() {
        let (custodian, deleted) =
            custodian_with_tracked(vec![tracked_project("gone", "/does/not/exist")]);

        let report = custodian.prune(false).expect("dry run should succeed");

        assert!(report.dry_run);
        assert_eq!(report.stale_project_count, 1);
        assert_eq!(report.projects.len(), 1);
        assert_eq!(report.projects[0].name, "gone");
        assert_eq!(report.projects[0].path, PathBuf::from("/does/not/exist"));
        assert_eq!(report.projects[0].outcome, PruneOutcome::Detected);

        assert!(
            deleted.lock().unwrap().is_empty(),
            "dry run must not delete anything"
        );
        assert_eq!(
            custodian.list().unwrap().len(),
            1,
            "dry run must not mutate storage"
        );
    }

    #[test]
    fn test_prune_purge_removes_stale_record() {
        let (custodian, deleted) =
            custodian_with_tracked(vec![tracked_project("gone", "/does/not/exist")]);

        let report = custodian.prune(true).expect("purge should succeed");

        assert!(!report.dry_run);
        assert_eq!(report.stale_project_count, 1);
        assert_eq!(report.projects[0].outcome, PruneOutcome::Purged);
        assert_eq!(deleted.lock().unwrap().len(), 1);
        assert!(
            custodian.list().unwrap().is_empty(),
            "purge must remove the stale record"
        );
    }

    #[test]
    fn test_prune_ignores_projects_with_existing_paths() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let (custodian, deleted) = custodian_with_tracked(vec![
            tracked_project("alive", dir.path().to_str().unwrap()),
            tracked_project("gone", "/does/not/exist"),
        ]);

        let report = custodian.prune(false).expect("dry run should succeed");

        assert_eq!(report.stale_project_count, 1);
        assert_eq!(report.projects.len(), 1);
        assert_eq!(report.projects[0].name, "gone");
        assert!(
            deleted.lock().unwrap().is_empty(),
            "existing projects must never be considered stale"
        );
    }

    #[test]
    fn test_prune_empty_database_succeeds() {
        let (custodian, _deleted) = custodian_with_tracked(vec![]);

        let dry = custodian.prune(false).expect("dry run on empty db");
        assert!(dry.dry_run);
        assert_eq!(dry.stale_project_count, 0);
        assert!(dry.projects.is_empty());

        let purged = custodian.prune(true).expect("purge on empty db");
        assert!(!purged.dry_run);
        assert_eq!(purged.stale_project_count, 0);
    }

    #[test]
    fn test_prune_dry_run_performs_zero_mutation() {
        let (custodian, deleted) =
            custodian_with_tracked(vec![tracked_project("gone", "/does/not/exist")]);

        let report = custodian.prune(false).expect("dry run should succeed");
        assert_eq!(report.stale_project_count, 1);

        // Nothing changed: the project is still listed with the same identity.
        let remaining = custodian.list().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].name, "gone");
        assert_eq!(remaining[0].id, report.projects[0].id);
        assert!(deleted.lock().unwrap().is_empty());
    }
}
