#![allow(clippy::collapsible_if)]

use anyhow::{Context, Result};
use eframe::egui;
use rustodian_storage::SqliteStore;
use rustodian_types::Project;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

mod message;
mod worker;

use message::{GuiMessage, MarkdownBlock, ParsedMarkdown, WorkerMessage};
use rustodian_core::log_buffer::LogBuffer;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1024.0, 768.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Rustodian Desktop",
        options,
        Box::new(|cc| {
            let store = match setup_db() {
                Ok(s) => Arc::new(s),
                Err(e) => {
                    eprintln!("Failed to setup DB: {e}");
                    // We might want to handle this better in a real app, but for now just panic
                    panic!("Failed to setup DB: {e}");
                }
            };

            let (gui_tx, worker_rx) = std::sync::mpsc::channel();
            let (worker_tx, gui_rx) = std::sync::mpsc::channel();

            let ctx_clone = cc.egui_ctx.clone();
            let store_clone = store.clone();
            std::thread::spawn(move || {
                worker::run_worker(store_clone, &worker_rx, &worker_tx, &ctx_clone);
            });

            let default_scan_root = dirs::home_dir()
                .map_or_else(|| ".".to_string(), |p| p.to_string_lossy().to_string());
            let scan_root_input = store
                .get_setting("scan_root")
                .ok()
                .flatten()
                .unwrap_or(default_scan_root);

            let mut app = RustodianApp {
                worker_tx: Some(gui_tx),
                worker_rx: Some(gui_rx),
                scan_root_input,

                ..Default::default()
            };
            app.load_projects();
            Ok(Box::new(app))
        }),
    )
}

fn setup_db() -> Result<SqliteStore> {
    let db_path = SqliteStore::default_path().context("failed to determine database path")?;
    let store = SqliteStore::open(&db_path).context("failed to open database")?;
    store.migrate().context("failed to run migrations")?;
    Ok(store)
}

#[derive(PartialEq, Default)]
enum Tab {
    #[default]
    Details,
    GitContext,
    PullRequests,
    Tasks,
    RunnerLogs,
}

struct DocCache {
    available_docs: Vec<(String, std::path::PathBuf)>,
    selected_index: usize,
    content: String,
    parsed: ParsedMarkdown,
    last_modified: Option<SystemTime>,
    content_hash: u64,
    last_checked: Instant,
}

#[derive(Default)]
struct RustodianApp {
    selected_tab: Tab,
    scan_root_input: String,
    scan_status: Option<String>,

    selected_project: Option<Project>,
    projects: Vec<Project>,
    db_error: Option<String>,

    // Channels to/from worker
    worker_tx: Option<std::sync::mpsc::Sender<GuiMessage>>,
    worker_rx: Option<std::sync::mpsc::Receiver<WorkerMessage>>,

    // Command running state
    running_cmd_name: Option<String>,
    running_cmd_log: Option<LogBuffer>,
    running_cmd_status: Option<String>,
    is_running: bool,

    // Document state
    doc_caches: std::collections::HashMap<std::path::PathBuf, DocCache>,

    // Render state
    log_display_buf: String,
}

impl RustodianApp {
    fn send(&self, msg: GuiMessage) {
        if let Some(tx) = &self.worker_tx {
            let _ = tx.send(msg);
        }
    }

    fn load_projects(&mut self) {
        self.send(GuiMessage::LoadProjects);
    }

    fn kill_running_process(&mut self) {
        self.send(GuiMessage::KillCommand);
    }

    fn run_command(
        &mut self,
        command_name: &str,
        command_str: &str,
        project_id: &rustodian_types::ProjectId,
        project_path: &std::path::Path,
        use_shell: bool,
    ) {
        self.send(GuiMessage::RunCommand {
            project_id: project_id.clone(),
            project_path: project_path.to_path_buf(),
            command_name: command_name.to_string(),
            command_str: command_str.to_string(),
            use_shell,
        });
    }

