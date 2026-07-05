#![allow(clippy::too_many_lines, clippy::collapsible_if, clippy::cast_sign_loss)]
pub mod markdown;
pub mod message;
pub mod worker;

slint::include_modules!();

pub mod ui_mapping {
    use crate::SlintProject;
    use crate::SlintProjectCommand;
    use rustodian_types::{Project, ProjectCommand};
    use slint::{ModelRc, SharedString, VecModel};

    pub fn map_project(project: &Project) -> SlintProject {
        SlintProject {
            name: SharedString::from(project.name.clone()),
            path: SharedString::from(project.path.to_string_lossy().into_owned()),
            discovery_date: SharedString::from(project.discovered_at.to_rfc3339()),
            commands: ModelRc::new(VecModel::from(
                project
                    .metadata
                    .commands
                    .iter()
                    .map(map_project_command)
                    .collect::<Vec<_>>(),
            )),
        }
    }

    pub fn map_project_command(command: &ProjectCommand) -> SlintProjectCommand {
        SlintProjectCommand {
            name: SharedString::from(command.name.clone()),
            cmd: SharedString::from(command.command.clone()),
            args: SharedString::from(command.source.clone()),
        }
    }

    pub fn map_projects(projects: &[Project]) -> ModelRc<SlintProject> {
        let slint_projects: Vec<SlintProject> = projects.iter().map(map_project).collect();
        ModelRc::new(VecModel::from(slint_projects))
    }
}

use crate::message::{GuiMessage, MarkdownBlock, WorkerMessage};
use crate::ui_mapping::map_projects;
use rustodian_storage::SqliteStore;
use slint::{ComponentHandle, ModelRc, VecModel};
use std::path::PathBuf;
use std::sync::Arc;

