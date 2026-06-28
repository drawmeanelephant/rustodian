//! Background worker thread for Rustodian Desktop.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use chrono::Utc;

use rustodian_core::log_buffer::LogBuffer;
use rustodian_core::runner::{CommandSpec, DefaultCommandRunner};
use rustodian_core::traits::{CommandRunner, ProjectStore, RunningProcess};
use rustodian_storage::{ProjectLog, SqliteStore};

use crate::message::{GuiMessage, MarkdownBlock, ParsedMarkdown, WorkerMessage};

/// Parse a raw string into Markdown blocks.
pub fn parse_markdown(text: &str) -> ParsedMarkdown {
    let mut blocks = Vec::new();
    let mut in_code_block = false;

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue; // The fence itself isn't a block we render directly here, or we could include it
        }
        if in_code_block {
            blocks.push(MarkdownBlock::CodeFence {
                text: line.to_string(),
            });
            continue;
        }

        if trimmed.is_empty() {
            blocks.push(MarkdownBlock::BlankLine);
            continue;
        }

        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            blocks.push(MarkdownBlock::HorizontalRule);
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("#### ") {
            blocks.push(MarkdownBlock::Header {
                level: 4,
                text: rest.to_string(),
            });
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("### ") {
            blocks.push(MarkdownBlock::Header {
                level: 3,
                text: rest.to_string(),
            });
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            blocks.push(MarkdownBlock::Header {
                level: 2,
                text: rest.to_string(),
            });
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            blocks.push(MarkdownBlock::Header {
                level: 1,
                text: rest.to_string(),
            });
            continue;
        }

        if let Some(rest) = strip_task_prefix(trimmed, true) {
            blocks.push(MarkdownBlock::Task {
                text: rest.to_string(),
                checked: true,
            });
            continue;
        }
        if let Some(rest) = strip_task_prefix(trimmed, false) {
            blocks.push(MarkdownBlock::Task {
                text: rest.to_string(),
                checked: false,
            });
            continue;
        }

        if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            blocks.push(MarkdownBlock::BulletList {
                text: rest.to_string(),
            });
            continue;
        }

        if let Some(dot_pos) = trimmed.find(". ") {
            let prefix = &trimmed[..dot_pos];
            if !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()) {
                blocks.push(MarkdownBlock::NumberedList {
                    number: trimmed[..=dot_pos].to_string(),
                    text: trimmed[dot_pos + 2..].to_string(),
                });
                continue;
            }
        }

        blocks.push(MarkdownBlock::Text {
            text: line.to_string(),
        });
    }

    ParsedMarkdown { blocks }
}

fn strip_task_prefix(line: &str, checked: bool) -> Option<&str> {
    let patterns: &[&str] = if checked {
        &["- [x] ", "- [X] ", "* [x] ", "* [X] "]
    } else {
        &["- [ ] ", "* [ ] "]
    };
    for pat in patterns {
        if let Some(rest) = line.strip_prefix(pat) {
            return Some(rest);
        }
    }
    None
}

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

pub struct WorkerState {
    pub store: Arc<SqliteStore>,
    pub running_process: Option<Box<dyn RunningProcess>>,
    pub is_running: Arc<Mutex<bool>>,
    pub should_kill: Arc<Mutex<bool>>,
}

