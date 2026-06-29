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
                Language::Unknown(_) => {}
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
        self.custodian.run_and_log_command(
            project,
            "bootstrap:rust",
            "cargo build",
            true,
            env.clone(),
        )?;

        // Verification
        tracing::info!("Verifying Rust project: {}", project.name);
        self.custodian.run_and_log_command(
            project,
            "verify:rust",
            "cargo test",
            true,
            env.clone(),
        )?;

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
        self.custodian.run_and_log_command(
            project,
            "bootstrap:node",
            install_cmd,
            true,
            env.clone(),
        )?;

        // Verification
        tracing::info!("Verifying Node project: {}", project.name);
        self.custodian
            .run_and_log_command(project, "verify:node", test_cmd, true, env.clone())?;

        Ok(())
    }

    fn bootstrap_go(
        &self,
        project: &Project,
        env: &HashMap<String, String>,
    ) -> Result<(), CoreError> {
        // Setup/Bootstrap
        tracing::info!("Bootstrapping Go project: {}", project.name);
        self.custodian.run_and_log_command(
            project,
            "bootstrap:go",
            "go mod download",
            true,
            env.clone(),
        )?;

        // Verification
        tracing::info!("Verifying Go project: {}", project.name);
        self.custodian.run_and_log_command(
            project,
            "verify:go",
            "go test ./...",
            true,
            env.clone(),
        )?;

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
                .custodian
                .run_and_log_command(project, "bootstrap:python_venv", cmd, true, env.clone())
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
            self.custodian.run_and_log_command(
                project,
                "bootstrap:python_deps",
                &install_cmd,
                true,
                pip_env.clone(),
            )?;
        }
        if path.join("pyproject.toml").exists() || path.join("setup.py").exists() {
            let install_cmd = format!("{pip_path} install .");
            self.custodian.run_and_log_command(
                project,
                "bootstrap:python_deps",
                &install_cmd,
                true,
                pip_env.clone(),
            )?;
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
        self.custodian
            .run_and_log_command(project, "verify:python", &test_cmd, true, pip_env)?;

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

    struct MockCommandRunner {
        commands_run: Arc<Mutex<Vec<String>>>,
    }

    impl CommandRunner for MockCommandRunner {
        fn spawn(&self, spec: CommandSpec) -> Result<Box<dyn RunningProcess>, CoreError> {
            let mut list = self.commands_run.lock().unwrap();
            list.push(spec.program.clone());
            Ok(Box::new(MockRunningProcess { exit_code: Some(0) }))
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
    }

    #[test]
    fn test_bootstrap_rust_project() {
        let commands_run = Arc::new(Mutex::new(Vec::new()));
        let runner = MockCommandRunner {
            commands_run: commands_run.clone(),
        };
        let store = MockStore;
        let scanner = MockScanner;
        let git = MockGit;
        let custodian = Custodian::new(
            Box::new(store),
            Box::new(scanner),
            Box::new(git),
            Box::new(runner),
        );

        let project = Project {
            id: ProjectId::new(),
            name: "test_rust".to_string(),
            path: PathBuf::from("/tmp/test_rust"),
            languages: vec![LanguageDetection {
                language: Language::Rust,
                confidence: DetectionConfidence::High,
                markers: vec![],
            }],
            vcs: None,
            discovered_at: chrono::Utc::now(),
            last_scanned_at: None,
            metadata: rustodian_types::ProjectMetadata::default(),
        };

        let bootstrapper = ProjectBootstrapper::new(&custodian);
        bootstrapper.bootstrap_and_verify(&project).unwrap();

        let run_list = commands_run.lock().unwrap();
        assert_eq!(run_list.len(), 2);
        assert_eq!(run_list[0], "cargo build");
        assert_eq!(run_list[1], "cargo test");
    }

    #[test]
    fn test_bootstrap_go_project() {
        let commands_run = Arc::new(Mutex::new(Vec::new()));
        let runner = MockCommandRunner {
            commands_run: commands_run.clone(),
        };
        let store = MockStore;
        let scanner = MockScanner;
        let git = MockGit;
        let custodian = Custodian::new(
            Box::new(store),
            Box::new(scanner),
            Box::new(git),
            Box::new(runner),
        );

        let project = Project {
            id: ProjectId::new(),
            name: "test_go".to_string(),
            path: PathBuf::from("/tmp/test_go"),
            languages: vec![LanguageDetection {
                language: Language::Go,
                confidence: DetectionConfidence::High,
                markers: vec![],
            }],
            vcs: None,
            discovered_at: chrono::Utc::now(),
            last_scanned_at: None,
            metadata: rustodian_types::ProjectMetadata::default(),
        };

        let bootstrapper = ProjectBootstrapper::new(&custodian);
        bootstrapper.bootstrap_and_verify(&project).unwrap();

        let run_list = commands_run.lock().unwrap();
        assert_eq!(run_list.len(), 2);
        assert_eq!(run_list[0], "go mod download");
        assert_eq!(run_list[1], "go test ./...");
    }
}