fn main() -> Result<(), slint::PlatformError> {
    let window = PipelineWindow::new()?;

    // 1. Initialize Database
    let db_path = SqliteStore::default_path().expect("failed to determine database path");
    let store = SqliteStore::open(&db_path).expect("failed to open database");
    store.migrate().expect("failed to run migrations");
    let store_arc = Arc::new(store);

    // 2. Setup bidirectional channel boundaries
    let (gui_tx, gui_rx) = std::sync::mpsc::channel::<GuiMessage>();
    let (worker_tx, worker_rx) = std::sync::mpsc::channel::<WorkerMessage>();

    let window_weak = window.as_weak();

    // 3. Define the repaint trigger.
    // Slint runs on a blocking main thread. To wake it from the worker thread,
    // we use a dummy event callback inside the loop or call `invoke_from_event_loop`.
    let window_weak_clone = window_weak.clone();
    let repaint_fn = Arc::new(move || {
        let win_weak = window_weak_clone.clone();
        let _ = slint::invoke_from_event_loop(move || {
            // Triggers an evaluation pass on the main event loop
            if let Some(win) = win_weak.upgrade() {
                win.window().request_redraw();
            }
        });
    }) as Arc<dyn Fn() + Send + Sync>;

    // 4. Spawn Background Worker Thread
    let worker_store = Arc::clone(&store_arc);
    std::thread::spawn(move || {
        worker::run_worker(worker_store, &gui_rx, &worker_tx, &repaint_fn);
    });

    // 5. Spawn GUI Message Receiver Loop
    let window_receiver_weak = window_weak.clone();
    // Cache the original Rust projects to allow index lookup
    let projects_cache = Arc::new(std::sync::Mutex::new(Vec::<rustodian_types::Project>::new()));
    let projects_cache_clone = Arc::clone(&projects_cache);

    std::thread::spawn(move || {
        while let Ok(msg) = worker_rx.recv() {
            let window_inner = window_receiver_weak.clone();
            let cache = Arc::clone(&projects_cache_clone);

            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = window_inner.upgrade() {
                    match msg {
                        WorkerMessage::ProjectsLoaded(Ok(rust_projects)) => {
                            // Update cache
                            if let Ok(mut lock) = cache.lock() {
                                lock.clone_from(&rust_projects);
                            }
                            // Map and set to Slint UI property
                            ui.set_projects(map_projects(&rust_projects));
                        }
                        WorkerMessage::ProjectsLoaded(Err(err)) => {
                            ui.set_stream_logs(format!("[Storage Error] {err}\n").into());
                        }
                        WorkerMessage::CommandStatus {
                            command_name: _,
                            is_running,
                            exit_status,
                            log_buffer,
                        } => {
                            ui.set_working(is_running);

                            // Map the log buffer incrementally to the console area
                            let full_logs = log_buffer.snapshot();
                            ui.set_stream_logs(full_logs.into());

                            if let Some(status) = exit_status {
                                let current_logs = ui.get_stream_logs().to_string();
                                ui.set_stream_logs(
                                    format!(
                                        "{}{}\nCommand closed: {}\n",
                                        current_logs,
                                        "-".repeat(50),
                                        status
                                    )
                                    .into(),
                                );
                            }
                        }
                        WorkerMessage::DocLoaded {
                            content: _,
                            parsed,
                            last_modified: _,
                            content_hash: _,
                        } => {
                            // Translate parsed MarkdownBlock vector variants to flat SlintMarkdownBlock structures
                            let slint_blocks: Vec<SlintMarkdownBlock> = parsed
                                .blocks
                                .into_iter()
                                .filter_map(|block| {
                                    match block {
                                        MarkdownBlock::Header { level, text } => {
                                            Some(SlintMarkdownBlock {
                                                block_type: "heading".into(),
                                                content: text.into(),
                                                level: level.try_into().unwrap_or(0),
                                                is_checked: false,
                                                task_id: "".into(),
                                            })
                                        }
                                        MarkdownBlock::Text { text } => Some(SlintMarkdownBlock {
                                            block_type: "paragraph".into(),
                                            content: text.into(),
                                            level: 0,
                                            is_checked: false,
                                            task_id: "".into(),
                                        }),
                                        MarkdownBlock::CodeFence { text } => {
                                            Some(SlintMarkdownBlock {
                                                block_type: "code".into(),
                                                content: text.into(),
                                                level: 0,
                                                is_checked: false,
                                                task_id: "".into(),
                                            })
                                        }
                                        MarkdownBlock::Task { text, checked } => {
                                            // Simple task indexing/identifier generation
                                            let task_id = text.clone();
                                            Some(SlintMarkdownBlock {
                                                block_type: "task".into(),
                                                content: text.into(),
                                                level: 0,
                                                is_checked: checked,
                                                task_id: task_id.into(),
                                            })
                                        }
                                        _ => None, // Handle other structural variants (BlankLines, HorizontalRules)
                                    }
                                })
                                .collect();

                            ui.set_doc_blocks(ModelRc::new(VecModel::from(slint_blocks)));
                        }
                        WorkerMessage::ScanComplete(Ok(report)) => {
                            ui.set_stream_logs(
                                format!(
                                    "[Scan Done] Found: {} | New: {} | Updated: {}\n",
                                    report.projects_found,
                                    report.projects_new,
                                    report.projects_updated
                                )
                                .into(),
                            );
                        }
                        WorkerMessage::ScanComplete(Err(e)) => {
                            ui.set_stream_logs(
                                format!("[Scan Error] Failed to scan path: {e}\n").into(),
                            );
                        }
                        _ => {}
                    }
                }
            });
        }
    });

    // 6. Bind the Callback Endpoints

    // Initial load trigger on bootstrap
    let _ = gui_tx.send(GuiMessage::LoadProjects);

    // Callback: trigger-ingest
    let gui_tx_clone = gui_tx.clone();
    let window_weak_clone = window.as_weak();
    window.on_trigger_ingest(move || {
        if let Some(win) = window_weak_clone.upgrade() {
            let slug = win.get_repo_slug().to_string();
            let path = PathBuf::from(&slug);

            if slug.trim().is_empty() {
                win.set_stream_logs("Error: Repo slug cannot be empty\n".into());
                return;
            }

            win.set_working(true);
            if let Err(e) = gui_tx_clone.send(GuiMessage::ScanProjects { path }) {
                tracing::error!("Worker channel closed unexpectedly: {e}");
            }
        }
    });

    // Callback: run-command
    let gui_tx_clone = gui_tx.clone();
    let window_weak_clone = window.as_weak();
    let cache_ref = Arc::clone(&projects_cache);
    window.on_run_command(move |proj_name, cmd_name| {
        if let Some(win) = window_weak_clone.upgrade() {
            let proj_name_str = proj_name.to_string();
            let cmd_name_str = cmd_name.to_string();

            // Locate command configuration in the cache using Project queries
            if let Ok(lock) = cache_ref.lock() {
                if let Some(proj) = lock.iter().find(|p| p.name == proj_name_str) {
                    if let Some(cmd) = proj
                        .metadata
                        .commands
                        .iter()
                        .find(|c| c.name == cmd_name_str)
                    {
                        win.set_working(true);
                        let _ = gui_tx_clone.send(GuiMessage::RunCommand {
                            project_id: proj.id.clone(),
                            project_path: proj.path.clone(),
                            command_name: cmd.name.clone(),
                            command_str: cmd.command.clone(),
                            use_shell: cmd.use_shell,
                        });
                    }
                }
            }
        }
    });

    // Callback: load-document
    let gui_tx_clone = gui_tx.clone();
    let window_weak_clone = window.as_weak();
    let cache_ref = Arc::clone(&projects_cache);
    window.on_load_document(move |doc_name| {
        if let Some(win) = window_weak_clone.upgrade() {
            let selected_idx = win.get_selected_project_index();
            if selected_idx >= 0 {
                if let Ok(lock) = cache_ref.lock() {
                    if let Some(proj) = lock.get(selected_idx as usize) {
                        let full_doc_path = proj.path.join(doc_name.as_str());
                        let _ = gui_tx_clone.send(GuiMessage::LoadDocContent {
                            path: full_doc_path,
                            known_hash: None,
                        });
                    }
                }
            }
        }
    });

    // Callback: toggle-task
    let gui_tx_clone = gui_tx.clone();
    window.on_toggle_task(move |task_id, checked| {
        let _ = gui_tx_clone.send(GuiMessage::ToggleTask {
            task_id: task_id.to_string(),
            completed: checked,
        });
    });

    // Run application window loop blocks
    window.run()
}