    fn ensure_doc_cache(&mut self, project: &Project) {
        if let Some(cache) = self.doc_caches.get_mut(&project.path) {
            // If it's time to check for freshness
            if cache.last_checked.elapsed() > Duration::from_secs(2) {
                let mut to_check = None;
                if let Some((_, path)) = cache.available_docs.get(cache.selected_index) {
                    to_check = Some((path.clone(), cache.last_modified));
                }
                cache.last_checked = Instant::now();

                if let Some((path, known_mtime)) = to_check {
                    self.send(GuiMessage::CheckDocFreshness { path, known_mtime });
                }
            }
            return;
        }

        // We don't have the cache, so request discovery
        self.send(GuiMessage::DiscoverDocs {
            project_path: project.path.clone(),
        });
    }

    fn reload_selected_doc(&mut self) {
        let to_load = if let Some(proj) = &self.selected_project {
            if let Some(cache) = self.doc_caches.get_mut(&proj.path) {
                cache
                    .available_docs
                    .get(cache.selected_index)
                    .map(|(_, path)| (path.clone(), cache.content_hash))
            } else {
                None
            }
        } else {
            None
        };
        if let Some((path, hash)) = to_load {
            self.send(GuiMessage::LoadDocContent {
                path,
                known_hash: Some(hash),
            });
        }
    }

    #[allow(clippy::too_many_lines)]
    fn process_messages(&mut self) {
        let Some(rx) = &self.worker_rx else { return };
        let mut needs_reload = false;

        while let Ok(msg) = rx.try_recv() {
            match msg {
                WorkerMessage::ProjectsLoaded(Ok(mut p)) => {
                    p.sort_by(|a, b| a.name.cmp(&b.name));

                    // Maintain selected project state
                    if let Some(selected) = &mut self.selected_project {
                        if let Some(fresh) = p.iter().find(|x| x.id == selected.id) {
                            *selected = fresh.clone();
                        } else {
                            self.selected_project = None;
                        }
                    }

                    self.projects = p;
                    self.db_error = None;
                }
                WorkerMessage::ScanComplete(Ok(report)) => {
                    let purge_info = if report.projects_purged > 0 {
                        format!(", {} purged", report.projects_purged)
                    } else {
                        String::new()
                    };
                    self.scan_status = Some(format!(
                        "Scan finished: {} found, {} new, {} updated{}.",
                        report.projects_found,
                        report.projects_new,
                        report.projects_updated,
                        purge_info
                    ));
                }
                WorkerMessage::ScanComplete(Err(e)) => {
                    self.scan_status = Some(format!("Scan failed: {e}"));
                }

                WorkerMessage::ProjectsLoaded(Err(e)) => {
                    self.db_error = Some(format!("Failed to load projects: {e}"));
                }
                WorkerMessage::CommandStatus {
                    command_name,
                    is_running,
                    exit_status,
                    log_buffer,
                } => {
                    self.running_cmd_name = Some(command_name);
                    self.is_running = is_running;
                    if exit_status.is_some() {
                        self.running_cmd_status = exit_status;
                    }
                    self.running_cmd_log = Some(log_buffer);
                }
                WorkerMessage::DocsDiscovered {
                    project_path,
                    available_docs,
                } => {
                    if let Some(proj) = &self.selected_project {
                        if proj.path == project_path {
                            let cache = DocCache {
                                available_docs,
                                selected_index: 0,
                                content: String::new(),
                                parsed: ParsedMarkdown { blocks: vec![] },
                                last_modified: None,
                                content_hash: 0,
                                last_checked: Instant::now(),
                            };

                            if let Some((_, path)) = cache.available_docs.first() {
                                self.send(GuiMessage::LoadDocContent {
                                    path: path.clone(),
                                    known_hash: None,
                                });
                            }

                            self.doc_caches.insert(project_path.clone(), cache);
                        }
                    }
                }
                WorkerMessage::DocLoaded {
                    content,
                    parsed,
                    last_modified,
                    content_hash,
                } => {
                    if let Some(proj) = &self.selected_project {
                        if let Some(cache) = self.doc_caches.get_mut(&proj.path) {
                            cache.content = content;
                            cache.parsed = parsed;
                            cache.last_modified = last_modified;
                            cache.content_hash = content_hash;
                            cache.last_checked = Instant::now();
                        }
                    }
                }
                WorkerMessage::DocUnchanged => {
                    if let Some(proj) = &self.selected_project {
                        if let Some(cache) = self.doc_caches.get_mut(&proj.path) {
                            cache.last_checked = Instant::now();
                        }
                    }
                }
                WorkerMessage::DocStale { path: _ } => {
                    needs_reload = true;
                }
                WorkerMessage::DocFresh { path: _ } => {
                    // It's fresh, do nothing, the last_checked was already reset on dispatch
                }
            }
        }

        if needs_reload {
            self.reload_selected_doc();
        }
    }
}

