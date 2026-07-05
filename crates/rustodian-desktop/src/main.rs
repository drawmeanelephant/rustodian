#![allow(clippy::too_many_lines, clippy::collapsible_if, clippy::cast_sign_loss)]
pub mod markdown;
pub mod message;
pub mod worker;

slint::include_modules!();

pub mod ui_mapping {
    use crate::SlintProject;
    use crate::SlintProjectCommand;
    use crate::SlintPullRequest;
    use rustodian_types::{Project, ProjectCommand, PullRequest};
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

    #[allow(clippy::cast_possible_truncation)]
    pub fn map_pull_request(pr: &PullRequest) -> SlintPullRequest {
        SlintPullRequest {
            number: pr.number as i32,
            title: SharedString::from(pr.title.clone()),
            author: SharedString::from(pr.author.clone()),
            branch: SharedString::from(pr.branch.clone()),
            url: SharedString::from(pr.url.clone()),
            updated_at: SharedString::from(pr.updated_at.to_rfc3339()),
            is_draft: pr.is_draft,
        }
    }

    pub fn map_pull_requests(prs: &[PullRequest]) -> ModelRc<SlintPullRequest> {
        let slint_prs: Vec<SlintPullRequest> = prs.iter().map(map_pull_request).collect();
        ModelRc::new(VecModel::from(slint_prs))
    }
}

use crate::message::{GuiMessage, MarkdownBlock, WorkerMessage};
use crate::ui_mapping::{map_projects, map_pull_requests};
use rustodian_storage::SqliteStore;
use slint::{ComponentHandle, ModelRc, VecModel};
use std::fmt::Write;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

