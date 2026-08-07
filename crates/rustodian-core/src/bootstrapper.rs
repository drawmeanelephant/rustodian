use crate::Custodian;
use crate::error::CoreError;
use rustodian_types::{Language, Project};
use std::collections::HashMap;
use std::path::Path;

/// Handles automated project environment bootstrapping, isolation, and verification.
pub struct ProjectBootstrapper<'a> {
    custodian: &'a Custodian,
}

impl<'a> ProjectBootstrapper<'a> {
    pub fn new(custodian: &'a Custodian) -> Self {
        Self { custodian }
    }

    /// Run a required bootstrap or verification command, treating any non-zero
    /// (or missing) exit code as a hard failure.
    ///
    /// Successful behavior and command names are unchanged — this only turns
    /// failed commands into propagated errors so bootstrapping stops before
    /// any later step runs.
    fn run_checked(
        &self,
        project: &Project,
        command_name: &str,
        program: &str,
        env: &HashMap<String, String>,
    ) -> Result<(), CoreError> {
        let exit_code = self.custodian.run_and_log_command(
            project,
            command_name,
            program,
            true,
            env.clone(),
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

    /// Perform environment isolation, bootstrap setup, and verification for the project.
    pub fn bootstrap_and_verify(&self, project: &Project) -> Result<(), CoreError> {
        let mut env = HashMap::new();

        for lang_det in &project.languages {
            match lang_det.language {
                Language::Rust => {
                    self.bootstrap_rust(project, &env)?;
                }
                Language::Node => {
                    self.bootstrap_node(project, &env)?;
                }
                Language::Go => {
                    // Isolation: Set GOPATH to a project-local .gopath folder to keep the host system clean
                    let local_gopath = project.path.join(".gopath");
                    env.insert(
                        "GOPATH".to_string(),
                        local_gopath.to_string_lossy().to_string(),
                    );
                    self.bootstrap_go(project, &env)?;
                }
                Language::Python => {
                    self.bootstrap_python(project, &env)?;
                }
                Language::Unknown(_) | Language::Ruby | Language::Zig => {}
            }
        }

        Ok(())
    }

    fn bootstrap_rust(
        &self,
        project: &Project,
        env: &HashMap<String, String>,
    ) -> Result<(), CoreError> {
        // Setup/Bootstrap
        tracing::info!("Bootstrapping Rust project: {}", project.name);
        self.run_checked(project, "bootstrap:rust", "cargo build", env)?;

        // Verification
        tracing::info!("Verifying Rust project: {}", project.name);
        self.run_checked(project, "verify:rust", "cargo test", env)?;

        Ok(())
    }

    fn bootstrap_node(
        &self,
        project: &Project,
        env: &HashMap<String, String>,
    ) -> Result<(), CoreError> {
        let path = &project.path;
        let (install_cmd, test_cmd) = if path.join("yarn.lock").exists() {
            ("yarn install", "yarn test")
        } else if path.join("pnpm-lock.yaml").exists() {
            ("pnpm install", "pnpm test")
        } else if path.join("bun.lockb").exists() {
            ("bun install", "bun test")
        } else {
            ("npm install", "npm test")
        };

        // Setup/Bootstrap
        tracing::info!("Bootstrapping Node project: {}", project.name);
        self.run_checked(project, "bootstrap:node", install_cmd, env)?;

        // Verification
        tracing::info!("Verifying Node project: {}", project.name);
        self.run_checked(project, "verify:node", test_cmd, env)?;

        Ok(())
    }

    fn bootstrap_go(
        &self,
        project: &Project,
        env: &HashMap<String, String>,
    ) -> Result<(), CoreError> {
        // Setup/Bootstrap
        tracing::info!("Bootstrapping Go project: {}", project.name);
        self.run_checked(project, "bootstrap:go", "go mod download", env)?;

        // Verification
        tracing::info!("Verifying Go project: {}", project.name);
        self.run_checked(project, "verify:go", "go test ./...", env)?;

        Ok(())
    }

    fn bootstrap_python(
        &self,
        project: &Project,
        env: &HashMap<String, String>,
    ) -> Result<(), CoreError> {
        tracing::info!("Bootstrapping Python project: {}", project.name);

        // Isolation: Set up a virtualenv (.venv) inside the project
        let mut venv_success = false;
        for cmd in &["python3 -m venv .venv", "python -m venv .venv"] {
            if self
                .run_checked(project, "bootstrap:python_venv", cmd, env)
                .is_ok()
            {
                venv_success = true;
                break;
            }
        }

        if !venv_success {
            return Err(CoreError::Internal(
                "failed to create Python virtual environment (.venv)".to_string(),
            ));
        }

        // Setup/Bootstrap dependencies
        let path = &project.path;
        let pip_env = env.clone();
        // Point to the virtualenv python/pip bin
        let pip_path = if cfg!(windows) {
            ".venv\\Scripts\\pip"
        } else {
            ".venv/bin/pip"
        };

        if path.join("requirements.txt").exists() {
            let install_cmd = format!("{pip_path} install -r requirements.txt");
            self.run_checked(project, "bootstrap:python_deps", &install_cmd, &pip_env)?;
        }
        if path.join("pyproject.toml").exists() || path.join("setup.py").exists() {
            let install_cmd = format!("{pip_path} install .");
            self.run_checked(project, "bootstrap:python_deps", &install_cmd, &pip_env)?;
        }

        // Verification
        let pytest_path = if cfg!(windows) {
            ".venv\\Scripts\\pytest"
        } else {
            ".venv/bin/pytest"
        };
        let python_path = if cfg!(windows) {
            ".venv\\Scripts\\python"
        } else {
            ".venv/bin/python"
        };

        let test_cmd = if path.join(pytest_path).exists() || Path::new(pytest_path).exists() {
            format!("{pytest_path} -v")
        } else {
            format!("{python_path} -m unittest discover")
        };

        tracing::info!("Verifying Python project: {}", project.name);
        self.run_checked(project, "verify:python", &test_cmd, &pip_env)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Custodian;
    use crate::runner::CommandSpec;
    use crate::traits::{
        CommandRunner, GitInspector, ProjectScanner, ProjectStore, RunningProcess,
    };
    use rustodian_types::{DetectionConfidence, Language, LanguageDetection, Project, ProjectId};
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::Mutex;

    struct MockRunningProcess {
        exit_code: Option<i32>,
    }

    impl RunningProcess for MockRunningProcess {
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
            Some(Box::new(std::io::Cursor::new("mock stdout\n")))
        }
        fn stderr(&mut self) -> Option<Box<dyn std::io::Read + Send + Sync>> {
            Some(Box::new(std::io::Cursor::new("mock stderr\n")))
        }
    }

    /// Mock command runner with a configurable queue of exit codes, consumed in
    /// spawn order. Defaults to success (exit 0) when the queue is exhausted.
    struct MockCommandRunner {
        commands_run: Arc<Mutex<Vec<String>>>,
        exit_codes: Arc<Mutex<VecDeque<i32>>>,
    }

    impl CommandRunner for MockCommandRunner {
        fn spawn(&self, spec: CommandSpec) -> Result<Box<dyn RunningProcess>, CoreError> {
            let mut list = self.commands_run.lock().unwrap();
            list.push(spec.program.clone());
            let exit_code = self.exit_codes.lock().unwrap().pop_front().unwrap_or(0);
            Ok(Box::new(MockRunningProcess {
                exit_code: Some(exit_code),
            }))
        }
    }

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
        fn save_scan(
            &self,
            _scan: &rustodian_types::ScanRecord,
        ) -> Result<rustodian_types::ScanId, CoreError> {
            Ok(rustodian_types::ScanId::new())
        }
        fn get_latest_scan(&self) -> Result<Option<rustodian_types::ScanRecord>, CoreError> {
            Ok(None)
        }
        fn save_log(&self, _log: &rustodian_types::ProjectLog) -> Result<(), CoreError> {
            Ok(())
        }
        fn list_logs(
            &self,
            _project_id: &str,
            _limit: usize,
        ) -> Result<Vec<rustodian_types::ProjectLog>, CoreError> {
            Ok(vec![])
        }
        fn get_log(&self, _id: &str) -> Result<Option<rustodian_types::ProjectLog>, CoreError> {
            Ok(None)
        }
        fn get_latest_log(
            &self,
            _project_id: &str,
        ) -> Result<Option<rustodian_types::ProjectLog>, CoreError> {
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
            _config: &rustodian_types::ScanConfig,
        ) -> Result<Vec<crate::traits::DiscoveredProject>, CoreError> {
            Ok(vec![])
        }
    }

    struct MockGit;
    impl GitInspector for MockGit {
        fn inspect(&self, _path: &Path) -> Result<Option<rustodian_types::VcsInfo>, CoreError> {
            Ok(None)
        }
        fn get_dirty_files(
            &self,
            _project_path: &Path,
        ) -> Result<Vec<std::path::PathBuf>, CoreError> {
            Ok(vec![])
        }
    }

    fn make_custodian(runner: MockCommandRunner) -> Custodian {
        Custodian::new(
            Box::new(MockStore),
            Box::new(MockScanner),
            Box::new(MockGit),
            Box::new(runner),
        )
    }

    fn make_runner(
        commands_run: Arc<Mutex<Vec<String>>>,
        exit_codes: Vec<i32>,
    ) -> MockCommandRunner {
        MockCommandRunner {
            commands_run,
            exit_codes: Arc::new(Mutex::new(VecDeque::from(exit_codes))),
        }
    }

    fn make_project(name: &str, language: Language) -> Project {
        Project {
            id: ProjectId::new(),
            name: name.to_string(),
            path: PathBuf::from(format!("/tmp/{name}")),
            languages: vec![LanguageDetection {
                language,
                confidence: DetectionConfidence::High,
                markers: vec![],
            }],
            vcs: None,
            discovered_at: chrono::Utc::now(),
            last_scanned_at: None,
            metadata: rustodian_types::ProjectMetadata::default(),
        }
    }

    #[test]
    fn test_bootstrap_rust_project() {
        let commands_run = Arc::new(Mutex::new(Vec::new()));
        let custodian = make_custodian(make_runner(commands_run.clone(), vec![]));
        let project = make_project("test_rust", Language::Rust);

        ProjectBootstrapper::new(&custodian)
            .bootstrap_and_verify(&project)
            .unwrap();

        let run_list = commands_run.lock().unwrap();
        assert_eq!(run_list.len(), 2);
        assert_eq!(run_list[0], "cargo build");
        assert_eq!(run_list[1], "cargo test");
    }

    #[test]
    fn test_rust_build_failure_stops_before_test() {
        let commands_run = Arc::new(Mutex::new(Vec::new()));
        let custodian = make_custodian(make_runner(commands_run.clone(), vec![1]));
        let project = make_project("test_rust_fail", Language::Rust);

        let err = ProjectBootstrapper::new(&custodian)
            .bootstrap_and_verify(&project)
            .unwrap_err();

        // `cargo test` must never run after a failed `cargo build`.
        let run_list = commands_run.lock().unwrap();
        assert_eq!(run_list.len(), 1);
        assert_eq!(run_list[0], "cargo build");
        assert!(err.to_string().contains("bootstrap:rust"));
    }

    #[test]
    fn test_rust_test_failure_returns_failure() {
        let commands_run = Arc::new(Mutex::new(Vec::new()));
        let custodian = make_custodian(make_runner(commands_run.clone(), vec![0, 1]));
        let project = make_project("test_rust_test_fail", Language::Rust);

        let err = ProjectBootstrapper::new(&custodian)
            .bootstrap_and_verify(&project)
            .unwrap_err();

        // Build succeeds, but a failed `cargo test` must fail the whole bootstrap.
        let run_list = commands_run.lock().unwrap();
        assert_eq!(run_list.len(), 2);
        assert_eq!(run_list[0], "cargo build");
        assert_eq!(run_list[1], "cargo test");
        assert!(err.to_string().contains("verify:rust"));
    }

    #[test]
    fn test_bootstrap_go_project() {
        let commands_run = Arc::new(Mutex::new(Vec::new()));
        let custodian = make_custodian(make_runner(commands_run.clone(), vec![]));
        let project = make_project("test_go", Language::Go);

        ProjectBootstrapper::new(&custodian)
            .bootstrap_and_verify(&project)
            .unwrap();

        let run_list = commands_run.lock().unwrap();
        assert_eq!(run_list.len(), 2);
        assert_eq!(run_list[0], "go mod download");
        assert_eq!(run_list[1], "go test ./...");
    }

    #[test]
    fn test_go_verification_failure_propagates() {
        let commands_run = Arc::new(Mutex::new(Vec::new()));
        let custodian = make_custodian(make_runner(commands_run.clone(), vec![0, 1]));
        let project = make_project("test_go_fail", Language::Go);

        let err = ProjectBootstrapper::new(&custodian)
            .bootstrap_and_verify(&project)
            .unwrap_err();

        let run_list = commands_run.lock().unwrap();
        assert_eq!(run_list.len(), 2);
        assert_eq!(run_list[0], "go mod download");
        assert_eq!(run_list[1], "go test ./...");
        assert!(err.to_string().contains("verify:go"));
    }

    #[test]
    fn test_node_install_failure_stops_before_test() {
        let commands_run = Arc::new(Mutex::new(Vec::new()));
        let custodian = make_custodian(make_runner(commands_run.clone(), vec![1]));
        let project = make_project("test_node_fail", Language::Node);

        let err = ProjectBootstrapper::new(&custodian)
            .bootstrap_and_verify(&project)
            .unwrap_err();

        // `npm test` must never run after a failed install.
        let run_list = commands_run.lock().unwrap();
        assert_eq!(run_list.len(), 1);
        assert_eq!(run_list[0], "npm install");
        assert!(err.to_string().contains("bootstrap:node"));
    }

    #[test]
    fn test_python_venv_fallback_first_fails_second_succeeds() {
        let commands_run = Arc::new(Mutex::new(Vec::new()));
        let custodian = make_custodian(make_runner(commands_run.clone(), vec![1, 0, 0]));
        let project = make_project("test_python_fallback", Language::Python);

        ProjectBootstrapper::new(&custodian)
            .bootstrap_and_verify(&project)
            .unwrap();

        let run_list = commands_run.lock().unwrap();
        assert_eq!(run_list.len(), 3);
        assert_eq!(run_list[0], "python3 -m venv .venv");
        assert_eq!(run_list[1], "python -m venv .venv");
        let test_cmd = if cfg!(windows) {
            ".venv\\Scripts\\python -m unittest discover"
        } else {
            ".venv/bin/python -m unittest discover"
        };
        assert_eq!(run_list[2], test_cmd);
    }

    #[test]
    fn test_python_both_venv_commands_fail() {
        let commands_run = Arc::new(Mutex::new(Vec::new()));
        let custodian = make_custodian(make_runner(commands_run.clone(), vec![1, 1]));
        let project = make_project("test_python_venv_fail", Language::Python);

        let err = ProjectBootstrapper::new(&custodian)
            .bootstrap_and_verify(&project)
            .unwrap_err();

        // No dependency install or verification may run when the venv cannot be created.
        let run_list = commands_run.lock().unwrap();
        assert_eq!(run_list.len(), 2);
        assert_eq!(run_list[0], "python3 -m venv .venv");
        assert_eq!(run_list[1], "python -m venv .venv");
        assert!(err.to_string().contains(".venv"));
    }

    #[test]
    fn test_python_dependency_failure_propagates() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("requirements.txt"), b"requests==2.32.0\n").unwrap();

        let commands_run = Arc::new(Mutex::new(Vec::new()));
        let custodian = make_custodian(make_runner(commands_run.clone(), vec![0, 1]));
        let project = Project {
            id: ProjectId::new(),
            name: "test_python_deps".to_string(),
            path: temp.path().to_path_buf(),
            languages: vec![LanguageDetection {
                language: Language::Python,
                confidence: DetectionConfidence::High,
                markers: vec![],
            }],
            vcs: None,
            discovered_at: chrono::Utc::now(),
            last_scanned_at: None,
            metadata: rustodian_types::ProjectMetadata::default(),
        };

        let err = ProjectBootstrapper::new(&custodian)
            .bootstrap_and_verify(&project)
            .unwrap_err();

        // A failed dependency install must fail the whole bootstrap before verification.
        let run_list = commands_run.lock().unwrap();
        assert_eq!(run_list.len(), 2);
        assert_eq!(run_list[0], "python3 -m venv .venv");
        let install_cmd = if cfg!(windows) {
            ".venv\\Scripts\\pip install -r requirements.txt"
        } else {
            ".venv/bin/pip install -r requirements.txt"
        };
        assert_eq!(run_list[1], install_cmd);
        assert!(err.to_string().contains("bootstrap:python_deps"));
    }
}
