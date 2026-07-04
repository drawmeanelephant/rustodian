pub mod markdown;
pub mod message;
pub mod worker;

pub mod ui_mapping {
    use rustodian_types::{Project, ProjectCommand};
    use slint::{ModelRc, SharedString, VecModel};

    slint::slint! {
        export struct SlintProject {
            id: string,
            name: string,
            path: string,
            discovered_at: string,
        }

        export struct SlintProjectCommand {
            name: string,
            description: string,
            command: string,
            source: string,
            use_shell: bool,
        }

        export component Dummy inherits Window {}
    }

    pub fn map_project(project: &Project) -> SlintProject {
        SlintProject {
            id: SharedString::from(project.id.to_string()),
            name: SharedString::from(project.name.clone()),
            path: SharedString::from(project.path.to_string_lossy().into_owned()),
            discovered_at: SharedString::from(project.discovered_at.to_rfc3339()),
        }
    }

    pub fn map_project_command(command: &ProjectCommand) -> SlintProjectCommand {
        SlintProjectCommand {
            name: SharedString::from(command.name.clone()),
            description: SharedString::from(command.description.clone().unwrap_or_default()),
            command: SharedString::from(command.command.clone()),
            source: SharedString::from(command.source.clone()),
            use_shell: command.use_shell,
        }
    }

    pub fn map_projects(projects: &[Project]) -> ModelRc<SlintProject> {
        let slint_projects: Vec<SlintProject> = projects.iter().map(map_project).collect();
        ModelRc::new(VecModel::from(slint_projects))
    }

    pub fn map_commands(commands: &[ProjectCommand]) -> ModelRc<SlintProjectCommand> {
        let slint_commands: Vec<SlintProjectCommand> =
            commands.iter().map(map_project_command).collect();
        ModelRc::new(VecModel::from(slint_commands))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use chrono::Utc;
        use rustodian_types::{ProjectId, ProjectMetadata};
        use slint::Model;
        use std::path::PathBuf;

        #[test]
        fn test_map_project() {
            let project = Project {
                id: ProjectId::new(),
                name: "TestProject".to_string(),
                path: PathBuf::from("/test/path"),
                languages: vec![],
                vcs: None,
                discovered_at: Utc::now(),
                last_scanned_at: None,
                metadata: ProjectMetadata::default(),
            };

            let slint_project = map_project(&project);

            assert_eq!(slint_project.id.as_str(), project.id.to_string());
            assert_eq!(slint_project.name.as_str(), "TestProject");
            assert_eq!(slint_project.path.as_str(), "/test/path");
        }

        #[test]
        fn test_map_project_command() {
            let cmd = ProjectCommand {
                name: "test-cmd".to_string(),
                description: Some("Test description".to_string()),
                command: "cargo test".to_string(),
                source: "Cargo.toml".to_string(),
                use_shell: false,
            };

            let slint_cmd = map_project_command(&cmd);

            assert_eq!(slint_cmd.name.as_str(), "test-cmd");
            assert_eq!(slint_cmd.description.as_str(), "Test description");
            assert_eq!(slint_cmd.command.as_str(), "cargo test");
            assert_eq!(slint_cmd.source.as_str(), "Cargo.toml");
            assert!(!slint_cmd.use_shell);
        }

        #[test]
        fn test_map_project_command_no_desc() {
            let cmd = ProjectCommand {
                name: "test-cmd".to_string(),
                description: None,
                command: "cargo test".to_string(),
                source: "Cargo.toml".to_string(),
                use_shell: true,
            };

            let slint_cmd = map_project_command(&cmd);
            assert_eq!(slint_cmd.description.as_str(), "");
            assert!(slint_cmd.use_shell);
        }

        #[test]
        fn test_map_projects_array() {
            let p1 = Project {
                id: ProjectId::new(),
                name: "P1".to_string(),
                path: PathBuf::from("/p1"),
                languages: vec![],
                vcs: None,
                discovered_at: Utc::now(),
                last_scanned_at: None,
                metadata: ProjectMetadata::default(),
            };
            let p2 = Project {
                id: ProjectId::new(),
                name: "P2".to_string(),
                path: PathBuf::from("/p2"),
                languages: vec![],
                vcs: None,
                discovered_at: Utc::now(),
                last_scanned_at: None,
                metadata: ProjectMetadata::default(),
            };

            let models = map_projects(&[p1.clone(), p2.clone()]);
            assert_eq!(models.row_count(), 2);
            assert_eq!(models.row_data(0).unwrap().name.as_str(), "P1");
            assert_eq!(models.row_data(1).unwrap().name.as_str(), "P2");
        }
    }
}

slint::include_modules!();

use crate::message::{GuiMessage, WorkerMessage};
use rustodian_storage::SqliteStore;
use slint::ComponentHandle;
use std::path::PathBuf;
use std::sync::Arc;

fn main() -> Result<(), slint::PlatformError> {
    let window = PipelineWindow::new()?;

    // Initialize database
    let db_path = SqliteStore::default_path().expect("failed to determine database path");
    let store = SqliteStore::open(&db_path).expect("failed to open database");
    store.migrate().expect("failed to run migrations");
    let store_arc = Arc::new(store);

    // Setup channels for background worker communication
    let (gui_tx, gui_rx) = std::sync::mpsc::channel::<GuiMessage>();
    let (worker_tx, _worker_rx) = std::sync::mpsc::channel::<WorkerMessage>();

    let _window_weak = window.as_weak();

    // Create repaint callback for the background worker to trigger UI updates
    let repaint_fn = Arc::new(move || {
        // We do nothing for now, but in the future we can upgrade window_weak and trigger updates
    }) as Arc<dyn Fn() + Send + Sync>;

    // Spawn the background worker thread
    let worker_store = Arc::clone(&store_arc);
    std::thread::spawn(move || {
        worker::run_worker(worker_store, &gui_rx, &worker_tx, &repaint_fn);
    });

    let gui_tx_clone = gui_tx.clone();
    let window_weak_clone = window.as_weak();

    // Bind the ingest action
    window.on_trigger_ingest(move || {
        if let Some(win) = window_weak_clone.upgrade() {
            let slug = win.get_repo_slug();
            let path = PathBuf::from(slug.as_str());
            // Send a ScanProjects message to the background worker
            if let Err(e) = gui_tx_clone.send(GuiMessage::ScanProjects { path }) {
                tracing::error!("Worker channel closed unexpectedly: {e}");
            }
        }
    });

    window.run()
}
