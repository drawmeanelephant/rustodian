//! Background worker thread for Rustodian Desktop.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use chrono::Utc;
use tracing::{info, error};

use rustodian_core::log_buffer::LogBuffer;
use rustodian_core::runner::{CommandSpec, DefaultCommandRunner};
use rustodian_core::traits::{CommandRunner, ProjectStore, RunningProcess};
use rustodian_storage::{ProjectLog, SqliteStore};

use crate::message::{GuiMessage, WorkerMessage};

/// Candidate filenames for documentation.
const DOC_CANDIDATES: &[&str] = &[
    "TODO.md",
    "todo.md",
    "CHANGELOG.md",
    "changelog.md",
    "README.md",
    "readme.md",
    "TASKS.md",
    "tasks.md",
    "task.md",
];

fn discover_docs(project_path: &Path) -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    let mut seen_lower = std::collections::HashSet::new();
    for &name in DOC_CANDIDATES {
        let lower = name.to_string().to_lowercase();
        if seen_lower.contains(&lower) {
            continue;
        }
        let full_path = project_path.join(name);
        if full_path.is_file() {
            seen_lower.insert(lower);
            found.push((name.to_string(), full_path));
        }
    }
    found
}

struct WorkerState {
    store: Arc<SqliteStore>,
    running_process: Option<Arc<Mutex<Box<dyn RunningProcess>>>>,
    is_running: Arc<Mutex<bool>>,
    should_kill: Arc<Mutex<bool>>,
    process_exited: Arc<std::sync::atomic::AtomicBool>,
}

pub fn run_worker(
    rx: std::sync::mpsc::Receiver<crate::message::GuiMessage>,
    tx: std::sync::mpsc::Sender<crate::message::WorkerMessage>,
    custodian: std::sync::Arc<rustodian_core::Custodian>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    while let Ok(msg) = rx.recv() {
        match msg {
            GuiMessage::Shutdown => {
                info!("Worker thread received shutdown signal, breaking loop.");
                break;
            }
            GuiMessage::TriggerIngest { path } | GuiMessage::TriggerAgentExport { path } => {
                let log_buffer = LogBuffer::new(100);
                log_buffer.push_line(format!("Starting operation for path: {}", path.display()));

                let _ = tx.send(WorkerMessage::CommandStatus {
                    command_name: "Scanning".to_string(),
                    is_running: true,
                    exit_status: None,
                    log_buffer: log_buffer.clone(),
                });

                let config = rustodian_types::ScanConfig::default();
                let res = custodian.scan(&path, &config).map_err(anyhow::Error::from);

                log_buffer.push_line("Operation completed.".to_string());

                let _ = tx.send(WorkerMessage::CommandStatus {
                    command_name: "Scanning".to_string(),
                    is_running: false,
                    exit_status: Some("finished".to_string()),
                    log_buffer: log_buffer.clone(),
                });

                let _ = tx.send(WorkerMessage::ScanComplete(res));
            }
            GuiMessage::LoadDocContent { path, known_hash } => {
                // look up project folder
                let _project = custodian.find_project(&path.to_string_lossy());
                let content = fs::read_to_string(&path)
                    .unwrap_or_else(|e| format!("Error reading file: {e}"));

                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                std::hash::Hash::hash(&content, &mut hasher);
                let content_hash = std::hash::Hasher::finish(&hasher);

                if Some(content_hash) == known_hash {
                    let _ = tx.send(WorkerMessage::DocUnchanged);
                } else {
                    let last_modified = fs::metadata(&path).and_then(|m| m.modified()).ok();
                    let parsed = crate::markdown::parse_markdown(&content);

                    let _ = tx.send(WorkerMessage::DocLoaded {
                        content,
                        parsed,
                        last_modified,
                        content_hash,
                    });
                }
            }
            GuiMessage::ToggleTask { task_id, target_content, path } => {
                let content = match fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        error!("Failed to read file {:?} for ToggleTask: {}", path, e);
                        continue;
                    }
                };

                let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                let mut modified = false;

                for line in &mut lines {
                    let matches_task_id = task_id.as_ref().map_or(false, |id| line.contains(id));
                    let matches_content = target_content.as_ref().map_or(false, |c| line.contains(c));

                    if matches_task_id || matches_content {
                        if line.contains("- [ ]") {
                            *line = line.replace("- [ ]", "- [x]");
                            modified = true;
                            info!("Toggled task to checked in {:?}", path);
                            break;
                        } else if line.contains("- [x]") || line.contains("- [X]") {
                            *line = line.replace("- [x]", "- [ ]").replace("- [X]", "- [ ]");
                            modified = true;
                            info!("Toggled task to unchecked in {:?}", path);
                            break;
                        }
                    }
                }

                if modified {
                    let new_content = lines.join("
") + "
";
                    if let Err(e) = fs::write(&path, new_content) {
                        error!("Failed to write modified file {:?} for ToggleTask: {}", path, e);
                    }
                } else {
                    info!("No matching task found to toggle in {:?}", path);
                }
            }
            _ => {}
        }
    }
    Ok(())
}