#[allow(clippy::too_many_lines)]
pub fn run_worker(
    store: Arc<SqliteStore>,
    rx: &std::sync::mpsc::Receiver<GuiMessage>,
    tx: &std::sync::mpsc::Sender<WorkerMessage>,
    ctx: &eframe::egui::Context,
) {
    let mut state = WorkerState {
        store,
        running_process: None,
        is_running: Arc::new(Mutex::new(false)),
        should_kill: Arc::new(Mutex::new(false)),
    };

    while let Ok(msg) = rx.recv() {
        match msg {
            GuiMessage::LoadProjects => {
                let res = state.store.list_projects().map_err(|e| e.to_string());
                let _ = tx.send(WorkerMessage::ProjectsLoaded(res));
                ctx.request_repaint();
            }
            GuiMessage::RunCommand {
                project_id,
                project_path,
                command_name,
                command_str,
            } => {
                // Kill any existing process first
                if let Some(mut proc) = state.running_process.take() {
                    let _ = proc.kill();
                }
                *state.is_running.lock().unwrap() = true;
                *state.should_kill.lock().unwrap() = false;

                let log_buffer = LogBuffer::new();
                let log_buffer_clone = log_buffer.clone();

                let _ = tx.send(WorkerMessage::CommandStatus {
                    command_name: command_name.clone(),
                    is_running: true,
                    exit_status: None,
                    log_buffer: log_buffer.clone(),
                });
                ctx.request_repaint();

                let spec = CommandSpec {
                    program: command_str.clone(),
                    args: vec![],
                    working_dir: project_path,
                    env: HashMap::new(),
                    use_shell: false,
                    capture_output: true,
                };

                let runner = DefaultCommandRunner;
                match runner.spawn(spec) {
                    Ok(mut child) => {
                        let stdout = child.stdout();
                        let stderr = child.stderr();

                        state.running_process = Some(child);

                        let stdout_log = log_buffer.clone();
                        let mut stdout_handle = None;

                        if let Some(so) = stdout {
                            stdout_handle = Some(thread::spawn(move || {
                                use std::io::{BufRead, BufReader};
                                let reader = BufReader::new(so);
                                for line in reader.lines().map_while(Result::ok) {
                                    stdout_log.push_line(line);
                                }
                            }));
                        }

                        let stderr_log = log_buffer.clone();
                        let mut stderr_handle = None;

                        if let Some(se) = stderr {
                            stderr_handle = Some(thread::spawn(move || {
                                use std::io::{BufRead, BufReader};
                                let reader = BufReader::new(se);
                                for line in reader.lines().map_while(Result::ok) {
                                    stderr_log.push_line(line);
                                }
                            }));
                        }

                        // We need to spawn another thread to wait for the process to finish,
                        // so we don't block the worker loop which needs to process KillCommand
                        let is_running_clone = state.is_running.clone();
                        let tx_clone = tx.clone();
                        let store_clone = state.store.clone();
                        let cmd_name = command_name.clone();
                        let ctx_clone = ctx.clone();
                        let should_kill_clone = state.should_kill.clone();

                        // Wait thread
                        thread::spawn(move || {
                            // Wait for streams to finish reading
                            if let Some(h) = stdout_handle {
                                let _ = h.join();
                            }
                            if let Some(h) = stderr_handle {
                                let _ = h.join();
                            }

                            let mut exit_code = None;
                            let killed = *should_kill_clone.lock().unwrap();

                            if killed {
                                exit_code = Some(-1);
                            }

                            let full_log = log_buffer_clone.snapshot();

                            // Save to database
                            let log_record = ProjectLog {
                                id: uuid::Uuid::new_v4().to_string(),
                                project_id: project_id.to_string(),
                                command_name: cmd_name.clone(),
                                exit_code,
                                log_text: full_log,
                                run_at: Utc::now(),
                            };
                            let _ = store_clone.save_log(&log_record);

                            let _ = tx_clone.send(WorkerMessage::CommandStatus {
                                command_name: cmd_name,
                                is_running: false,
                                exit_status: Some(if killed {
                                    "killed".to_string()
                                } else {
                                    "finished".to_string()
                                }),
                                log_buffer: log_buffer_clone,
                            });
                            *is_running_clone.lock().unwrap() = false;
                            ctx_clone.request_repaint();
                        });
                    }
                    Err(e) => {
                        log_buffer.push_line(format!("Failed to spawn process: {e}"));
                        let _ = tx.send(WorkerMessage::CommandStatus {
                            command_name,
                            is_running: false,
                            exit_status: Some("spawn error".to_string()),
                            log_buffer,
                        });
                        *state.is_running.lock().unwrap() = false;
                        ctx.request_repaint();
                    }
                }
            }
            GuiMessage::KillCommand => {
                if let Some(mut proc) = state.running_process.take() {
                    *state.should_kill.lock().unwrap() = true;
                    let _ = proc.kill();
                }
            }
            GuiMessage::DiscoverDocs { project_path } => {
                let available_docs = discover_docs(&project_path);
                let _ = tx.send(WorkerMessage::DocsDiscovered {
                    project_path,
                    available_docs,
                });
                ctx.request_repaint();
            }
            GuiMessage::LoadDocContent { path } => {
                let content = fs::read_to_string(&path)
                    .unwrap_or_else(|e| format!("Error reading file: {e}"));
                let last_modified = fs::metadata(&path).and_then(|m| m.modified()).ok();
                let parsed = parse_markdown(&content);

                let _ = tx.send(WorkerMessage::DocLoaded {
                    content,
                    parsed,
                    last_modified,
                });
                ctx.request_repaint();
            }
        }
    }
}