impl eframe::App for RustodianApp {
    #[allow(clippy::too_many_lines)]
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.process_messages();

        // Throttle repaints slightly to not hog CPU when streaming logs
        if self.is_running {
            ctx.request_repaint_after(Duration::from_millis(32));
        }

        egui::Panel::left("projects_panel")
            .resizable(true)
            .min_size(200.0)
            .show(ui, |ui| {
                ui.heading("🏛️ Projects");
                ui.horizontal(|ui| {
                    ui.label("Root:");
                    ui.text_edit_singleline(&mut self.scan_root_input);
                    if ui.button("Browse...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.scan_root_input = path.to_string_lossy().to_string();
                        }
                    }
                });
                if ui.button("Scan").clicked() {
                    let path = std::path::PathBuf::from(&self.scan_root_input);
                    self.send(GuiMessage::ScanProjects { path: path.clone() });
                    self.scan_status = Some("Scanning...".to_string());
                    if let Some(tx) = &self.worker_tx {
                        let _ = tx.send(GuiMessage::SaveSetting {
                            key: "scan_root".to_string(),
                            value: self.scan_root_input.clone(),
                        });
                    }
                    // Since worker does the scan, we can just let worker do it, or we do it here.
                    // Actually, let's just trigger the scan.
                }
                if let Some(status) = &self.scan_status {
                    ui.label(status);
                }
                ui.separator();

                ui.separator();

                if let Some(err) = &self.db_error {
                    ui.colored_label(egui::Color32::RED, err);
                    return;
                }

                if self.projects.is_empty() {
                    ui.label("No projects found.");
                    ui.label("Run `rustodian scan <path>` first.");
                } else {
                    let mut clicked_index: Option<usize> = None;
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (i, proj) in self.projects.iter().enumerate() {
                            let is_selected =
                                self.selected_project.as_ref().map(|p| &p.id) == Some(&proj.id);
                            if ui.selectable_label(is_selected, &proj.name).clicked() {
                                clicked_index = Some(i);
                            }
                        }
                    });