fn extract_github_slug(url: &str) -> Option<String> {
    let clean = url.trim_end_matches(".git");
    if let Some(pos) = clean.find("github.com") {
        let sub = &clean[pos + 10..]; // Skip "github.com"
        let sub = if sub.starts_with('/') || sub.starts_with(':') {
            &sub[1..]
        } else {
            sub
        };
        let parts: Vec<&str> = sub.split('/').collect();
        if parts.len() >= 2 {
            return Some(format!("{}/{}", parts[0], parts[1]));
        }
    }
    None
}

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

    // 3. Define the repaint trigger
    let window_weak_clone = window_weak.clone();
    let repaint_fn = Arc::new(move || {
        let win_weak = window_weak_clone.clone();
        let _ = slint::invoke_from_event_loop(move || {
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
    let projects_cache = Arc::new(std::sync::Mutex::new(Vec::<rustodian_types::Project>::new()));
    let projects_cache_clone = Arc::clone(&projects_cache);

    // Command process execution tracker
    let active_run_id = Arc::new(std::sync::Mutex::new(Option::<Uuid>::None));
    let active_run_id_receiver = Arc::clone(&active_run_id);

    let gui_tx_receiver_loop = gui_tx.clone();
    std::thread::spawn(move || {
        while let Ok(msg) = worker_rx.recv() {
            let window_inner = window_receiver_weak.clone();
            let cache = Arc::clone(&projects_cache_clone);
            let active_run_id_receiver_clone = Arc::clone(&active_run_id_receiver);
            let gui_tx_receiver = gui_tx_receiver_loop.clone();

            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = window_inner.upgrade() {
                    match msg {
                        WorkerMessage::ProjectsLoaded(Ok(rust_projects)) => {
                            if let Ok(mut lock) = cache.lock() {
                                lock.clone_from(&rust_projects);
                            }
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
                            // Check run isolation boundary to filter stale execution notifications
                            let current_run = active_run_id_receiver_clone.lock().unwrap();
                            if Some(run_id) == *current_run {
                                ui.set_working(is_running);

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
                                    drop(current_run); // Avoid deadlock prior to mut lock
                                    if let Ok(mut lock) = active_run_id_receiver_clone.lock() {
                                        *lock = None;
                                    }
                                }
                            }
                        }
                        WorkerMessage::DocLoaded {
                            content: _,
                            parsed,
                            last_modified: _,
                            content_hash: _,
                        } => {
                            let slint_blocks: Vec<SlintMarkdownBlock> = parsed
                                .blocks
                                .into_iter()
                                .map(|block| match block {
                                    MarkdownBlock::Header { level, text } => SlintMarkdownBlock {
                                        block_type: "heading".into(),
                                        content: text.into(),
                                        level: level.try_into().unwrap_or(0),
                                        is_checked: false,
                                        task_id: "".into(),
                                    },
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
                                    MarkdownBlock::Task { text, checked } => {
                                        let task_id = text.clone();
                                        SlintMarkdownBlock {
                                            block_type: "task".into(),
                                            content: text.into(),
                                            level: 0,
                                            is_checked: checked,
                                            task_id: task_id.into(),
                                        }
                                    }
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
                                            content: format!("{number} {text}").into(),
                                            level: 0,
                                            is_checked: false,
                                            task_id: "".into(),
                                        }
                                    }
                                    MarkdownBlock::HorizontalRule => SlintMarkdownBlock {
                                        block_type: "separator".into(),
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
                        WorkerMessage::PullRequestsLoaded(Ok(prs)) => {
                            ui.set_working(false);
                            ui.set_pr_has_error(false);
                            ui.set_pr_status(
                                format!("Loaded {} open pull requests", prs.len()).into(),
                            );
                            ui.set_pull_requests(map_pull_requests(&prs));
                        }
                        WorkerMessage::PullRequestsLoaded(Err(err)) => {
                            ui.set_working(false);
                            ui.set_pr_has_error(true);
                            ui.set_pr_status(err.into());
                            ui.set_pull_requests(ModelRc::new(VecModel::default()));
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
    let active_run_id_clone = Arc::clone(&active_run_id);
    window.on_run_command(move |proj_name, cmd_name| {
        if let Some(win) = window_weak_clone.upgrade() {
            win.set_working(true);
            let proj_name_str = proj_name.to_string();
            let cmd_name_str = cmd_name.to_string();

            if let Ok(lock) = cache_ref.lock() {
                if let Some(proj) = lock.iter().find(|p| p.name == proj_name_str) {
                    if let Some(cmd) = proj
                        .metadata
                        .commands
                        .iter()
                        .find(|c| c.name == cmd_name_str)
                    {
                        let run_id = Uuid::new_v4();
                        if let Ok(mut run_lock) = active_run_id_clone.lock() {
                            *run_lock = Some(run_id);
                        }
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

    // Callback: trigger-fetch-prs
    let gui_tx_clone = gui_tx.clone();
    let window_weak_clone = window.as_weak();
    window.on_trigger_fetch_prs(move |slug| {
        if let Some(win) = window_weak_clone.upgrade() {
            win.set_working(true);
            win.set_pr_status("Fetching open pull requests...".into());
            let _ = gui_tx_clone.send(GuiMessage::FetchPullRequests {
                repo_slug: slug.to_string(),
            });
        }
    });

    // Run application window loop blocks
    let gui_tx_timer = gui_tx.clone();
    let window_timer_weak = window.as_weak();
    let last_mtime_checked = Arc::new(std::sync::Mutex::new(None));
    let cache_ref_timer = Arc::clone(&projects_cache);

    // Project selection tracker to pre-populate repository slug from remote
    let active_selected_idx = Arc::new(std::sync::Mutex::new(-1));

    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_secs(2),
        move || {
            if let Some(win) = window_timer_weak.upgrade() {
                let selected_idx = win.get_selected_project_index();

                // Track project selection changes to extract repo slug
                if let Ok(mut last_idx) = active_selected_idx.lock() {
                    if selected_idx != *last_idx {
                        *last_idx = selected_idx;
                        if selected_idx >= 0 {
                            if let Ok(lock) = cache_ref_timer.lock() {
                                if let Some(proj) = lock.get(selected_idx as usize) {
                                    if let Some(ref vcs) = proj.vcs {
                                        if let Some(ref remote_url) = vcs.remote_url {
                                            if let Some(slug) = extract_github_slug(remote_url) {
                                                win.set_repo_slug(slug.into());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

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
