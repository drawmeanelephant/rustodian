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

slint::slint! {
    export { PipelineWindow } from "ui/pipeline.slint";
}

use message::{GuiMessage, MarkdownBlock, WorkerMessage};
use rustodian_storage::SqliteStore;
use rustodian_types::Project;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};

fn map_project_to_slint(project: &Project) -> SlintProject {
    let commands_vec: Vec<SlintProjectCommand> = project
        .metadata
        .commands
        .iter()
        .map(|c| SlintProjectCommand {
            name: SharedString::from(c.name.clone()),
            cmd: SharedString::from(c.command.clone()),
            args: SharedString::default(),
        })
        .collect();

    SlintProject {
        name: SharedString::from(project.name.clone()),
        path: SharedString::from(project.path.to_string_lossy().to_string()),
        discovery_date: SharedString::from(project.discovered_at.to_rfc3339()),
        commands: ModelRc::new(VecModel::from(commands_vec)),
    }
}

fn map_markdown_block(block: MarkdownBlock) -> SlintMarkdownBlock {
    match block {
        MarkdownBlock::Header { level, text } => SlintMarkdownBlock {
            block_type: SharedString::from("heading"),
            content: SharedString::from(text),
            level: level as i32,
            is_checked: false,
            task_id: SharedString::default(),
        },
        MarkdownBlock::CodeFence { text } => SlintMarkdownBlock {
            block_type: SharedString::from("code"),
            content: SharedString::from(text),
            level: 0,
            is_checked: false,
            task_id: SharedString::default(),
        },
        MarkdownBlock::HorizontalRule => SlintMarkdownBlock {
            block_type: SharedString::from("paragraph"),
            content: SharedString::from("---"),
            level: 0,
            is_checked: false,
            task_id: SharedString::default(),
        },
        MarkdownBlock::Task { text, checked } => {
            let task_id = text.clone();
            SlintMarkdownBlock {
                block_type: SharedString::from("task"),
                content: SharedString::from(text),
                level: 0,
                is_checked: checked,
                task_id: SharedString::from(task_id),
            }
        }
        MarkdownBlock::BulletList { text } => SlintMarkdownBlock {
            block_type: SharedString::from("paragraph"),
            content: SharedString::from(format!("• {text}")),
            level: 0,
            is_checked: false,
            task_id: SharedString::default(),
        },
        MarkdownBlock::NumberedList { number, text } => SlintMarkdownBlock {
            block_type: SharedString::from("paragraph"),
            content: SharedString::from(format!("{number} {text}")),
            level: 0,
            is_checked: false,
            task_id: SharedString::default(),
        },
        MarkdownBlock::Text { text } => SlintMarkdownBlock {
            block_type: SharedString::from("paragraph"),
            content: SharedString::from(text),
            level: 0,
            is_checked: false,
            task_id: SharedString::default(),
        },
        MarkdownBlock::BlankLine => SlintMarkdownBlock {
            block_type: SharedString::from("paragraph"),
            content: SharedString::default(),
            level: 0,
            is_checked: false,
            task_id: SharedString::default(),
        },
    }
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = PipelineWindow::new()?;
    let ui_weak = ui.as_weak();

    let store = Arc::new(SqliteStore::open(&SqliteStore::default_path().unwrap()).unwrap());
    let projects_list = Arc::new(Mutex::new(Vec::<Project>::new()));

    let (tx_gui, rx_gui) = mpsc::channel();
    let (tx_worker, rx_worker) = mpsc::channel();

    let repaint_fn: Arc<dyn Fn() + Send + Sync> = Arc::new({
        let ui_weak = ui_weak.clone();
        move || {
            let _ = slint::invoke_from_event_loop({
                let ui_weak = ui_weak.clone();
                move || {
                    if let Some(_ui) = ui_weak.upgrade() {}
                }
            });
        }
    });

    let worker_handle = {
        let store = store.clone();
        let repaint_fn_clone = repaint_fn.clone();
        std::thread::spawn(move || {
            worker::run_worker(store, &rx_gui, &tx_worker, &repaint_fn_clone);
        })
    };

    let ui_weak_ingest = ui_weak.clone();
    let tx_gui_ingest = tx_gui.clone();
    ui.on_trigger_ingest(move || {
        if let Some(ui) = ui_weak_ingest.upgrade() {
            ui.set_working(true);
            let target = ui.get_target_project().to_string();
            let _ = tx_gui_ingest.send(GuiMessage::ScanProjects {
                path: PathBuf::from(target),
            });
        }
    });

    let ui_weak_export = ui_weak.clone();
    let tx_gui_export = tx_gui.clone();
    ui.on_trigger_agent_export(move || {
        if let Some(ui) = ui_weak_export.upgrade() {
            ui.set_working(true);
            let target = ui.get_target_project().to_string();
            let _ = tx_gui_export.send(GuiMessage::ScanProjects {
                path: PathBuf::from(target),
            });
        }
    });

    let ui_weak_cmd = ui_weak.clone();
    let tx_gui_cmd = tx_gui.clone();
    let projects_list_cmd = projects_list.clone();
    ui.on_run_command(move |proj_name, cmd_name| {
        if let Some(ui) = ui_weak_cmd.upgrade() {
            let projects = projects_list_cmd.lock().unwrap();
            if let Some(p) = projects.iter().find(|p| p.name == proj_name.as_str()) {
                if let Some(c) = p
                    .metadata
                    .commands
                    .iter()
                    .find(|c| c.name == cmd_name.as_str())
                {
                    ui.set_working(true);
                    let _ = tx_gui_cmd.send(GuiMessage::RunCommand {
                        project_id: p.id.clone(),
                        project_path: p.path.clone(),
                        command_name: c.name.clone(),
                        command_str: c.command.clone(),
                        use_shell: c.use_shell,
                    });
                }
            }
        }
    });

    let ui_weak_doc = ui_weak.clone();
    let tx_gui_doc = tx_gui.clone();
    let projects_list_doc = projects_list.clone();
    ui.on_load_document(move |doc_name| {
        if let Some(ui) = ui_weak_doc.upgrade() {
            let idx = ui.get_selected_project_index();
            if idx >= 0 {
                let projects = projects_list_doc.lock().unwrap();
                if let Some(proj) = projects.get(idx as usize) {
                    let doc_path = proj.path.join(doc_name.as_str());
                    let _ = tx_gui_doc.send(GuiMessage::LoadDocContent {
                        path: doc_path,
                        known_hash: None,
                    });
                }
            }
        }
    });

    let tx_gui_toggle = tx_gui.clone();
    ui.on_toggle_task(move |task_id, completed| {
        let _ = tx_gui_toggle.send(GuiMessage::ToggleTask {
            task_id: task_id.to_string(),
            completed,
        });
    });

    let ui_weak_thread = ui_weak.clone();
    let projects_list_thread = projects_list.clone();
    std::thread::spawn(move || {
        while let Ok(msg) = rx_worker.recv() {
            let ui_weak_clone = ui_weak_thread.clone();
            let projects_list_clone = projects_list_thread.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_weak_clone.upgrade() {
                    match msg {
                        WorkerMessage::CommandStatus {
                            command_name: _,
                            is_running,
                            exit_status: _,
                            log_buffer,
                        } => {
                            ui.set_stream_logs(SharedString::from(log_buffer.snapshot()));
                            if !is_running {
                                ui.set_working(false);
                            }
                        }
                        WorkerMessage::ScanComplete(res) => {
                            ui.set_working(false);
                            match res {
                                Ok(report) => {
                                    ui.set_stream_logs(SharedString::from(format!(
                                        "Scan complete: {:?}",
                                        report
                                    )));
                                }
                                Err(e) => {
                                    ui.set_stream_logs(SharedString::from(format!(
                                        "Scan failed: {e}"
                                    )));
                                }
                            }
                        }
                        WorkerMessage::ProjectsLoaded(res) => {
                            if let Ok(projects) = res {
                                *projects_list_clone.lock().unwrap() = projects.clone();
                                let slint_projects: Vec<SlintProject> =
                                    projects.iter().map(map_project_to_slint).collect();
                                ui.set_projects(ModelRc::new(VecModel::from(slint_projects)));
                            }
                        }
                        WorkerMessage::DocLoaded {
                            content: _, parsed, ..
                        } => {
                            let slint_blocks: Vec<SlintMarkdownBlock> =
                                parsed.blocks.into_iter().map(map_markdown_block).collect();
                            ui.set_doc_blocks(ModelRc::new(VecModel::from(slint_blocks)));
                        }
                        _ => {}
                    }
                }
            });
        }
    });

    // Populate initial project list
    let _ = tx_gui.send(GuiMessage::LoadProjects);

    // Run event loop
    ui.run()?;

    // Join worker thread on exit
    drop(tx_gui);
    let _ = worker_handle.join();

    Ok(())
}