                    if let Some(idx) = clicked_index {
                        let proj = self.projects[idx].clone();
                        let switching = self
                            .selected_project
                            .as_ref()
                            .is_none_or(|p| p.id != proj.id);
                        if switching {
                            self.kill_running_process();
                            self.running_cmd_name = None;
                            self.running_cmd_log = None;
                            self.running_cmd_status = None;
                            self.is_running = false;
                            // Cache is now preserved across switches
                            // but we can ensure it exists later
                        }
                        self.selected_project = Some(proj);
                    }
                }
            });

        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(project) = self.selected_project.clone() {
                ui.horizontal(|ui| {
                    ui.heading(&project.name);

                    let langs: Vec<String> = project
                        .languages
                        .iter()
                        .map(|l| l.language.to_string())
                        .collect();
                    let lang_str = if langs.is_empty() {
                        "unknown".to_string()
                    } else {
                        langs.join(", ")
                    };

                    ui.label(format!("{lang_str} · local"));
                });
                ui.separator();

                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.selected_tab, Tab::Details, "Details");
                    ui.selectable_value(
                        &mut self.selected_tab,
                        Tab::GitContext,
                        "Git Context (RAG)",
                    );
                    ui.selectable_value(&mut self.selected_tab, Tab::PullRequests, "Pull Requests");
                    ui.selectable_value(&mut self.selected_tab, Tab::Tasks, "Tasks");
                    ui.selectable_value(&mut self.selected_tab, Tab::RunnerLogs, "Runner / Logs");
                });
                ui.separator();

                match self.selected_tab {
                    Tab::Details => {
                        ui.label("Project Details:");
                        ui.label(format!("Path: {}", project.path.display()));

                        if let Some(vcs) = &project.vcs {
                            ui.label(format!(
                                "VCS: {} (branch: {})",
                                vcs.vcs_type,
                                vcs.branch.as_deref().unwrap_or("none")
                            ));
                            if let Some(commit) = &vcs.last_commit {
                                let sha = if commit.sha.len() >= 7 {
                                    &commit.sha[0..7]
                                } else {
                                    &commit.sha
                                };
                                ui.label(format!("Latest Commit: {} - {}", sha, commit.message));
                            }
                        }

                        ui.separator();
                        if ui.button("Purge Workspace Cruft").clicked() {
                            self.run_command(
                                "janitor:clean",
                                "echo 'Worker hook pending'",
                                &project.id,
                                &project.path,
                                false,
                            );
                            self.selected_tab = Tab::RunnerLogs;
                        }
                    }
                    Tab::GitContext => {
                        ui.label("Git Context (RAG):");
                        if ui.button("Get Dirty Files").clicked() {
                            self.run_command(
                                "get_dirty_files",
                                "echo 'Worker hook pending'",
                                &project.id,
                                &project.path,
                                false,
                            );
                            self.selected_tab = Tab::RunnerLogs;
                        }
                        if ui.button("Export RAG Context").clicked() {
                            self.run_command(
                                "export-rag",
                                "cargo xtask export-rag --dirty-only",
                                &project.id,
                                &project.path,
                                false,
                            );
                            self.selected_tab = Tab::RunnerLogs;
                        }
                    }
                    Tab::PullRequests => {
                        ui.label("GitHub PRs will go here.");
                    }
                    Tab::Tasks => {
                        self.render_tasks_tab(ui);
                    }
                    Tab::RunnerLogs => {
                        self.render_runner_logs(ui, &ctx);
                    }
                }
            } else {
                ui.heading("Rustodian Desktop");
                ui.label("Select a project from the left panel.");
            }
        });
    }
}

impl RustodianApp {
    fn render_runner_logs(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        if self.is_running {
            let cmd_name = self.running_cmd_name.clone().unwrap_or_default();
            if let Some(log_buf) = &self.running_cmd_log {
                log_buf.snapshot_into(&mut self.log_display_buf);
            } else {
                self.log_display_buf.clear();
            }

            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!("Running: {cmd_name}"));
                if ui.button("⏹ Stop").clicked() {
                    self.kill_running_process();
                }
            });
            ui.separator();

            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.log_display_buf.as_str())
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY),
                    );
                });
        } else {
            let mut should_clear = false;
            if let Some(log_buf) = &self.running_cmd_log {
                let status_label = self.running_cmd_status.as_deref().unwrap_or("unknown");
                let cmd_name = self.running_cmd_name.as_deref().unwrap_or("unknown");

                let count = log_buf.line_count();
                if count > 0 || status_label != "unknown" {
                    ui.horizontal(|ui| {
                        ui.label(format!("Last run: {cmd_name} — {status_label}"));
                        if ui.small_button("✕ Clear").clicked() {
                            should_clear = true;
                        }
                    });

                    log_buf.snapshot_into(&mut self.log_display_buf);

                    egui::ScrollArea::vertical()
                        .max_height(250.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.log_display_buf.as_str())
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY),
                            );
                        });

                    ui.separator();
                }
            }

            if should_clear {
                self.running_cmd_log = None;
                self.running_cmd_name = None;
                self.running_cmd_status = None;
            }

            let project = self.selected_project.clone();
            if let Some(project) = &project {
                let commands = &project.metadata.commands;
                if commands.is_empty() {
                    ui.label("No commands discovered for this project.");
                    ui.label("Commands are detected from Cargo.toml, package.json, and justfile.");
                } else {
                    ui.label("Available Commands:");
                    ui.add_space(4.0);

                    egui::Grid::new("commands_grid")
                        .num_columns(4)
                        .striped(true)
                        .spacing([12.0, 6.0])
                        .show(ui, |ui| {
                            ui.strong("Name");
                            ui.strong("Source");
                            ui.strong("Command");
                            ui.strong("");
                            ui.end_row();

                            for cmd in commands {
                                ui.label(&cmd.name);
                                ui.label(&cmd.source);
                                ui.monospace(&cmd.command);
                                if ui.button("▶ Run").clicked() {
                                    self.run_command(
                                        &cmd.name,
                                        &cmd.command,
                                        &project.id,
                                        &project.path,
                                        cmd.use_shell,
                                    );
                                    self.selected_tab = Tab::RunnerLogs;
                                }
                                ui.end_row();
                            }
                        });
                }
            }
        }
    }
}

