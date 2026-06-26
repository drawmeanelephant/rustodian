use anyhow::{Context, Result};
use eframe::egui;
use rustodian_core::traits::ProjectStore;
use rustodian_storage::SqliteStore;
use rustodian_types::Project;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1024.0, 768.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Rustodian Desktop",
        options,
        Box::new(|_cc| {
            // Attempt to load the DB
            let store = match setup_db() {
                Ok(s) => Some(Arc::new(s)),
                Err(e) => {
                    eprintln!("Failed to setup DB: {e}");
                    None
                }
            };

            let mut app = RustodianApp {
                store,
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
    PullRequests,
    Tasks,
    RunnerLogs,
}

// ---------------------------------------------------------------------------
// Command runner shared state
// ---------------------------------------------------------------------------

/// Shared state for the background command runner, accessed by both the
/// background reader thread and the GUI thread.
struct CommandRunState {
    /// The name of the command being run (for display purposes).
    command_name: String,
    /// Accumulated stdout + stderr output.
    output_log: String,
    /// Whether the process is still running.
    running: bool,
    /// Exit status message (set when the process finishes).
    exit_status: Option<String>,
}

/// Handle to the spawned child process. Kept on the main thread so the GUI
/// can send a kill signal via the "Stop" button.
struct RunningProcess {
    child: Child,
}

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

#[derive(Default)]
struct RustodianApp {
    selected_tab: Tab,
    selected_project: Option<Project>,
    store: Option<Arc<SqliteStore>>,
    projects: Vec<Project>,
    db_error: Option<String>,
    /// Shared log buffer + status flag, polled every frame when a command is active.
    run_state: Option<Arc<Mutex<CommandRunState>>>,
    /// Holds the `Child` handle so the GUI can kill the process.
    running_process: Option<RunningProcess>,
}

impl RustodianApp {
    fn load_projects(&mut self) {
        if let Some(store) = &self.store {
            match store.list_projects() {
                Ok(mut p) => {
                    p.sort_by(|a, b| a.name.cmp(&b.name));
                    self.projects = p;
                }
                Err(e) => {
                    self.db_error = Some(format!("Failed to load projects: {e}"));
                }
            }
        }
    }

    /// Kill the running process (if any) and clear the run state.
    fn kill_running_process(&mut self) {
        if let Some(mut proc) = self.running_process.take() {
            let _ = proc.child.kill();
        }
    }

    /// Spawn a command in a background thread and wire up output streaming.
    fn run_command(
        &mut self,
        command_name: &str,
        command_str: &str,
        project_path: &std::path::Path,
        ctx: &egui::Context,
    ) {
        // Kill any previously running process first.
        self.kill_running_process();

        let state = Arc::new(Mutex::new(CommandRunState {
            command_name: command_name.to_string(),
            output_log: String::new(),
            running: true,
            exit_status: None,
        }));
        self.run_state = Some(Arc::clone(&state));

        // Spawn the child process using a shell wrapper so that pipes,
        // environment variables, and complex command strings work correctly.
        let spawn_result = Command::new("sh")
            .arg("-c")
            .arg(command_str)
            .current_dir(project_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        let mut child = match spawn_result {
            Ok(c) => c,
            Err(e) => {
                if let Ok(mut s) = state.lock() {
                    s.output_log = format!("Failed to spawn process: {e}\n");
                    s.running = false;
                    s.exit_status = Some("spawn error".to_string());
                }
                return;
            }
        };

        // Take ownership of the pipe handles before moving the child.
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        // Store the child handle so the GUI can kill it.
        self.running_process = Some(RunningProcess { child });

        // Clone what the background thread needs.
        let ctx_clone = ctx.clone();

        thread::spawn(move || {
            // Spawn a secondary thread for stderr so we can read both streams
            // concurrently without deadlocking.
            let stderr_state = Arc::clone(&state);
            let stderr_ctx = ctx_clone.clone();
            let stderr_handle = stderr.map(|se| {
                thread::spawn(move || {
                    let reader = BufReader::new(se);
                    for line in reader.lines() {
                        match line {
                            Ok(l) => {
                                if let Ok(mut s) = stderr_state.lock() {
                                    s.output_log.push_str(&l);
                                    s.output_log.push('\n');
                                }
                                stderr_ctx.request_repaint();
                            }
                            Err(_) => break,
                        }
                    }
                })
            });

            // Read stdout on this thread.
            if let Some(so) = stdout {
                let reader = BufReader::new(so);
                for line in reader.lines() {
                    match line {
                        Ok(l) => {
                            if let Ok(mut s) = state.lock() {
                                s.output_log.push_str(&l);
                                s.output_log.push('\n');
                            }
                            ctx_clone.request_repaint();
                        }
                        Err(_) => break,
                    }
                }
            }

            // Wait for stderr thread to finish.
            if let Some(handle) = stderr_handle {
                let _ = handle.join();
            }

            // Mark the process as finished. We cannot call child.wait() here
            // because the Child is owned by RunningProcess on the main thread.
            // Instead, we just mark running = false; the exit code is best-effort.
            if let Ok(mut s) = state.lock() {
                s.running = false;
                if s.exit_status.is_none() {
                    s.exit_status = Some("finished".to_string());
                }
            }
            ctx_clone.request_repaint();
        });
    }
}

impl eframe::App for RustodianApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("projects_panel")
            .resizable(true)
            .min_width(200.0)
            .show(ctx, |ui| {
                ui.heading("🏛️ Projects");
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
                    // Apply project selection outside the immutable borrow of self.projects.
                    if let Some(idx) = clicked_index {
                        let proj = self.projects[idx].clone();
                        let switching = self
                            .selected_project
                            .as_ref()
                            .is_none_or(|p| p.id != proj.id);
                        if switching {
                            self.kill_running_process();
                            self.run_state = None;
                        }
                        self.selected_project = Some(proj);
                    }
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(project) = &self.selected_project {
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
                    }
                    Tab::PullRequests => {
                        ui.label("GitHub PRs will go here.");
                    }
                    Tab::Tasks => {
                        ui.label("TODO.md / CHANGELOG.md parsing will go here.");
                    }
                    Tab::RunnerLogs => {
                        self.render_runner_logs(ui, ctx);
                    }
                }
            } else {
                ui.heading("Rustodian Desktop");
                ui.label("Select a project from the left panel.");
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Runner / Logs tab rendering (extracted for clarity)
// ---------------------------------------------------------------------------

impl RustodianApp {
    #[allow(clippy::too_many_lines)]
    fn render_runner_logs(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Determine current state by peeking into the shared run_state.
        let is_running = self
            .run_state
            .as_ref()
            .and_then(|s| s.lock().ok().map(|g| g.running))
            .unwrap_or(false);

        if is_running {
            // ---------------------------------------------------------------
            // State B: A command is currently running.
            // ---------------------------------------------------------------
            let (cmd_name, log_snapshot) = self
                .run_state
                .as_ref()
                .and_then(|s| {
                    s.lock()
                        .ok()
                        .map(|g| (g.command_name.clone(), g.output_log.clone()))
                })
                .unwrap_or_default();

            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!("Running: {cmd_name}"));
                if ui.button("⏹ Stop").clicked() {
                    self.kill_running_process();
                    if let Some(state) = &self.run_state
                        && let Ok(mut s) = state.lock()
                    {
                        s.running = false;
                        s.exit_status = Some("killed by user".to_string());
                        s.output_log.push_str("\n--- process killed ---\n");
                    }
                }
            });
            ui.separator();

            // Live output area.
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut log_snapshot.as_str())
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY),
                    );
                });
        } else {
            // ---------------------------------------------------------------
            // State A: No command running (or previous run finished).
            // ---------------------------------------------------------------

            // Show previous run output if available.
            let mut should_clear = false;
            if let Some(state) = &self.run_state
                && let Ok(guard) = state.lock()
                && !guard.output_log.is_empty()
            {
                let status_label = guard.exit_status.as_deref().unwrap_or("unknown");

                ui.horizontal(|ui| {
                    ui.label(format!(
                        "Last run: {} — {}",
                        guard.command_name, status_label
                    ));
                    if ui.small_button("✕ Clear").clicked() {
                        should_clear = true;
                    }
                });

                let log_text = guard.output_log.clone();
                drop(guard);

                egui::ScrollArea::vertical()
                    .max_height(250.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut log_text.as_str())
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY),
                        );
                    });

                ui.separator();
            }
            if should_clear {
                self.run_state = None;
            }

            // Show the command table.
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
                            // Header row.
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
                                    self.run_command(&cmd.name, &cmd.command, &project.path, ctx);
                                }
                                ui.end_row();
                            }
                        });
                }
            }
        }
    }
}
