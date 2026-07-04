//! Background worker thread for Rustodian Desktop.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tracing::{error, info};

use rustodian_core::log_buffer::LogBuffer;
use rustodian_core::traits::RunningProcess;
use rustodian_storage::SqliteStore;

use crate::message::{GuiMessage, WorkerMessage};

/// Candidate filenames for documentation.
#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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
    let mut current_doc_path: Option<PathBuf> = None;

    while let Ok(msg) = rx.recv() {
        match msg {
            GuiMessage::Shutdown => {
                info!("Worker thread received shutdown signal, breaking loop.");
                break;
            }
            GuiMessage::TriggerIngest {
                repo_slug,
                target_project,
            } => {
                let log_buffer = LogBuffer::new();
                log_buffer.push_line(format!(
                    "Starting ingest operation for repo: {}, target: {}",
                    repo_slug, target_project
                ));

                let _ = tx.send(WorkerMessage::CommandStatus {
                    status: "Running".to_string(),
                    log: Some(log_buffer.snapshot()),
                });

                let path = PathBuf::from(&target_project);
                let config = rustodian_types::ScanConfig::default();
                let res = custodian.scan(&path, &config).map_err(anyhow::Error::from);

                log_buffer.push_line("Ingest operation completed.".to_string());

                let _ = tx.send(WorkerMessage::CommandStatus {
                    status: "Finished".to_string(),
                    log: Some(log_buffer.snapshot()),
                });

                let (success, message) = match res {
                    Ok(report) => (true, format!("Scan complete: {:?}", report)),
                    Err(e) => (false, format!("Scan failed: {}", e)),
                };
                let _ = tx.send(WorkerMessage::ScanComplete { success, message });
            }
            GuiMessage::TriggerAgentExport { target_project } => {
                let log_buffer = LogBuffer::new();
                log_buffer.push_line(format!(
                    "Starting export operation for target: {}",
                    target_project
                ));

                let _ = tx.send(WorkerMessage::CommandStatus {
                    status: "Running".to_string(),
                    log: Some(log_buffer.snapshot()),
                });

                let path = PathBuf::from(&target_project);
                let config = rustodian_types::ScanConfig::default();
                let res = custodian.scan(&path, &config).map_err(anyhow::Error::from);

                log_buffer.push_line("Export operation completed.".to_string());

                let _ = tx.send(WorkerMessage::CommandStatus {
                    status: "Finished".to_string(),
                    log: Some(log_buffer.snapshot()),
                });

                let (success, message) = match res {
                    Ok(report) => (true, format!("Scan complete: {:?}", report)),
                    Err(e) => (false, format!("Scan failed: {}", e)),
                };
                let _ = tx.send(WorkerMessage::ScanComplete { success, message });
            }
            GuiMessage::LoadDocContent { path } => {
                let path_buf = PathBuf::from(&path);
                current_doc_path = Some(path_buf.clone());
                let content = fs::read_to_string(&path_buf)
                    .unwrap_or_else(|e| format!("Error reading file: {e}"));

                let blocks = crate::markdown::parse_markdown(&content);

                let _ = tx.send(WorkerMessage::DocLoaded { path, blocks });
            }
            GuiMessage::ToggleTask { task_id, completed } => {
                let path = match &current_doc_path {
                    Some(p) => p.clone(),
                    None => {
                        error!("ToggleTask received but no document is currently loaded.");
                        continue;
                    }
                };

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
                    if line.contains(&task_id) {
                        if completed && line.contains("- [ ]") {
                            *line = line.replace("- [ ]", "- [x]");
                            modified = true;
                            info!("Toggled task to checked in {:?}", path);
                            break;
                        } else if !completed && (line.contains("- [x]") || line.contains("- [X]")) {
                            *line = line.replace("- [x]", "- [ ]").replace("- [X]", "- [ ]");
                            modified = true;
                            info!("Toggled task to unchecked in {:?}", path);
                            break;
                        }
                    }
                }

                if modified {
                    let new_content = lines.join("\n") + "\n";
                    if let Err(e) = fs::write(&path, new_content) {
                        error!(
                            "Failed to write modified file {:?} for ToggleTask: {}",
                            path, e
                        );
                    }
                } else {
                    info!("No matching task found to toggle in {:?}", path);
                }
            }
        }
    }
    Ok(())
}