impl RustodianApp {
    fn render_tasks_tab(&mut self, ui: &mut egui::Ui) {
        let Some(project) = self.selected_project.clone() else {
            return;
        };

        self.ensure_doc_cache(&project);

        let has_cache = self.doc_caches.contains_key(&project.path);
        let has_docs = has_cache && !self.doc_caches[&project.path].available_docs.is_empty();
        if !has_docs {
            ui.label(
                "No documentation files (TODO.md, CHANGELOG.md, README.md) found in this project.",
            );
            return;
        }

        let mut needs_reload = false;
        ui.horizontal(|ui| {
            let cache = self.doc_caches.get_mut(&project.path).unwrap();
            for (i, (name, _)) in cache.available_docs.iter().enumerate() {
                if ui
                    .selectable_label(cache.selected_index == i, name)
                    .clicked()
                    && cache.selected_index != i
                {
                    cache.selected_index = i;
                    needs_reload = true;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("\u{1f504} Refresh").clicked() {
                    needs_reload = true;
                }
            });
        });
        ui.separator();

        if needs_reload {
            self.reload_selected_doc();
        }

        let parsed = match self.doc_caches.get(&project.path) {
            Some(c) => c.parsed.clone(),
            None => return,
        };

        egui::ScrollArea::vertical().show(ui, |ui| {
            for block in &parsed.blocks {
                match block {
                    MarkdownBlock::Header { level, text } => {
                        let size = match level {
                            1 => 22.0,
                            2 => 18.0,
                            3 => 15.0,
                            _ => 14.0,
                        };
                        let top_space = match level {
                            1 => 6.0,
                            2 => 4.0,
                            _ => 2.0,
                        };
                        let bot_space = match level {
                            1 => 4.0,
                            _ => 2.0,
                        };
                        ui.add_space(top_space);
                        ui.label(egui::RichText::new(text).size(size).strong());
                        ui.add_space(bot_space);
                    }
                    MarkdownBlock::CodeFence { text } => {
                        ui.monospace(text);
                    }
                    MarkdownBlock::HorizontalRule => {
                        ui.separator();
                    }
                    MarkdownBlock::Task { text, checked } => {
                        ui.horizontal(|ui| {
                            let mut c = *checked;
                            ui.add_enabled(false, egui::Checkbox::without_text(&mut c));
                            ui.label(text);
                        });
                    }
                    MarkdownBlock::BulletList { text } => {
                        ui.horizontal(|ui| {
                            ui.label("  \u{2022}");
                            ui.label(text);
                        });
                    }
                    MarkdownBlock::NumberedList { number, text } => {
                        ui.horizontal(|ui| {
                            ui.label(format!("  {number}"));
                            ui.label(text);
                        });
                    }
                    MarkdownBlock::Text { text } => {
                        ui.label(text);
                    }
                    MarkdownBlock::BlankLine => {
                        ui.add_space(4.0);
                    }
                }
            }
        });
    }
}
