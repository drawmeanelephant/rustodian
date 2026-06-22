use eframe::egui;
use rustodian_core::traits::ProjectStore;
use rustodian_storage::SqliteStore;
use rustodian_types::Project;
use std::sync::Arc;
use anyhow::{Context, Result};

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

            let mut app = RustodianApp { store, ..Default::default() };
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

#[derive(Default)]
struct RustodianApp {
    selected_tab: Tab,
    selected_project: Option<Project>,
    store: Option<Arc<SqliteStore>>,
    projects: Vec<Project>,
    db_error: Option<String>,
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
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for proj in &self.projects {
                            let is_selected = self.selected_project.as_ref().map(|p| &p.id) == Some(&proj.id);
                            if ui.selectable_label(is_selected, &proj.name).clicked() {
                                self.selected_project = Some(proj.clone());
                            }
                        }
                    });
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(project) = &self.selected_project {
                ui.horizontal(|ui| {
                    ui.heading(&project.name);

                    let langs: Vec<String> = project.languages.iter().map(|l| l.language.to_string()).collect();
                    let lang_str = if langs.is_empty() { "unknown".to_string() } else { langs.join(", ") };

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
                            ui.label(format!("VCS: {} (branch: {})", vcs.vcs_type, vcs.branch.as_deref().unwrap_or("none")));
                            if let Some(commit) = &vcs.last_commit {
                                let sha = if commit.sha.len() >= 7 { &commit.sha[0..7] } else { &commit.sha };
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
                        ui.label("Process runner logs will go here.");
                    }
                }
            } else {
                ui.heading("Rustodian Desktop");
                ui.label("Select a project from the left panel.");
            }
        });
    }
}
