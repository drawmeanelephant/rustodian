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
        let (branch, dirty_status) = if let Some(ref vcs) = project.vcs {
            (
                vcs.branch.clone().unwrap_or_else(|| "detached".to_string()),
                if vcs.is_dirty {
                    "Dirty ⚠️"
                } else {
                    "Clean"
                },
            )
        } else {
            ("No Git Repo".to_string(), "Clean")
        };

        SlintProject {
            id: SharedString::from(project.id.to_string()),
            git_branch: SharedString::from(branch),
            git_status: SharedString::from(dirty_status),
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
use std::fmt::Write;
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

    let gui_tx_receiver_loop = gui_tx.clone();
    let active_run_id = Arc::new(std::sync::Mutex::new(None::<uuid::Uuid>));
    std::thread::spawn(move || {
        while let Ok(msg) = worker_rx.recv() {
            let window_inner = window_receiver_weak.clone();
            let cache = Arc::clone(&projects_cache_clone);
            let gui_tx_receiver = gui_tx_receiver_loop.clone();
            let active_run_id_clone = active_run_id.clone();

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
                            run_id,
                            command_name: _,
                            is_running,
                            exit_status,
                            log_buffer,
                        } => {
                            let mut active_run_id_lock = active_run_id_clone.lock().unwrap();
                            if active_run_id_lock.is_some() && *active_run_id_lock != Some(run_id) {
                                return;
                            }
                            if is_running {
                                *active_run_id_lock = Some(run_id);
                            } else {
                                *active_run_id_lock = None;
                            }
                            ui.set_working(is_running);

                            // Map the log buffer incrementally to the console area
                            let full_logs = log_buffer.snapshot();
                            let current_logs = ui.get_stream_logs();
                            if full_logs.len() != current_logs.as_str().len() {
                                ui.set_stream_logs(full_logs.into());
                            }

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
                                .map(|block| {
                                    match block {
                                        MarkdownBlock::Header { level, text } => {
                                            SlintMarkdownBlock {
                                                block_type: "heading".into(),
                                                content: text.into(),
                                                level: level.try_into().unwrap_or(0),
                                                is_checked: false,
                                                task_id: "".into(),
                                            }
                                        }
                                        MarkdownBlock::Text { text } => SlintMarkdownBlock {
                                            block_type: "paragraph".into(),
                                            content: text.into(),
                                            level: 0,
                                            is_checked: false,
                                            task_id: "".into(),
                                        },
                                        MarkdownBlock::CodeFence { text } => SlintMarkdownBlock {
                                            block_type: "code".into(),
                                            content: text.into(),
                                            level: 0,
                                            is_checked: false,
                                            task_id: "".into(),
                                        },
                                        MarkdownBlock::BulletList { text } => SlintMarkdownBlock {
                                            block_type: "bullet".into(),
                                            content: text.into(),
                                            level: 0,
                                            is_checked: false,
                                            task_id: "".into(),
                                        },
                                        MarkdownBlock::NumberedList { number, text } => {
                                            SlintMarkdownBlock {
                                                block_type: "numbered".into(),
                                                content: format!("{number}. {text}").into(),
                                                level: 0,
                                                is_checked: false,
                                                task_id: "".into(),
                                            }
                                        }
                                        MarkdownBlock::HorizontalRule => SlintMarkdownBlock {
                                            block_type: "hr".into(),
                                            content: "".into(),
                                            level: 0,
                                            is_checked: false,
                                            task_id: "".into(),
                                        },
                                        MarkdownBlock::BlankLine => SlintMarkdownBlock {
                                            block_type: "blank".into(),
                                            content: "".into(),
                                            level: 0,
                                            is_checked: false,
                                            task_id: "".into(),
                                        },
                                        MarkdownBlock::Task { text, checked } => {
                                            // Simple task indexing/identifier generation
                                            let task_id = text.clone();
                                            SlintMarkdownBlock {
                                                block_type: "task".into(),
                                                content: text.into(),
                                                level: 0,
                                                is_checked: checked,
                                                task_id: task_id.into(),
                                            }
                                        }
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
                        #[allow(clippy::cast_precision_loss)]
                        WorkerMessage::CruftPurged(Ok(report)) => {
                            ui.set_working(false);
                            let bytes = report.bytes_reclaimed;
                            let formatted_size = if bytes == 0 {
                                "0 B".to_string()
                            } else if bytes < 1024 {
                                format!("{bytes} B")
                            } else if bytes < 1024 * 1024 {
                                format!("{:.2} KB", (bytes as f64) / 1024.0)
                            } else {
                                format!("{:.2} MB", (bytes as f64) / (1024.0 * 1024.0))
                            };
                            ui.set_janitor_bytes_reclaimable(formatted_size.clone().into());
                            if report.dry_run {
                                ui.set_janitor_status(
                                    format!(
                                        "Inspection complete. Found {} targets.",
                                        report.targets_found.len()
                                    )
                                    .into(),
                                );
                            } else {
                                ui.set_janitor_status(
                                    format!(
                                        "Purged {} targets successfully.",
                                        report.targets_found.len()
                                    )
                                    .into(),
                                );
                                let mut logs = ui.get_stream_logs().to_string();
                                let _ = write!(
                                    logs,
                                    "\n[Janitor] Purged targets: {:?}. Space reclaimed: {formatted_size}\n",
                                    report.targets_found
                                );
                                ui.set_stream_logs(logs.into());
                            }
                        }
                        WorkerMessage::CruftPurged(Err(err)) => {
                            ui.set_working(false);
                            ui.set_janitor_status(format!("Janitor Error: {err}").into());
                        }
                        WorkerMessage::DocStale { path } => {
                            let _ = gui_tx_receiver.send(GuiMessage::LoadDocContent {
                                path,
                                known_hash: None,
                            });
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

    // Callback: trigger-janitor-clean
    let gui_tx_clone = gui_tx.clone();
    let window_weak_clone = window.as_weak();
    let cache_ref_janitor = Arc::clone(&projects_cache);

    window.on_trigger_janitor_clean(move |proj_id_str, dry_run| {
        if let Some(win) = window_weak_clone.upgrade() {
            win.set_working(true);
            if let Ok(lock) = cache_ref_janitor.lock() {
                if let Some(proj) = lock
                    .iter()
                    .find(|p| p.id.to_string() == proj_id_str.as_str())
                {
                    win.set_janitor_status(if dry_run {
                        "Scanning workspace...".into()
                    } else {
                        "Purging workspace...".into()
                    });

                    let _ = gui_tx_clone.send(GuiMessage::PurgeCruft {
                        project_id: proj.id.clone(),
                        project_path: proj.path.clone(),
                        dry_run,
                    });
                } else {
                    win.set_working(false);
                }
            } else {
                win.set_working(false);
            }
        }
    });

    // Callback: trigger-ingest
    let gui_tx_clone = gui_tx.clone();
    let window_weak_clone = window.as_weak();
    window.on_trigger_ingest(move || {
        if let Some(win) = window_weak_clone.upgrade() {
            win.set_working(true);
            let slug = win.get_repo_slug().to_string();
            let path = PathBuf::from(&slug);

            if slug.trim().is_empty() {
                win.set_stream_logs("Error: Repo slug cannot be empty\n".into());
                win.set_working(false);
                return;
            }
            if let Err(e) = gui_tx_clone.send(GuiMessage::ScanProjects { path }) {
                tracing::error!("Worker channel closed unexpectedly: {e}");
                win.set_working(false);
            }
        }
    });

    // Callback: run-command
    let gui_tx_clone = gui_tx.clone();
    let window_weak_clone = window.as_weak();
    let cache_ref = Arc::clone(&projects_cache);
    window.on_run_command(move |proj_name, cmd_name| {
        if let Some(win) = window_weak_clone.upgrade() {
            win.set_working(true);
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
                        let run_id = uuid::Uuid::new_v4();
                        let _ = gui_tx_clone.send(GuiMessage::RunCommand {
                            run_id,
                            project_id: proj.id.clone(),
                            project_path: proj.path.clone(),
                            command_name: cmd.name.clone(),
                            command_str: cmd.command.clone(),
                            use_shell: cmd.use_shell,
                        });
                    } else {
                        win.set_working(false);
                    }
                } else {
                    win.set_working(false);
                }
            } else {
                win.set_working(false);
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
    let gui_tx_timer = gui_tx.clone();
    let window_timer_weak = window.as_weak();
    let last_mtime_checked = Arc::new(std::sync::Mutex::new(None));
    let cache_ref_timer = Arc::clone(&projects_cache);

    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_secs(2),
        move || {
            if let Some(win) = window_timer_weak.upgrade() {
                let selected_idx = win.get_selected_project_index();
                if selected_idx >= 0 && win.get_active_page() == 4 {
                    // Only poll if viewing Docs tab
                    if let Ok(lock) = cache_ref_timer.lock() {
                        if let Some(proj) = lock.get(selected_idx as usize) {
                            let readme_path = proj.path.join("README.md");
                            if readme_path.exists() {
                                let _ = gui_tx_timer.send(GuiMessage::CheckDocFreshness {
                                    path: readme_path,
                                    known_mtime: *last_mtime_checked.lock().unwrap(),
                                });
                            }
                        }
                    }
                }
            }
        },
    );

    window.run()
}
