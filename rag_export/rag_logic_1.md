# RAG Export - Logic (Part 1)

### Path: ./test_symlink.rs
```
use std::os::unix::fs::symlink;
fn main() {
}

```

### Path: ./get_err.sh
```
sed -i 's/assert!(err.to_string().contains("invalid metadata JSON"));/println!("{}", err.to_string()); assert!(err.to_string().contains("invalid metadata JSON"));/' crates/rustodian-storage/src/store.rs

```

### Path: ./xtask/src/export_rag.rs
```
use ignore::WalkBuilder;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

const MAX_LINES_PER_FILE: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Category {
    Logic,
    Config,
    Content,
    Misc,
    Excluded,
}

impl Category {
    fn prefix(&self) -> &'static str {
        match self {
            Category::Logic => "rag_logic",
            Category::Config => "rag_config",
            Category::Content => "rag_content",
            Category::Misc => "rag_misc",
            Category::Excluded => "rag_excluded",
        }
    }
}

pub fn export_rag(dirty_only: bool) {
    println!("Exporting RAG friendly archives...");
    if dirty_only {
        println!("Mode: --dirty-only (filtering to git-dirty files only)");
    }

    let out_dir = Path::new("rag_export");
    if out_dir.exists() {
        fs::remove_dir_all(out_dir).expect("Failed to clear existing rag_export directory");
    }
    fs::create_dir_all(out_dir).expect("Failed to create rag_export directory");

    // ── Optional dirty-file filter ────────────────────────────────────────
    let dirty_filter: Option<HashSet<std::path::PathBuf>> = if dirty_only {
        use rustodian_core::traits::GitInspector;
        let inspector = rustodian_git::Git2Inspector;
        match inspector.get_dirty_files(Path::new(".")) {
            Ok(files) => {
                let set: HashSet<_> = files
                    .into_iter()
                    .map(|f| {
                        // Canonicalize for reliable comparison
                        f.canonicalize().unwrap_or(f)
                    })
                    .collect();
                println!("  Found {} dirty file(s) to export.", set.len());
                Some(set)
            }
            Err(e) => {
                eprintln!("Warning: could not query git status: {e}. Exporting all files.");
                None
            }
        }
    } else {
        None
    };

    let mut walker = WalkBuilder::new(".");
    walker.hidden(false); // We might want to see some hidden files like .gitignore, .github/
    walker.filter_entry(|e| {
        let name = e.file_name().to_string_lossy();
        if name == ".git" || name == "target" || name == "rag_export" {
            return false;
        }
        true
    });

    struct CategoryWriter {
        category: Category,
        file_index: usize,
        current_lines: usize,
        file: Option<File>,
    }

    impl CategoryWriter {
        fn new(category: Category) -> Self {
            Self {
                category,
                file_index: 1,
                current_lines: 0,
                file: None,
            }
        }

        fn get_file(&mut self) -> io::Result<&mut File> {
            if self.file.is_none() || self.current_lines >= MAX_LINES_PER_FILE {
                if self.file.is_some() {
                    self.file_index += 1;
                }
                let filename = format!("{}_{}.md", self.category.prefix(), self.file_index);
                let path = Path::new("rag_export").join(filename);
                let mut f = File::create(path)?;
                writeln!(
                    f,
                    "# RAG Export - {:?} (Part {})\n",
                    self.category, self.file_index
                )?;
                self.file = Some(f);
                self.current_lines = 2; // header
            }
            Ok(self.file.as_mut().unwrap())
        }

        fn write_entry(&mut self, path: &str, content: &str) -> io::Result<()> {
            let lines_in_content = content.lines().count() + 6; // +6 for markdown formatting
            let f = self.get_file()?;
            writeln!(f, "### Path: {}", path)?;
            writeln!(f, "```")?;
            writeln!(f, "{}", content)?;
            writeln!(f, "```\n")?;
            self.current_lines += lines_in_content;
            Ok(())
        }

        fn write_excluded(&mut self, path: &str, reason: &str) -> io::Result<()> {
            let f = self.get_file()?;
            writeln!(f, "- **Excluded:** `{}` (Reason: {})", path, reason)?;
            self.current_lines += 1;
            Ok(())
        }
    }

    let mut writers = HashMap::new();
    for &cat in &[
        Category::Logic,
        Category::Config,
        Category::Content,
        Category::Misc,
        Category::Excluded,
    ] {
        writers.insert(cat, CategoryWriter::new(cat));
    }

    for result in walker.build() {
        let entry = match result {
            Ok(entry) => entry,
            Err(_) => continue,
        };

        let path = entry.path();
        if path.is_dir() {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();

        // ── Dirty-only filter gate ────────────────────────────────────────
        if let Some(ref filter) = dirty_filter {
            let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            if !filter.contains(&canon) {
                continue;
            }
        }

        // Skip some common generated or unhelpful stuff
        if path_str.contains("Cargo.lock") {
            continue;
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        let category = match ext {
            "rs" | "py" | "js" | "ts" | "go" | "sh" | "c" | "cpp" | "h" => Category::Logic,
            "toml" | "json" | "yml" | "yaml" | "ini" => Category::Config,
            "md" | "txt" | "csv" => Category::Content,
            _ => {
                if file_name == "justfile"
                    || file_name == "Dockerfile"
                    || file_name == ".gitignore"
                    || file_name == ".editorconfig"
                {
                    Category::Config
                } else if file_name == "LICENSE-APACHE" || file_name == "LICENSE-MIT" {
                    Category::Content
                } else {
                    Category::Misc
                }
            }
        };

        // Try to read content
        match fs::read_to_string(path) {
            Ok(content) => {
                let writer = writers.get_mut(&category).unwrap();
                writer
                    .write_entry(&path_str, &content)
                    .expect("Failed to write entry");
            }
            Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                let writer = writers.get_mut(&Category::Excluded).unwrap();
                writer
                    .write_excluded(&path_str, "Binary file / Invalid UTF-8")
                    .expect("Failed to write excluded");
            }
            Err(e) => {
                let writer = writers.get_mut(&Category::Excluded).unwrap();
                writer
                    .write_excluded(&path_str, &format!("Read error: {}", e))
                    .expect("Failed to write excluded");
            }
        }
    }

    println!("✅ RAG archives generated in rag_export/ directory.");
}

```

### Path: ./xtask/src/main.rs
```
//! # xtask
//!
//! Workspace-level automation tasks for Rustodian.
//!
//! Run with: `cargo xtask <command>`
//! Or via justfile: `just xtask <command>`

use std::process::Command;

mod export_rag;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("coverage") => coverage(),
        Some("lint") => lint(),
        Some("dist") => dist(),
        Some("export-rag") => {
            let dirty_only = args.iter().any(|a| a == "--dirty-only");
            export_rag::export_rag(dirty_only);
        }
        Some("help") | None => help(),
        Some(unknown) => {
            eprintln!("Unknown command: {unknown}");
            eprintln!();
            help();
            std::process::exit(1);
        }
    }
}

fn help() {
    println!("Rustodian xtask - workspace automation");
    println!();
    println!("USAGE: cargo xtask <COMMAND>");
    println!();
    println!("COMMANDS:");
    println!("  coverage    Run tests with coverage reporting");
    println!("  lint        Run all lints (fmt + clippy + doc)");
    println!("  dist        Build release binaries");
    println!("  export-rag  Export codebase to RAG-friendly markdown files");
    println!("              --dirty-only  Only export git-dirty files (untracked/modified/staged)");
    println!("  help        Show this help");
}

fn coverage() {
    println!("Running tests with coverage...");
    let status = Command::new("cargo")
        .args(["test", "--workspace"])
        .status()
        .expect("failed to run cargo test");

    if !status.success() {
        std::process::exit(1);
    }
    println!(
        "Coverage reporting not yet configured. \
         Run `cargo install cargo-tarpaulin` to set up."
    );
}

fn lint() {
    println!("Running all lints...");

    let checks = [
        ("cargo", vec!["fmt", "--all", "--", "--check"]),
        (
            "cargo",
            vec![
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        ),
        ("cargo", vec!["doc", "--workspace", "--no-deps"]),
    ];

    for (cmd, args) in &checks {
        println!("\n→ {} {}", cmd, args.join(" "));
        let status = Command::new(cmd)
            .args(args)
            .status()
            .unwrap_or_else(|e| panic!("failed to run {cmd}: {e}"));

        if !status.success() {
            eprintln!("\nLint failed!");
            std::process::exit(1);
        }
    }

    println!("\n✅ All lints passed!");
}

fn dist() {
    println!("Building release binary...");
    let status = Command::new("cargo")
        .args(["build", "--release", "-p", "rustodian-cli"])
        .status()
        .expect("failed to run cargo build");

    if !status.success() {
        std::process::exit(1);
    }
    println!("Binary at: target/release/rustodian");
}

```

### Path: ./run_test_runner.rs
```
// I will just use cargo test to run tests in runner.rs

```

### Path: ./crates/rustodian-desktop/src/markdown.rs
```
use crate::message::MarkdownBlock;

/// Parse a raw string into Markdown blocks.
pub(crate) fn parse_markdown(text: &str) -> Vec<MarkdownBlock> {
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

    blocks
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_markdown_tasks() {
        let input = "- [ ] task one\n- [x] task two\n";
        insta::assert_debug_snapshot!(parse_markdown(input));
    }

    #[test]
    fn test_parse_markdown_commands() {
        let input = "## Commands\n```\ncargo test\n```\n";
        insta::assert_debug_snapshot!(parse_markdown(input));
    }
}

```

### Path: ./crates/rustodian-desktop/src/message.rs
```
//! Message passing types for the Desktop GUI.

use std::path::PathBuf;
use std::time::SystemTime;
use uuid::Uuid;

use rustodian_core::log_buffer::LogBuffer;
use rustodian_types::{Project, ProjectId};

/// Messages sent from the GUI thread to the Background Worker thread.
pub enum GuiMessage {
    /// Load all projects from the database.
    LoadProjects,
    /// Run a command for a project.
    RunCommand {
        run_id: Uuid,
        project_id: ProjectId,
        project_path: PathBuf,
        command_name: String,
        command_str: String,
        use_shell: bool,
    },
    /// Kill the currently running command (if any).
    KillCommand,
    /// Discover documentation files in a project root.
    DiscoverDocs { project_path: PathBuf },
    /// Check if a specific document file is fresh.
    CheckDocFreshness {
        path: PathBuf,
        known_mtime: Option<SystemTime>,
    },
    /// Scan projects.
    ScanProjects { path: PathBuf },
    /// Purge cruft files.
    PurgeCruft {
        project_id: ProjectId,
        project_path: PathBuf,
        dry_run: bool,
    },
    /// Get dirty files list.
    GetDirtyFiles { project_path: PathBuf },
    /// Save a setting to the database.
    SaveSetting { key: String, value: String },
    /// Load the content of a specific document file.
    LoadDocContent {
        path: PathBuf,
        known_hash: Option<u64>,
    },
    /// Update a specific task markdown line checkbox state.
    ToggleTask { task_id: String, completed: bool },
    /// Fetch open pull requests for a given repository slug.
    FetchPullRequests { repo_slug: String },
    /// Load all settings from the database.
    LoadSettings,
}

/// A parsed markdown block.
#[derive(Debug, Clone)]
pub enum MarkdownBlock {
    Header { level: usize, text: String },
    CodeFence { text: String },
    HorizontalRule,
    Task { text: String, checked: bool },
    BulletList { text: String },
    NumberedList { number: String, text: String },
    Text { text: String },
    BlankLine,
}

/// Memoized markdown content.
#[derive(Debug, Clone)]
pub struct ParsedMarkdown {
    pub blocks: Vec<MarkdownBlock>,
}

/// Messages sent from the Background Worker thread to the GUI thread.
pub enum WorkerMessage {
    /// Streams incremental chunked log lines back to the UI.
    CommandStatus {
        run_id: Uuid,
        command_name: String,
        is_running: bool,
        exit_status: Option<String>,
        log_buffer: LogBuffer,
    },
    /// Signals end of a scan run.
    ScanComplete(Result<rustodian_core::custodian::ScanReport, anyhow::Error>),
    /// Notifies UI of projects available in the store.
    ProjectsLoaded(Result<Vec<Project>, String>),
    /// Result of discovering documentation files.
    DocsDiscovered {
        project_path: PathBuf,
        available_docs: Vec<(String, PathBuf)>,
    },
    DocStale {
        path: PathBuf,
    },
    DocFresh {
        path: PathBuf,
    },
    /// Result of running the digital janitor.
    CruftPurged(Result<rustodian_core::janitor::JanitorReport, String>),
    /// Result of getting dirty files from git inspector.
    DirtyFilesResult(Result<Vec<PathBuf>, String>),
    /// Result when content has not changed.
    DocUnchanged,
    /// Returns structural parsed markdown blocks.
    DocLoaded {
        content: String,
        parsed: ParsedMarkdown,
        last_modified: Option<SystemTime>,
        content_hash: u64,
    },
    /// Returns fetched Pull Requests.
    PullRequestsLoaded(Result<Vec<rustodian_types::PullRequest>, String>),
    /// Returns all settings loaded from the database.
    SettingsLoaded(std::collections::HashMap<String, String>),
}

```

### Path: ./crates/rustodian-desktop/src/worker.rs
```
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

use crate::message::{GuiMessage, ParsedMarkdown, WorkerMessage};

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

#[allow(clippy::too_many_lines)]
pub fn run_worker(
    store: Arc<SqliteStore>,
    rx: &std::sync::mpsc::Receiver<GuiMessage>,
    tx: &std::sync::mpsc::Sender<WorkerMessage>,
    repaint_fn: &std::sync::Arc<dyn Fn() + Send + Sync>,
) {
    let mut state = WorkerState {
        store,
        running_process: None,
        is_running: Arc::new(Mutex::new(false)),
        should_kill: Arc::new(Mutex::new(false)),
        process_exited: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };

    let mut current_doc_path: Option<PathBuf> = None;
    while let Ok(msg) = rx.recv() {
        match msg {
            GuiMessage::LoadProjects => {
                let res = state.store.list_projects().map_err(|e| e.to_string());
                let _ = tx.send(WorkerMessage::ProjectsLoaded(res));
                repaint_fn();
            }
            GuiMessage::RunCommand {
                run_id,
                project_id,
                project_path,
                command_name,
                command_str,
                use_shell,
            } => {
                // Kill any existing process first
                if let Some(proc_arc) = state.running_process.take() {
                    if !state
                        .process_exited
                        .load(std::sync::atomic::Ordering::SeqCst)
                    {
                        let mut proc = proc_arc.lock().unwrap();
                        let _ = proc.kill();
                    }
                }
                *state.is_running.lock().unwrap() = true;
                *state.should_kill.lock().unwrap() = false;
                state
                    .process_exited
                    .store(false, std::sync::atomic::Ordering::SeqCst);

                let log_buffer = LogBuffer::new();
                let log_buffer_clone = log_buffer.clone();

                let _ = tx.send(WorkerMessage::CommandStatus {
                    run_id,
                    command_name: command_name.clone(),
                    is_running: true,
                    exit_status: None,
                    log_buffer: log_buffer.clone(),
                });
                repaint_fn();

                let spec = CommandSpec {
                    program: command_str.clone(),
                    args: vec![],
                    working_dir: project_path,
                    env: HashMap::new(),
                    use_shell,
                    capture_output: true,
                };

                let runner = DefaultCommandRunner;
                match runner.spawn(spec) {
                    Ok(mut child) => {
                        let stdout = child.stdout();
                        let stderr = child.stderr();

                        let process_arc = Arc::new(Mutex::new(child));
                        state.running_process = Some(process_arc.clone());

                        let stdout_log = log_buffer.clone();
                        let mut stdout_handle = None;
                        let tx_stdout = tx.clone();
                        let repaint_stdout = repaint_fn.clone();
                        let cmd_stdout = command_name.clone();

                        if let Some(so) = stdout {
                            stdout_handle = Some(thread::spawn(move || {
                                use std::io::{BufRead, BufReader};
                                let reader = BufReader::new(so);
                                let mut last_send = std::time::Instant::now();
                                for line in reader.lines().map_while(Result::ok) {
                                    stdout_log.push_line(line);
                                    if last_send.elapsed() > std::time::Duration::from_millis(100) {
                                        let _ = tx_stdout.send(WorkerMessage::CommandStatus {
                                            run_id,
                                            command_name: cmd_stdout.clone(),
                                            is_running: true,
                                            exit_status: None,
                                            log_buffer: stdout_log.clone(),
                                        });
                                        repaint_stdout();
                                        last_send = std::time::Instant::now();
                                    }
                                }
                            }));
                        }

                        let stderr_log = log_buffer.clone();
                        let mut stderr_handle = None;
                        let tx_stderr = tx.clone();
                        let repaint_stderr = repaint_fn.clone();
                        let cmd_stderr = command_name.clone();

                        if let Some(se) = stderr {
                            stderr_handle = Some(thread::spawn(move || {
                                use std::io::{BufRead, BufReader};
                                let reader = BufReader::new(se);
                                let mut last_send = std::time::Instant::now();
                                for line in reader.lines().map_while(Result::ok) {
                                    stderr_log.push_line(line);
                                    if last_send.elapsed() > std::time::Duration::from_millis(100) {
                                        let _ = tx_stderr.send(WorkerMessage::CommandStatus {
                                            run_id,
                                            command_name: cmd_stderr.clone(),
                                            is_running: true,
                                            exit_status: None,
                                            log_buffer: stderr_log.clone(),
                                        });
                                        repaint_stderr();
                                        last_send = std::time::Instant::now();
                                    }
                                }
                            }));
                        }

                        let is_running_clone = state.is_running.clone();
                        let tx_clone = tx.clone();
                        let store_clone = state.store.clone();
                        let cmd_name = command_name.clone();
                        let repaint_fn_clone = repaint_fn.clone();
                        let should_kill_clone = state.should_kill.clone();
                        let process_exited_clone = state.process_exited.clone();

                        // Wait thread
                        thread::spawn(move || {
                            // Wait for streams to finish reading
                            if let Some(h) = stdout_handle {
                                let _ = h.join();
                            }
                            if let Some(h) = stderr_handle {
                                let _ = h.join();
                            }

                            process_exited_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                            let mut proc = process_arc.lock().unwrap();
                            let exit_status = proc.wait().ok().flatten();

                            let mut exit_code = exit_status;
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
                                run_id,
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
                            repaint_fn_clone();
                        });
                    }
                    Err(e) => {
                        log_buffer.push_line(format!("Failed to spawn process: {e}"));
                        let _ = tx.send(WorkerMessage::CommandStatus {
                            run_id,
                            command_name,
                            is_running: false,
                            exit_status: Some("spawn error".to_string()),
                            log_buffer,
                        });
                        *state.is_running.lock().unwrap() = false;
                        repaint_fn();
                    }
                }
            }
            GuiMessage::KillCommand => {
                if let Some(proc_arc) = state.running_process.take() {
                    *state.should_kill.lock().unwrap() = true;
                    if !state
                        .process_exited
                        .load(std::sync::atomic::Ordering::SeqCst)
                    {
                        let mut proc = proc_arc.lock().unwrap();
                        let _ = proc.kill();
                    }
                }
            }
            GuiMessage::DiscoverDocs { project_path } => {
                let available_docs = discover_docs(&project_path);
                let _ = tx.send(WorkerMessage::DocsDiscovered {
                    project_path,
                    available_docs,
                });
                repaint_fn();
            }
            GuiMessage::CheckDocFreshness { path, known_mtime } => {
                let current_mtime = fs::metadata(&path).and_then(|m| m.modified()).ok();
                if known_mtime == current_mtime {
                    let _ = tx.send(WorkerMessage::DocFresh { path });
                } else {
                    let _ = tx.send(WorkerMessage::DocStale { path });
                }
                repaint_fn();
            }
            GuiMessage::ScanProjects { path } => {
                let scanner = rustodian_scanner::FsScanner;
                let git = rustodian_git::Git2Inspector;
                let runner = rustodian_core::runner::DefaultCommandRunner;
                let custodian = rustodian_core::Custodian::new(
                    Box::new((*state.store).clone()),
                    Box::new(scanner),
                    Box::new(git),
                    Box::new(runner),
                );
                let res = custodian
                    .scan(&path, &rustodian_types::ScanConfig::default())
                    .map_err(anyhow::Error::from);
                let _ = tx.send(WorkerMessage::ScanComplete(res));
                let list_res = state.store.list_projects().map_err(|e| e.to_string());
                let _ = tx.send(WorkerMessage::ProjectsLoaded(list_res));
                repaint_fn();
            }

            GuiMessage::PurgeCruft {
                project_id,
                project_path: _,
                dry_run,
            } => {
                let scanner = rustodian_scanner::FsScanner;
                let git = rustodian_git::Git2Inspector;
                let runner = rustodian_core::runner::DefaultCommandRunner;
                let custodian = rustodian_core::Custodian::new(
                    Box::new((*state.store).clone()),
                    Box::new(scanner),
                    Box::new(git),
                    Box::new(runner),
                );

                let res = match state.store.get_project(&project_id) {
                    Ok(Some(project)) => {
                        let janitor = rustodian_core::janitor::DigitalJanitor::new(&custodian);
                        janitor.clean(&project, dry_run).map_err(|e| e.to_string())
                    }
                    Ok(None) => Err("Project not found".to_string()),
                    Err(e) => Err(e.to_string()),
                };

                let _ = tx.send(WorkerMessage::CruftPurged(res));
                repaint_fn();
            }
            GuiMessage::GetDirtyFiles { project_path } => {
                let git = rustodian_git::Git2Inspector;
                let res =
                    rustodian_core::traits::GitInspector::get_dirty_files(&git, &project_path)
                        .map_err(|e| e.to_string());
                let _ = tx.send(WorkerMessage::DirtyFilesResult(res));
                repaint_fn();
            }
            GuiMessage::SaveSetting { key, value } => {
                let _ = state.store.set_setting(&key, &value);
            }
            GuiMessage::LoadSettings => {
                let settings = state.store.list_settings().unwrap_or_default();
                let _ = tx.send(WorkerMessage::SettingsLoaded(settings));
                repaint_fn();
            }

            GuiMessage::LoadDocContent { path, known_hash } => {
                current_doc_path = Some(path.clone());
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
                        parsed: ParsedMarkdown { blocks: parsed },
                        last_modified,
                        content_hash,
                    });
                }
                repaint_fn();
            }

            GuiMessage::ToggleTask { task_id, completed } => {
                let path = match &current_doc_path {
                    Some(p) => p.clone(),
                    None => {
                        continue;
                    }
                };

                let Ok(content) = fs::read_to_string(&path) else {
                    continue;
                };

                let mut lines: Vec<String> = content.lines().map(ToString::to_string).collect();
                let mut modified = false;

                for line in &mut lines {
                    if line.contains(&task_id) {
                        if completed && line.contains("- [ ]") {
                            *line = line.replace("- [ ]", "- [x]");
                            modified = true;
                            break;
                        } else if !completed && (line.contains("- [x]") || line.contains("- [X]")) {
                            *line = line.replace("- [x]", "- [ ]").replace("- [X]", "- [ ]");
                            modified = true;
                            break;
                        }
                    }
                }

                if modified {
                    let new_content = lines.join("\n") + "\n";
                    let _ = fs::write(&path, new_content);
                }
            }

            GuiMessage::FetchPullRequests { repo_slug } => {
                let downloader = rustodian_remote::GithubDownloader::new();

                // Build a short-lived Tokio context for the sync worker
                match tokio::runtime::Runtime::new() {
                    Ok(rt) => {
                        let res = rt.block_on(async {
                            use rustodian_core::traits::PullRequestFetcher;
                            downloader.fetch_open_prs(&repo_slug).await
                        });

                        match res {
                            Ok(prs) => {
                                let _ = tx.send(WorkerMessage::PullRequestsLoaded(Ok(prs)));
                            }
                            Err(e) => {
                                let err_msg = if matches!(
                                    e,
                                    rustodian_core::CoreError::RateLimitExceeded
                                ) {
                                    "API rate limit exceeded. Set GITHUB_TOKEN to increase limits."
                                        .to_string()
                                } else {
                                    e.to_string()
                                };
                                let _ = tx.send(WorkerMessage::PullRequestsLoaded(Err(err_msg)));
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(WorkerMessage::PullRequestsLoaded(Err(format!(
                            "Tokio init failure: {e}"
                        ))));
                    }
                }
                repaint_fn();
            }
        }
    }
}

```

### Path: ./crates/rustodian-desktop/src/main.rs
```
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

    let last_saved_repo_slug = Arc::new(std::sync::Mutex::new(String::new()));
    let last_saved_target_project = Arc::new(std::sync::Mutex::new(String::new()));

    let last_saved_repo_slug_receiver = Arc::clone(&last_saved_repo_slug);
    let last_saved_target_project_receiver = Arc::clone(&last_saved_target_project);

    let gui_tx_receiver_loop = gui_tx.clone();
    std::thread::spawn(move || {
        while let Ok(msg) = worker_rx.recv() {
            let window_inner = window_receiver_weak.clone();
            let cache = Arc::clone(&projects_cache_clone);
            let active_run_id_receiver_clone = Arc::clone(&active_run_id_receiver);
            let gui_tx_receiver = gui_tx_receiver_loop.clone();

            let last_slug_cache = Arc::clone(&last_saved_repo_slug_receiver);
            let last_target_cache = Arc::clone(&last_saved_target_project_receiver);

            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = window_inner.upgrade() {
                    match msg {
                        WorkerMessage::SettingsLoaded(settings) => {
                            if let Some(repo_slug) = settings.get("repo_slug") {
                                ui.set_repo_slug(repo_slug.as_str().into());
                                if let Ok(mut lock) = last_slug_cache.lock() {
                                    (*lock).clone_from(repo_slug);
                                }
                            }
                            if let Some(target_project) = settings.get("target_project") {
                                ui.set_target_project(target_project.as_str().into());
                                if let Ok(mut lock) = last_target_cache.lock() {
                                    (*lock).clone_from(target_project);
                                }
                            }
                        }
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
    let _ = gui_tx.send(GuiMessage::LoadSettings);

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

    let last_saved_repo_slug_timer = Arc::clone(&last_saved_repo_slug);
    let last_saved_target_project_timer = Arc::clone(&last_saved_target_project);
    let gui_tx_timer_clone = gui_tx_timer.clone();

    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_secs(2),
        move || {
            if let Some(win) = window_timer_weak.upgrade() {
                let selected_idx = win.get_selected_project_index();

                let current_slug = win.get_repo_slug().to_string();
                let current_target = win.get_target_project().to_string();

                if let Ok(mut last_slug) = last_saved_repo_slug_timer.lock() {
                    if !current_slug.is_empty() && current_slug != *last_slug {
                        (*last_slug).clone_from(&current_slug);
                        let _ = gui_tx_timer_clone.send(GuiMessage::SaveSetting {
                            key: "repo_slug".to_string(),
                            value: current_slug,
                        });
                    }
                }

                if let Ok(mut lock) = last_saved_target_project_timer.lock() {
                    if !current_target.is_empty() && current_target != *lock {
                        (*lock).clone_from(&current_target);
                        let _ = gui_tx_timer_clone.send(GuiMessage::SaveSetting {
                            key: "target_project".to_string(),
                            value: current_target,
                        });
                    }
                }

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

    let gui_close_tx = gui_tx.clone();
    window.window().on_close_requested(move || {
        let _ = gui_close_tx.send(GuiMessage::KillCommand);
        slint::CloseRequestResponse::HideWindow
    });

    window.run()
}

```

### Path: ./crates/rustodian-desktop/build.rs
```
fn main() {
    slint_build::compile("ui/pipeline.slint").unwrap();
}

```

### Path: ./crates/rustodian-git/src/error.rs
```
//! Git-specific error types.

use rustodian_core::CoreError;

/// Errors specific to git inspection.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// Error from libgit2.
    #[error("git2 error: {0}")]
    Git2(#[from] git2::Error),
}

impl From<GitError> for CoreError {
    fn from(err: GitError) -> Self {
        CoreError::Git(err.to_string())
    }
}

```

### Path: ./crates/rustodian-git/src/lib.rs
```
//! # Rustodian Git
//!
//! Git repository inspection for Rustodian.
//!
//! Uses `git2` (libgit2 bindings) to extract repository information
//! without requiring a `git` binary on the system.

pub mod error;
pub mod inspector;

pub use inspector::Git2Inspector;

```

### Path: ./crates/rustodian-git/src/inspector.rs
```
//! Git2-based implementation of [`GitInspector`].

use std::path::Path;

use git2::{Repository, StatusOptions};
use tracing::{debug, instrument};

use rustodian_core::CoreError;
use rustodian_core::traits::GitInspector;
use rustodian_types::{CommitInfo, VcsInfo, VcsType};

/// Git inspector using libgit2.
#[derive(Debug, Default)]
pub struct Git2Inspector;

impl GitInspector for Git2Inspector {
    #[instrument(skip(self), fields(path = %project_path.display()))]
    fn inspect(&self, project_path: &Path) -> Result<Option<VcsInfo>, CoreError> {
        debug!("Inspecting git repository");

        let Ok(repo) = Repository::open(project_path) else {
            return Ok(None);
        };

        let branch = match repo.head() {
            Ok(head) => {
                if head.is_branch() {
                    head.shorthand().ok().map(std::string::ToString::to_string)
                } else {
                    None
                }
            }
            Err(_) => None,
        };

        let remote_url = repo
            .find_remote("origin")
            .ok()
            .and_then(|r| r.url().ok().map(std::string::ToString::to_string));

        let is_dirty = self
            .get_dirty_files(project_path)
            .is_ok_and(|files| !files.is_empty());

        let last_commit = match repo.head().and_then(|head| head.peel_to_commit()) {
            Ok(commit) => {
                let author = commit.author();
                let time = chrono::DateTime::from_timestamp(commit.time().seconds(), 0)
                    .unwrap_or_default();

                Some(CommitInfo {
                    sha: commit.id().to_string(),
                    message: commit
                        .summary()
                        .unwrap_or(Some(""))
                        .unwrap_or("")
                        .to_string(),
                    author: author.name().unwrap_or("").to_string(),
                    timestamp: time,
                })
            }
            Err(_) => None,
        };

        Ok(Some(VcsInfo {
            vcs_type: VcsType::Git,
            branch,
            remote_url,
            is_dirty,
            last_commit,
        }))
    }

    fn get_dirty_files(&self, project_path: &Path) -> Result<Vec<std::path::PathBuf>, CoreError> {
        let repo = Repository::open(project_path).map_err(|e| CoreError::Git(e.to_string()))?;

        let mut status_opts = StatusOptions::new();
        status_opts.include_untracked(true);
        let statuses = repo
            .statuses(Some(&mut status_opts))
            .map_err(|e| CoreError::Git(e.to_string()))?;

        let mut dirty_files = Vec::new();
        for entry in statuses.iter() {
            if let Ok(path) = entry.path() {
                dirty_files.push(std::path::PathBuf::from(path));
            }
        }
        Ok(dirty_files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_inspect_not_a_repo() {
        let dir = TempDir::new().unwrap();
        let inspector = Git2Inspector;
        let result = inspector.inspect(dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_inspect_repo() {
        let dir = TempDir::new().unwrap();

        let _repo = Repository::init(dir.path()).unwrap();

        let inspector = Git2Inspector;
        let result = inspector.inspect(dir.path()).unwrap();

        assert!(result.is_some());
        let info = result.unwrap();
        assert_eq!(info.vcs_type, VcsType::Git);
        assert!(!info.is_dirty);
        assert!(info.branch.is_none());
    }

    #[test]
    fn test_get_dirty_files_clean_repo() {
        let dir = TempDir::new().unwrap();
        let _repo = Repository::init(dir.path()).unwrap();

        let inspector = Git2Inspector;
        let dirty = inspector.get_dirty_files(dir.path()).unwrap();
        assert!(dirty.is_empty());
    }

    #[test]
    fn test_get_dirty_files_with_untracked() {
        let dir = TempDir::new().unwrap();
        let _repo = Repository::init(dir.path()).unwrap();

        std::fs::write(dir.path().join("new_file.txt"), "hello").unwrap();

        let inspector = Git2Inspector;
        let dirty = inspector.get_dirty_files(dir.path()).unwrap();
        assert_eq!(dirty.len(), 1);
        assert_eq!(dirty[0], std::path::PathBuf::from("new_file.txt"));
    }
}

```

### Path: ./crates/rustodian-core/src/error.rs
```
//! Core domain errors.

use std::path::PathBuf;

use rustodian_types::ProjectId;

/// Errors that can occur in the Rustodian domain.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// A project was not found.
    #[error("project not found: {0}")]
    ProjectNotFound(ProjectId),

    /// A path was not found or inaccessible.
    #[error("path not found: {}", .0.display())]
    PathNotFound(PathBuf),

    /// A storage operation failed.
    #[error("storage error: {0}")]
    Storage(String),

    /// A scan operation failed.
    #[error("scan error: {0}")]
    Scan(String),

    /// A git operation failed.
    #[error("git error: {0}")]
    Git(String),

    /// Rate limit exceeded on a remote API.
    #[error("API rate limit exceeded")]
    RateLimitExceeded,

    /// An unexpected internal error.
    #[error("internal error: {0}")]
    Internal(String),
}

```

### Path: ./crates/rustodian-core/src/custodian.rs
```
//! The Custodian — Rustodian's core orchestrator.
//!
//! Coordinates scanning, storage, and git inspection through trait objects.
//! Uses `Box<dyn Trait>` for simplicity — dynamic dispatch overhead is
//! irrelevant when every call hits the filesystem or database.

use std::collections::HashMap;
use std::path::Path;

use tracing::{info, instrument};

use rustodian_types::{Project, ProjectId, ProjectLog, ScanConfig, ScanId, ScanRecord};

use crate::error::CoreError;
use crate::runner::CommandSpec;
use crate::traits::{CommandRunner, GitInspector, ProjectScanner, ProjectStore};

/// Report from a scan operation.
#[derive(Debug)]
pub struct ScanReport {
    pub scan_id: ScanId,
    pub projects_found: usize,
    pub projects_new: usize,
    pub projects_updated: usize,
    pub projects_purged: usize,
}

/// Overall status summary.
#[derive(Debug)]
pub struct StatusReport {
    pub total_projects: usize,
    pub last_scan: Option<ScanRecord>,
    pub languages: Vec<(String, usize)>,
}

/// The core orchestrator for Rustodian.
///
/// Wires together storage, scanning, and git inspection.
/// This is the primary API surface for any frontend (CLI, GUI, etc.).
pub struct Custodian {
    store: Box<dyn ProjectStore>,

    scanner: Box<dyn ProjectScanner>,

    git: Box<dyn GitInspector>,
    runner: Box<dyn CommandRunner>,
}

impl Custodian {
    /// Create a new Custodian with the given infrastructure implementations.
    pub fn new(
        store: Box<dyn ProjectStore>,
        scanner: Box<dyn ProjectScanner>,
        git: Box<dyn GitInspector>,
        runner: Box<dyn CommandRunner>,
    ) -> Self {
        Self {
            store,
            scanner,
            git,
            runner,
        }
    }

    /// Access the underlying project store.
    pub fn store(&self) -> &dyn ProjectStore {
        self.store.as_ref()
    }

    /// Scan a directory tree for projects and store the results.
    #[instrument(skip(self), fields(root = %root.display()))]
    pub fn scan(&self, root: &Path, config: &ScanConfig) -> Result<ScanReport, CoreError> {
        info!("Starting scan");
        let start_time = chrono::Utc::now();

        let discovered = self.scanner.scan(root, config)?;

        let mut projects_new = 0;
        let mut projects_updated = 0;

        for d in &discovered {
            let vcs = self.git.inspect(&d.path)?;
            let now = chrono::Utc::now();

            let project = if let Some(mut existing) = self.store.find_by_path(&d.path)? {
                existing.name.clone_from(&d.name);
                existing.languages.clone_from(&d.languages);
                existing.metadata.commands.clone_from(&d.commands);
                existing.vcs = vcs;
                existing.last_scanned_at = Some(now);
                projects_updated += 1;
                existing
            } else {
                projects_new += 1;

                let mut metadata = rustodian_types::ProjectMetadata::default();
                metadata.commands.clone_from(&d.commands);

                Project {
                    id: ProjectId::new(),
                    name: d.name.clone(),
                    path: d.path.clone(),
                    languages: d.languages.clone(),
                    vcs,
                    discovered_at: now,
                    last_scanned_at: Some(now),
                    metadata,
                }
            };

            self.store.save_project(&project)?;
        }

        let scan_record = ScanRecord {
            id: ScanId::new(),
            root_path: root.to_path_buf(),
            started_at: start_time,
            completed_at: Some(chrono::Utc::now()),
            projects_found: discovered.len(),
            status: rustodian_types::ScanStatus::Completed,
        };

        let scan_id = self.store.save_scan(&scan_record)?;

        // ── Self-Healing Garbage Collection Pass ──────────────────────
        // Purge tracked projects whose paths no longer exist on disk.
        let mut projects_purged = 0usize;
        let all_tracked = self.store.list_projects()?;
        for tracked in &all_tracked {
            if !tracked.path.exists() {
                self.store.delete_project(&tracked.id)?;
                info!(
                    project = %tracked.name,
                    path = %tracked.path.display(),
                    "Garbage-collected dead project path"
                );
                projects_purged += 1;
            }
        }

        Ok(ScanReport {
            scan_id,
            projects_found: discovered.len(),
            projects_new,
            projects_updated,
            projects_purged,
        })
    }

    /// Finds a project and executes the given command name if discovered.
    pub fn run_command(&self, project_query: &str, command_name: &str) -> Result<(), CoreError> {
        let project = self
            .find_project(project_query)?
            .ok_or_else(|| CoreError::Storage(format!("Project not found: {project_query}")))?;

        let cmd = project
            .metadata
            .commands
            .iter()
            .find(|c| c.name == command_name)
            .ok_or_else(|| {
                CoreError::Storage(format!(
                    "Command '{}' not found in project '{}'",
                    command_name, project.name
                ))
            })?;

        self.run_and_log_command(
            &project,
            command_name,
            &cmd.command,
            cmd.use_shell,
            HashMap::new(),
        )?;
        Ok(())
    }

    /// Runs a command for a project, streams output in real-time, and logs it to the database.
    pub fn run_and_log_command(
        &self,
        project: &Project,
        command_name: &str,
        program: &str,
        use_shell: bool,
        env: HashMap<String, String>,
    ) -> Result<Option<i32>, CoreError> {
        let spec = CommandSpec {
            program: program.to_string(),
            args: vec![],
            working_dir: project.path.clone(),
            env,
            use_shell,
            capture_output: true,
        };

        let mut child = self.runner.spawn(spec)?;

        let log_buffer = crate::log_buffer::LogBuffer::new();

        let stdout_log = log_buffer.clone();
        let mut stdout_handle = None;
        if let Some(so) = child.stdout() {
            stdout_handle = Some(std::thread::spawn(move || {
                use std::io::{BufRead, BufReader};
                let reader = BufReader::new(so);
                for line in reader.lines().map_while(Result::ok) {
                    println!("{line}");
                    stdout_log.push_line(line);
                }
            }));
        }

        let stderr_log = log_buffer.clone();
        let mut stderr_handle = None;
        if let Some(se) = child.stderr() {
            stderr_handle = Some(std::thread::spawn(move || {
                use std::io::{BufRead, BufReader};
                let reader = BufReader::new(se);
                for line in reader.lines().map_while(Result::ok) {
                    eprintln!("{line}");
                    stderr_log.push_line(line);
                }
            }));
        }

        if let Some(h) = stdout_handle {
            h.join().expect("reader thread panicked");
        }
        if let Some(h) = stderr_handle {
            h.join().expect("reader thread panicked");
        }

        let exit_code = child.wait()?;

        let full_log = log_buffer.snapshot();

        let log_record = ProjectLog {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: project.id.to_string(),
            command_name: command_name.to_string(),
            exit_code,
            log_text: full_log,
            run_at: chrono::Utc::now(),
        };

        self.store.save_log(&log_record)?;
        let _ = self.store.prune_logs(&project.id.to_string(), 50);

        Ok(exit_code)
    }

    /// Automatically bootstrap (environment setup/isolation) and verify (run test suite) a project.
    pub fn bootstrap_and_verify(&self, project_id: &ProjectId) -> Result<(), CoreError> {
        let project = self.info(project_id)?;
        let bootstrapper = crate::bootstrapper::ProjectBootstrapper::new(self);
        bootstrapper.bootstrap_and_verify(&project)
    }

    /// List all tracked projects.
    #[instrument(skip(self))]
    pub fn list(&self) -> Result<Vec<Project>, CoreError> {
        info!("Listing projects");
        self.store.list_projects()
    }

    /// Get overall observatory status.
    #[instrument(skip(self))]
    pub fn status(&self) -> Result<StatusReport, CoreError> {
        info!("Getting status");
        let projects = self.store.list_projects()?;
        let last_scan = self.store.get_latest_scan()?;

        let mut lang_counts = HashMap::new();
        for p in &projects {
            if let Some(primary) = p.languages.first() {
                *lang_counts.entry(primary.language.clone()).or_insert(0) += 1;
            }
        }

        let mut languages: Vec<(String, usize)> = lang_counts
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        // Sort by count descending, then name alphabetically
        languages.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        Ok(StatusReport {
            total_projects: projects.len(),
            last_scan,
            languages,
        })
    }

    /// Get detailed info about a specific project.
    #[instrument(skip(self))]
    pub fn info(&self, id: &ProjectId) -> Result<Project, CoreError> {
        info!(%id, "Getting project info");
        self.store
            .get_project(id)?
            .ok_or_else(|| CoreError::ProjectNotFound(id.clone()))
    }

    /// Find a project by name or ID string.
    #[instrument(skip(self))]
    pub fn find_project(&self, query: &str) -> Result<Option<Project>, CoreError> {
        let all = self.store.list_projects()?;
        if let Some(p) = all.iter().find(|p| p.name == query) {
            return Ok(Some(p.clone()));
        }
        if let Some(p) = all.iter().find(|p| p.id.to_string() == query) {
            return Ok(Some(p.clone()));
        }
        Ok(None)
    }

    /// Find a project by its filesystem path.
    #[instrument(skip(self))]
    pub fn find_by_path(&self, path: &Path) -> Result<Option<Project>, CoreError> {
        self.store.find_by_path(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::DefaultCommandRunner;
    use crate::traits::{DiscoveredProject, GitInspector, ProjectScanner, ProjectStore};
    use rustodian_types::{ProjectId, ProjectLog, ScanConfig, ScanId, ScanRecord, VcsInfo};
    use std::path::Path;
    use std::path::PathBuf;

    struct MockStore;
    impl ProjectStore for MockStore {
        fn save_project(&self, _project: &Project) -> Result<ProjectId, CoreError> {
            Ok(ProjectId::new())
        }
        fn get_project(&self, _id: &ProjectId) -> Result<Option<Project>, CoreError> {
            Ok(None)
        }
        fn list_projects(&self) -> Result<Vec<Project>, CoreError> {
            Ok(vec![])
        }
        fn delete_project(&self, _id: &ProjectId) -> Result<bool, CoreError> {
            Ok(true)
        }
        fn find_by_path(&self, _path: &Path) -> Result<Option<Project>, CoreError> {
            Ok(None)
        }
        fn save_scan(&self, _scan: &ScanRecord) -> Result<ScanId, CoreError> {
            Ok(ScanId::new())
        }
        fn get_latest_scan(&self) -> Result<Option<ScanRecord>, CoreError> {
            Ok(None)
        }
        fn save_log(&self, _log: &ProjectLog) -> Result<(), CoreError> {
            Ok(())
        }
        fn list_logs(
            &self,
            _project_id: &str,
            _limit: usize,
        ) -> Result<Vec<ProjectLog>, CoreError> {
            Ok(vec![])
        }
        fn get_log(&self, _id: &str) -> Result<Option<ProjectLog>, CoreError> {
            Ok(None)
        }
        fn get_latest_log(&self, _project_id: &str) -> Result<Option<ProjectLog>, CoreError> {
            Ok(None)
        }
        fn prune_logs(&self, _project_id: &str, _limit: usize) -> Result<usize, CoreError> {
            Ok(0)
        }
    }

    struct MockScanner;
    impl ProjectScanner for MockScanner {
        fn scan(
            &self,
            _root: &Path,
            _config: &ScanConfig,
        ) -> Result<Vec<DiscoveredProject>, CoreError> {
            Ok(vec![])
        }
    }

    struct MockGit;
    impl GitInspector for MockGit {
        fn inspect(&self, _path: &Path) -> Result<Option<VcsInfo>, CoreError> {
            Ok(None)
        }
        fn get_dirty_files(&self, _project_path: &Path) -> Result<Vec<PathBuf>, CoreError> {
            Ok(vec![])
        }
    }

    #[test]
    fn test_large_output_no_deadlock() {
        let store = MockStore;
        let scanner = MockScanner;
        let git = MockGit;
        let runner = DefaultCommandRunner;

        let custodian = Custodian::new(
            Box::new(store),
            Box::new(scanner),
            Box::new(git),
            Box::new(runner),
        );

        let project = Project {
            id: ProjectId::new(),
            name: "test_deadlock".to_string(),
            path: PathBuf::from("."),
            languages: vec![],
            vcs: None,
            discovered_at: chrono::Utc::now(),
            last_scanned_at: None,
            metadata: rustodian_types::ProjectMetadata::default(),
        };

        // Generate > 100KB of stdout to trigger the pipe buffer limit
        // Use a simpler test program string
        let spec_program = if cfg!(unix) {
            "for i in $(seq 1 15000); do echo '1234567890'; done"
        } else {
            "FOR /L %i IN (1,1,15000) DO echo 1234567890"
        };

        let result = custodian.run_and_log_command(
            &project,
            "test_cmd",
            spec_program,
            true, // use_shell = true
            std::collections::HashMap::new(),
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(0));
    }
}

```

### Path: ./crates/rustodian-core/src/lib.rs
```
//! # Rustodian Core
//!
//! Domain logic, trait definitions, and orchestration for Rustodian.
//!
//! This crate defines the contracts that infrastructure crates must implement.
//! It has **zero knowledge** of `SQLite`, filesystems, or git — those are
//! implementation details provided by other crates.
//!
//! ## Architecture
//!
//! - [`traits`] — The contracts: `ProjectStore`, `ProjectScanner`, `GitInspector`
//! - [`custodian`] — The orchestrator that wires everything together
//! - [`error`] — Domain error types

pub mod bootstrapper;
pub mod custodian;
pub mod error;
pub mod janitor;
pub mod log_buffer;
pub mod runner;
pub mod traits;

pub use bootstrapper::ProjectBootstrapper;
pub use custodian::Custodian;
pub use error::CoreError;
pub use janitor::DigitalJanitor;
pub use log_buffer::LogBuffer;
pub use traits::{GitInspector, ProjectScanner, ProjectStore};

```

### Path: ./crates/rustodian-core/src/janitor.rs
```
//! The Digital Janitor — language-aware workspace artifact cleanup.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tracing::{info, instrument, warn};

use rustodian_types::{Language, Project, ProjectLog};

use crate::Custodian;
use crate::error::CoreError;

/// The disposition of one cleanup target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JanitorOutcome {
    Reclaimable,
    Removed,
    Skipped,
    Failed,
}

impl JanitorOutcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Reclaimable => "reclaimable",
            Self::Removed => "removed",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
        }
    }

    #[must_use]
    const fn is_actionable(self) -> bool {
        matches!(self, Self::Reclaimable | Self::Removed)
    }
}

/// The result of inspecting or removing one cleanup target.
#[derive(Debug, Clone)]
pub struct JanitorTargetResult {
    pub target: String,
    pub path: PathBuf,
    pub size_bytes: Option<u64>,
    pub outcome: JanitorOutcome,
    pub reason: Option<String>,
}

/// Result of a janitor inspection or clean operation.
#[derive(Debug, Clone)]
pub struct JanitorReport {
    /// Results for every discovered cleanup target and any validation failure.
    pub targets: Vec<JanitorTargetResult>,
    /// Total bytes reclaimable (or actually removed when `dry_run` is false).
    pub bytes_reclaimed: u64,
    /// Whether this was an inspection only.
    pub dry_run: bool,
}

impl JanitorReport {
    #[must_use]
    pub fn has_failures(&self) -> bool {
        self.targets
            .iter()
            .any(|target| target.outcome == JanitorOutcome::Failed)
    }
}

#[derive(Debug)]
struct Candidate {
    target: &'static str,
    path: PathBuf,
}

/// The autonomous Digital Janitor orchestrator.
pub struct DigitalJanitor<'a> {
    custodian: &'a Custodian,
}

impl<'a> DigitalJanitor<'a> {
    pub fn new(custodian: &'a Custodian) -> Self {
        Self { custodian }
    }

    /// Inspect a project for language-supported artifacts and optionally purge them.
    ///
    /// A purge always records one audit log, including failed targets. Dry runs do
    /// not mutate either the filesystem or the project database.
    #[instrument(skip(self), fields(project = %project.name, dry_run))]
    pub fn clean(&self, project: &Project, dry_run: bool) -> Result<JanitorReport, CoreError> {
        let mut report = JanitorReport {
            targets: Vec::new(),
            bytes_reclaimed: 0,
            dry_run,
        };

        match validated_project_root(&project.path) {
            Ok(root) => {
                let mut candidates = Vec::new();
                collect_direct_candidates(project, &root, &mut candidates, &mut report.targets);
                if supports_language(project, |language| matches!(language, Language::Python)) {
                    collect_python_caches(&root, &mut candidates, &mut report.targets);
                }

                for candidate in candidates {
                    let result = inspect_candidate(&root, candidate, dry_run);
                    if result.outcome.is_actionable() {
                        report.bytes_reclaimed += result.size_bytes.unwrap_or(0);
                    }
                    report.targets.push(result);
                }
            }
            Err(reason) => report.targets.push(JanitorTargetResult {
                target: "project root".to_string(),
                path: project.path.clone(),
                size_bytes: None,
                outcome: JanitorOutcome::Failed,
                reason: Some(reason),
            }),
        }

        if !dry_run {
            self.save_purge_log(project, &report)?;
            if let Err(error) = self
                .custodian
                .store()
                .prune_logs(&project.id.to_string(), 50)
            {
                warn!(error = %error, "Failed to prune old Janitor logs");
            }
        }

        Ok(report)
    }

    fn save_purge_log(&self, project: &Project, report: &JanitorReport) -> Result<(), CoreError> {
        let failures: Vec<&JanitorTargetResult> = report
            .targets
            .iter()
            .filter(|target| target.outcome == JanitorOutcome::Failed)
            .collect();
        let targets = report
            .targets
            .iter()
            .map(|target| {
                format!(
                    "target={} path={} outcome={} size_bytes={} reason={}",
                    target.target,
                    target.path.display(),
                    target.outcome.as_str(),
                    target
                        .size_bytes
                        .map_or_else(|| "unavailable".to_string(), |size| size.to_string()),
                    target.reason.as_deref().unwrap_or("none"),
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        let failure_paths = failures
            .iter()
            .map(|target| target.path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");

        let log_record = ProjectLog {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: project.id.to_string(),
            command_name: "janitor:clean".to_string(),
            exit_code: Some(i32::from(!failures.is_empty())),
            log_text: format!(
                "Digital Janitor purge: targets=[{targets}]; bytes_reclaimed={}; failures=[{failure_paths}]; success={}",
                report.bytes_reclaimed,
                failures.is_empty(),
            ),
            run_at: chrono::Utc::now(),
        };
        self.custodian.store().save_log(&log_record)
    }
}

fn supports_language(project: &Project, predicate: impl Fn(&Language) -> bool) -> bool {
    project
        .languages
        .iter()
        .any(|detection| predicate(&detection.language))
}

fn collect_direct_candidates(
    project: &Project,
    root: &Path,
    candidates: &mut Vec<Candidate>,
    results: &mut Vec<JanitorTargetResult>,
) {
    let mut targets = Vec::new();
    if supports_language(project, |language| matches!(language, Language::Rust)) {
        targets.push("target");
    }
    if supports_language(project, |language| matches!(language, Language::Node)) {
        targets.extend(["node_modules", ".next"]);
    }
    if supports_language(project, |language| matches!(language, Language::Python)) {
        targets.push(".venv");
    }
    if supports_language(project, |language| matches!(language, Language::Go)) {
        targets.push(".gopath");
    }

    for target in targets {
        let path = root.join(target);
        match fs::symlink_metadata(&path) {
            Ok(_) => candidates.push(Candidate { target, path }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => results.push(failed_result(target, path, None, &error)),
        }
    }
}

fn collect_python_caches(
    root: &Path,
    candidates: &mut Vec<Candidate>,
    results: &mut Vec<JanitorTargetResult>,
) {
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                results.push(failed_result(
                    "__pycache__ discovery",
                    directory,
                    None,
                    &error,
                ));
                continue;
            }
        };

        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    results.push(failed_result(
                        "__pycache__ discovery",
                        directory.clone(),
                        None,
                        &error,
                    ));
                    continue;
                }
            };
            let path = entry.path();
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    results.push(failed_result("__pycache__ discovery", path, None, &error));
                    continue;
                }
            };
            if entry.file_name() == "__pycache__" {
                candidates.push(Candidate {
                    target: "__pycache__",
                    path,
                });
            } else if !metadata.file_type().is_symlink()
                && metadata.is_dir()
                && !is_cleanup_directory(&entry.file_name())
            {
                stack.push(path);
            }
        }
    }
}

fn is_cleanup_directory(name: &std::ffi::OsStr) -> bool {
    matches!(
        name.to_str(),
        Some("target" | "node_modules" | ".next" | ".venv" | ".gopath")
    )
}

fn inspect_candidate(root: &Path, candidate: Candidate, dry_run: bool) -> JanitorTargetResult {
    if !candidate.path.starts_with(root) {
        return failed_result(
            candidate.target,
            candidate.path,
            None,
            &io::Error::other("candidate is not lexically contained in the project root"),
        );
    }

    let metadata = match fs::symlink_metadata(&candidate.path) {
        Ok(metadata) => metadata,
        Err(error) => return failed_result(candidate.target, candidate.path, None, &error),
    };
    if metadata.file_type().is_symlink() {
        return JanitorTargetResult {
            target: candidate.target.to_string(),
            path: candidate.path,
            size_bytes: None,
            outcome: JanitorOutcome::Skipped,
            reason: Some("refusing symbolic link cleanup target".to_string()),
        };
    }
    if !metadata.is_dir() {
        return JanitorTargetResult {
            target: candidate.target.to_string(),
            path: candidate.path,
            size_bytes: None,
            outcome: JanitorOutcome::Skipped,
            reason: Some("cleanup target is not a directory".to_string()),
        };
    }

    let canonical = match fs::canonicalize(&candidate.path) {
        Ok(path) => path,
        Err(error) => return failed_result(candidate.target, candidate.path, None, &error),
    };
    if !canonical.starts_with(root) {
        return failed_result(
            candidate.target,
            candidate.path,
            None,
            &io::Error::other("candidate is not canonically contained in the project root"),
        );
    }

    let size = match dir_size(&candidate.path) {
        Ok(size) => size,
        Err(error) => return failed_result(candidate.target, candidate.path, None, &error),
    };
    info!(
        target = candidate.target,
        size_bytes = size,
        "Found cleanup target"
    );

    if dry_run {
        return JanitorTargetResult {
            target: candidate.target.to_string(),
            path: candidate.path,
            size_bytes: Some(size),
            outcome: JanitorOutcome::Reclaimable,
            reason: None,
        };
    }

    remove_candidate(root, candidate, size)
}

/// Re-check immediately before deletion so a target swapped for a symlink is
/// never removed by this operation.
fn remove_candidate(root: &Path, candidate: Candidate, size: u64) -> JanitorTargetResult {
    let deletion_metadata = match fs::symlink_metadata(&candidate.path) {
        Ok(metadata) => metadata,
        Err(error) => return failed_result(candidate.target, candidate.path, Some(size), &error),
    };
    if deletion_metadata.file_type().is_symlink() {
        return JanitorTargetResult {
            target: candidate.target.to_string(),
            path: candidate.path,
            size_bytes: Some(size),
            outcome: JanitorOutcome::Skipped,
            reason: Some("refusing symbolic link cleanup target".to_string()),
        };
    }
    if !deletion_metadata.is_dir() {
        return JanitorTargetResult {
            target: candidate.target.to_string(),
            path: candidate.path,
            size_bytes: Some(size),
            outcome: JanitorOutcome::Skipped,
            reason: Some("cleanup target is no longer a directory".to_string()),
        };
    }
    match fs::canonicalize(&candidate.path) {
        Ok(path) if path.starts_with(root) => {}
        Ok(_) => {
            return failed_result(
                candidate.target,
                candidate.path,
                Some(size),
                &io::Error::other("candidate is not canonically contained in the project root"),
            );
        }
        Err(error) => return failed_result(candidate.target, candidate.path, Some(size), &error),
    }
    match fs::remove_dir_all(&candidate.path) {
        Ok(()) => JanitorTargetResult {
            target: candidate.target.to_string(),
            path: candidate.path,
            size_bytes: Some(size),
            outcome: JanitorOutcome::Removed,
            reason: None,
        },
        Err(error) => failed_result(candidate.target, candidate.path, Some(size), &error),
    }
}

fn failed_result(
    target: impl Into<String>,
    path: PathBuf,
    size_bytes: Option<u64>,
    error: &io::Error,
) -> JanitorTargetResult {
    JanitorTargetResult {
        target: target.into(),
        path,
        size_bytes,
        outcome: JanitorOutcome::Failed,
        reason: Some(error.to_string()),
    }
}

fn validated_project_root(path: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_dir() && !metadata.file_type().is_symlink() {
        return Err("project root is not a directory".to_string());
    }
    let root = fs::canonicalize(path).map_err(|error| error.to_string())?;
    if !fs::metadata(&root)
        .map_err(|error| error.to_string())?
        .is_dir()
    {
        return Err("resolved project root is not a directory".to_string());
    }
    Ok(root)
}

/// Recursively calculate a directory's size without following symbolic links.
fn dir_size(path: &Path) -> io::Result<u64> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(io::Error::other("refusing to size symbolic link"));
    }
    if !metadata.is_dir() {
        return Ok(metadata.len());
    }

    let mut total = 0;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        let metadata = fs::symlink_metadata(&entry_path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            total += dir_size(&entry_path)?;
        } else {
            total += metadata.len();
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dir_size_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        assert_eq!(dir_size(dir.path()).unwrap(), 0);
    }

    #[test]
    fn test_dir_size_with_file() {
        let dir = tempfile::TempDir::new().unwrap();
        fs::write(dir.path().join("test.txt"), "hello world").unwrap();
        assert_eq!(dir_size(dir.path()).unwrap(), 11);
    }

    #[cfg(unix)]
    #[test]
    fn test_dir_size_skips_nested_symlink() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        fs::write(outside.path().join("large.txt"), vec![0_u8; 128]).unwrap();
        fs::write(dir.path().join("inside.txt"), "safe").unwrap();
        symlink(outside.path(), dir.path().join("outside-link")).unwrap();

        assert_eq!(dir_size(dir.path()).unwrap(), 4);
    }
}

```

### Path: ./crates/rustodian-core/src/bootstrapper.rs
```
use crate::Custodian;
use crate::error::CoreError;
use rustodian_types::{Language, Project};
use std::collections::HashMap;
use std::path::Path;

/// Handles automated project environment bootstrapping, isolation, and verification.
pub struct ProjectBootstrapper<'a> {
    custodian: &'a Custodian,
}

impl<'a> ProjectBootstrapper<'a> {
    pub fn new(custodian: &'a Custodian) -> Self {
        Self { custodian }
    }

    /// Perform environment isolation, bootstrap setup, and verification for the project.
    pub fn bootstrap_and_verify(&self, project: &Project) -> Result<(), CoreError> {
        let mut env = HashMap::new();

        for lang_det in &project.languages {
            match lang_det.language {
                Language::Rust => {
                    self.bootstrap_rust(project, &env)?;
                }
                Language::Node => {
                    self.bootstrap_node(project, &env)?;
                }
                Language::Go => {
                    // Isolation: Set GOPATH to a project-local .gopath folder to keep the host system clean
                    let local_gopath = project.path.join(".gopath");
                    env.insert(
                        "GOPATH".to_string(),
                        local_gopath.to_string_lossy().to_string(),
                    );
                    self.bootstrap_go(project, &env)?;
                }
                Language::Python => {
                    self.bootstrap_python(project, &env)?;
                }
                Language::Unknown(_) | Language::Ruby | Language::Zig => {}
            }
        }

        Ok(())
    }

    fn bootstrap_rust(
        &self,
        project: &Project,
        env: &HashMap<String, String>,
    ) -> Result<(), CoreError> {
        // Setup/Bootstrap
        tracing::info!("Bootstrapping Rust project: {}", project.name);
        self.custodian.run_and_log_command(
            project,
            "bootstrap:rust",
            "cargo build",
            true,
            env.clone(),
        )?;

        // Verification
        tracing::info!("Verifying Rust project: {}", project.name);
        self.custodian.run_and_log_command(
            project,
            "verify:rust",
            "cargo test",
            true,
            env.clone(),
        )?;

        Ok(())
    }

    fn bootstrap_node(
        &self,
        project: &Project,
        env: &HashMap<String, String>,
    ) -> Result<(), CoreError> {
        let path = &project.path;
        let (install_cmd, test_cmd) = if path.join("yarn.lock").exists() {
            ("yarn install", "yarn test")
        } else if path.join("pnpm-lock.yaml").exists() {
            ("pnpm install", "pnpm test")
        } else if path.join("bun.lockb").exists() {
            ("bun install", "bun test")
        } else {
            ("npm install", "npm test")
        };

        // Setup/Bootstrap
        tracing::info!("Bootstrapping Node project: {}", project.name);
        self.custodian.run_and_log_command(
            project,
            "bootstrap:node",
            install_cmd,
            true,
            env.clone(),
        )?;

        // Verification
        tracing::info!("Verifying Node project: {}", project.name);
        self.custodian
            .run_and_log_command(project, "verify:node", test_cmd, true, env.clone())?;

        Ok(())
    }

    fn bootstrap_go(
        &self,
        project: &Project,
        env: &HashMap<String, String>,
    ) -> Result<(), CoreError> {
        // Setup/Bootstrap
        tracing::info!("Bootstrapping Go project: {}", project.name);
        self.custodian.run_and_log_command(
            project,
            "bootstrap:go",
            "go mod download",
            true,
            env.clone(),
        )?;

        // Verification
        tracing::info!("Verifying Go project: {}", project.name);
        self.custodian.run_and_log_command(
            project,
            "verify:go",
            "go test ./...",
            true,
            env.clone(),
        )?;

        Ok(())
    }

    fn bootstrap_python(
        &self,
        project: &Project,
        env: &HashMap<String, String>,
    ) -> Result<(), CoreError> {
        tracing::info!("Bootstrapping Python project: {}", project.name);

        // Isolation: Set up a virtualenv (.venv) inside the project
        let mut venv_success = false;
        for cmd in &["python3 -m venv .venv", "python -m venv .venv"] {
            if self
                .custodian
                .run_and_log_command(project, "bootstrap:python_venv", cmd, true, env.clone())
                .is_ok()
            {
                venv_success = true;
                break;
            }
        }

        if !venv_success {
            return Err(CoreError::Internal(
                "failed to create Python virtual environment (.venv)".to_string(),
            ));
        }

        // Setup/Bootstrap dependencies
        let path = &project.path;
        let pip_env = env.clone();
        // Point to the virtualenv python/pip bin
        let pip_path = if cfg!(windows) {
            ".venv\\Scripts\\pip"
        } else {
            ".venv/bin/pip"
        };

        if path.join("requirements.txt").exists() {
            let install_cmd = format!("{pip_path} install -r requirements.txt");
            self.custodian.run_and_log_command(
                project,
                "bootstrap:python_deps",
                &install_cmd,
                true,
                pip_env.clone(),
            )?;
        }
        if path.join("pyproject.toml").exists() || path.join("setup.py").exists() {
            let install_cmd = format!("{pip_path} install .");
            self.custodian.run_and_log_command(
                project,
                "bootstrap:python_deps",
                &install_cmd,
                true,
                pip_env.clone(),
            )?;
        }

        // Verification
        let pytest_path = if cfg!(windows) {
            ".venv\\Scripts\\pytest"
        } else {
            ".venv/bin/pytest"
        };
        let python_path = if cfg!(windows) {
            ".venv\\Scripts\\python"
        } else {
            ".venv/bin/python"
        };

        let test_cmd = if path.join(pytest_path).exists() || Path::new(pytest_path).exists() {
            format!("{pytest_path} -v")
        } else {
            format!("{python_path} -m unittest discover")
        };

        tracing::info!("Verifying Python project: {}", project.name);
        self.custodian
            .run_and_log_command(project, "verify:python", &test_cmd, true, pip_env)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Custodian;
    use crate::runner::CommandSpec;
    use crate::traits::{
        CommandRunner, GitInspector, ProjectScanner, ProjectStore, RunningProcess,
    };
    use rustodian_types::{DetectionConfidence, Language, LanguageDetection, Project, ProjectId};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::Mutex;

    struct MockRunningProcess {
        exit_code: Option<i32>,
    }

    impl RunningProcess for MockRunningProcess {
        fn id(&self) -> u32 {
            1234
        }
        fn wait(&mut self) -> Result<Option<i32>, CoreError> {
            Ok(self.exit_code)
        }
        fn try_wait(&mut self) -> Result<Option<Option<i32>>, CoreError> {
            Ok(Some(self.exit_code))
        }
        fn kill(&mut self) -> Result<(), CoreError> {
            Ok(())
        }
        fn stdout(&mut self) -> Option<Box<dyn std::io::Read + Send + Sync>> {
            Some(Box::new(std::io::Cursor::new("mock stdout\n")))
        }
        fn stderr(&mut self) -> Option<Box<dyn std::io::Read + Send + Sync>> {
            Some(Box::new(std::io::Cursor::new("mock stderr\n")))
        }
    }

    struct MockCommandRunner {
        commands_run: Arc<Mutex<Vec<String>>>,
    }

    impl CommandRunner for MockCommandRunner {
        fn spawn(&self, spec: CommandSpec) -> Result<Box<dyn RunningProcess>, CoreError> {
            let mut list = self.commands_run.lock().unwrap();
            list.push(spec.program.clone());
            Ok(Box::new(MockRunningProcess { exit_code: Some(0) }))
        }
    }

    struct MockStore;
    impl ProjectStore for MockStore {
        fn save_project(&self, _project: &Project) -> Result<ProjectId, CoreError> {
            Ok(ProjectId::new())
        }
        fn get_project(&self, _id: &ProjectId) -> Result<Option<Project>, CoreError> {
            Ok(None)
        }
        fn list_projects(&self) -> Result<Vec<Project>, CoreError> {
            Ok(vec![])
        }
        fn delete_project(&self, _id: &ProjectId) -> Result<bool, CoreError> {
            Ok(true)
        }
        fn find_by_path(&self, _path: &Path) -> Result<Option<Project>, CoreError> {
            Ok(None)
        }
        fn save_scan(
            &self,
            _scan: &rustodian_types::ScanRecord,
        ) -> Result<rustodian_types::ScanId, CoreError> {
            Ok(rustodian_types::ScanId::new())
        }
        fn get_latest_scan(&self) -> Result<Option<rustodian_types::ScanRecord>, CoreError> {
            Ok(None)
        }
        fn save_log(&self, _log: &rustodian_types::ProjectLog) -> Result<(), CoreError> {
            Ok(())
        }
        fn list_logs(
            &self,
            _project_id: &str,
            _limit: usize,
        ) -> Result<Vec<rustodian_types::ProjectLog>, CoreError> {
            Ok(vec![])
        }
        fn get_log(&self, _id: &str) -> Result<Option<rustodian_types::ProjectLog>, CoreError> {
            Ok(None)
        }
        fn get_latest_log(
            &self,
            _project_id: &str,
        ) -> Result<Option<rustodian_types::ProjectLog>, CoreError> {
            Ok(None)
        }
        fn prune_logs(&self, _project_id: &str, _limit: usize) -> Result<usize, CoreError> {
            Ok(0)
        }
    }

    struct MockScanner;
    impl ProjectScanner for MockScanner {
        fn scan(
            &self,
            _root: &Path,
            _config: &rustodian_types::ScanConfig,
        ) -> Result<Vec<crate::traits::DiscoveredProject>, CoreError> {
            Ok(vec![])
        }
    }

    struct MockGit;
    impl GitInspector for MockGit {
        fn inspect(&self, _path: &Path) -> Result<Option<rustodian_types::VcsInfo>, CoreError> {
            Ok(None)
        }
        fn get_dirty_files(
            &self,
            _project_path: &Path,
        ) -> Result<Vec<std::path::PathBuf>, CoreError> {
            Ok(vec![])
        }
    }

    #[test]
    fn test_bootstrap_rust_project() {
        let commands_run = Arc::new(Mutex::new(Vec::new()));
        let runner = MockCommandRunner {
            commands_run: commands_run.clone(),
        };
        let store = MockStore;
        let scanner = MockScanner;
        let git = MockGit;
        let custodian = Custodian::new(
            Box::new(store),
            Box::new(scanner),
            Box::new(git),
            Box::new(runner),
        );

        let project = Project {
            id: ProjectId::new(),
            name: "test_rust".to_string(),
            path: PathBuf::from("/tmp/test_rust"),
            languages: vec![LanguageDetection {
                language: Language::Rust,
                confidence: DetectionConfidence::High,
                markers: vec![],
            }],
            vcs: None,
            discovered_at: chrono::Utc::now(),
            last_scanned_at: None,
            metadata: rustodian_types::ProjectMetadata::default(),
        };

        let bootstrapper = ProjectBootstrapper::new(&custodian);
        bootstrapper.bootstrap_and_verify(&project).unwrap();

        let run_list = commands_run.lock().unwrap();
        assert_eq!(run_list.len(), 2);
        assert_eq!(run_list[0], "cargo build");
        assert_eq!(run_list[1], "cargo test");
    }

    #[test]
    fn test_bootstrap_go_project() {
        let commands_run = Arc::new(Mutex::new(Vec::new()));
        let runner = MockCommandRunner {
            commands_run: commands_run.clone(),
        };
        let store = MockStore;
        let scanner = MockScanner;
        let git = MockGit;
        let custodian = Custodian::new(
            Box::new(store),
            Box::new(scanner),
            Box::new(git),
            Box::new(runner),
        );

        let project = Project {
            id: ProjectId::new(),
            name: "test_go".to_string(),
            path: PathBuf::from("/tmp/test_go"),
            languages: vec![LanguageDetection {
                language: Language::Go,
                confidence: DetectionConfidence::High,
                markers: vec![],
            }],
            vcs: None,
            discovered_at: chrono::Utc::now(),
            last_scanned_at: None,
            metadata: rustodian_types::ProjectMetadata::default(),
        };

        let bootstrapper = ProjectBootstrapper::new(&custodian);
        bootstrapper.bootstrap_and_verify(&project).unwrap();

        let run_list = commands_run.lock().unwrap();
        assert_eq!(run_list.len(), 2);
        assert_eq!(run_list[0], "go mod download");
        assert_eq!(run_list[1], "go test ./...");
    }
}

```

### Path: ./crates/rustodian-core/src/runner.rs
```
use std::collections::HashMap;
use std::path::PathBuf;

/// Structured command specification for the command runner.
#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub working_dir: PathBuf,
    pub env: HashMap<String, String>,
    pub use_shell: bool,
    pub capture_output: bool,
}

impl Default for CommandSpec {
    fn default() -> Self {
        Self {
            program: String::new(),
            args: vec![],
            working_dir: PathBuf::from("."),
            env: HashMap::new(),
            use_shell: false,
            capture_output: false,
        }
    }
}

#[cfg(unix)]
use nix::sys::signal::{Signal, kill};
#[cfg(unix)]
use nix::unistd::Pid;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

use crate::error::CoreError;
use crate::traits::{CommandRunner, RunningProcess};

pub struct DefaultCommandRunner;

impl CommandRunner for DefaultCommandRunner {
    fn spawn(&self, spec: CommandSpec) -> Result<Box<dyn RunningProcess>, CoreError> {
        let mut cmd = if spec.use_shell {
            let shell_cmd = if spec.args.is_empty() {
                spec.program.clone()
            } else {
                format!("{} {}", spec.program, spec.args.join(" "))
            };
            #[cfg(unix)]
            {
                let mut c = Command::new("sh");
                c.arg("-c").arg(&shell_cmd);
                c
            }
            #[cfg(not(unix))]
            {
                let mut c = Command::new("cmd");
                c.arg("/C").arg(&shell_cmd);
                c
            }
        } else {
            // If the user specifies `use_shell=false`, but `spec.program` is actually a full command string,
            // we should parse it with shlex.
            let mut args_iter =
                shlex::split(&spec.program).unwrap_or_else(|| vec![spec.program.clone()]);

            let program = if args_iter.is_empty() {
                spec.program.clone()
            } else {
                args_iter.remove(0)
            };

            let mut c = Command::new(program);
            c.args(args_iter);
            c.args(&spec.args);
            c
        };

        cmd.current_dir(&spec.working_dir).envs(&spec.env);

        if spec.capture_output {
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        } else {
            cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
        }

        #[cfg(unix)]
        cmd.process_group(0); // Create a new process group

        let child = cmd
            .spawn()
            .map_err(|e| CoreError::Storage(format!("Failed to spawn process: {e}")))?;

        Ok(Box::new(DefaultRunningProcess { child }))
    }
}

pub struct DefaultRunningProcess {
    child: Child,
}

impl RunningProcess for DefaultRunningProcess {
    fn id(&self) -> u32 {
        self.child.id()
    }

    fn wait(&mut self) -> Result<Option<i32>, CoreError> {
        let status = self
            .child
            .wait()
            .map_err(|e| CoreError::Storage(format!("Failed to wait for process: {e}")))?;
        Ok(status.code())
    }

    fn try_wait(&mut self) -> Result<Option<Option<i32>>, CoreError> {
        match self.child.try_wait() {
            Ok(Some(status)) => Ok(Some(status.code())),
            Ok(None) => Ok(None),
            Err(e) => Err(CoreError::Storage(format!(
                "Failed to try_wait for process: {e}"
            ))),
        }
    }

    fn kill(&mut self) -> Result<(), CoreError> {
        #[cfg(unix)]
        {
            let pid = Pid::from_raw(self.child.id().cast_signed());
            // Kill the entire process group
            let _ = kill(Pid::from_raw(-pid.as_raw()), Signal::SIGKILL);
            Ok(())
        }

        #[cfg(not(unix))]
        {
            let pid = self.child.id();
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/T", "/PID", &pid.to_string()])
                .output();
            let _ = self.child.kill();
            Ok(())
        }
    }

    fn stdout(&mut self) -> Option<Box<dyn Read + Send + Sync>> {
        self.child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send + Sync>)
    }

    fn stderr(&mut self) -> Option<Box<dyn Read + Send + Sync>> {
        self.child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send + Sync>)
    }
}

/// Idiomatic Drop guard: terminates orphan background processes automatically
impl Drop for DefaultRunningProcess {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kill_exited_process() -> Result<(), crate::error::CoreError> {
        let runner = DefaultCommandRunner;
        let spec = CommandSpec {
            program: "true".to_string(),
            args: vec![],
            working_dir: std::path::PathBuf::from("."),
            env: std::collections::HashMap::new(),
            use_shell: false,
            capture_output: false,
        };

        let mut child = runner.spawn(spec)?;
        child.wait()?; // Wait for it to exit

        // This should not error even if the process is dead
        child.kill()?;
        Ok(())
    }
}

```

### Path: ./crates/rustodian-core/src/log_buffer.rs
```
//! Thread-safe append-only ring buffer for log capture.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Default maximum number of lines retained in memory.
const DEFAULT_MAX_LINES: usize = 10_000;

/// Inner state of the log buffer.
struct LogBufferInner {
    lines: VecDeque<String>,
    max_lines: usize,
}

/// A thread-safe, append-only ring buffer for capturing log output.
///
/// Lines beyond `max_lines` are evicted from the front (oldest first).
/// The buffer is `Clone + Send + Sync` (via `Arc`).
#[derive(Clone)]
pub struct LogBuffer {
    inner: Arc<Mutex<LogBufferInner>>,
}

impl LogBuffer {
    /// Create a new log buffer with the default capacity.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MAX_LINES)
    }

    /// Create a new log buffer with the specified maximum line count.
    #[must_use]
    pub fn with_capacity(max_lines: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(LogBufferInner {
                lines: VecDeque::with_capacity(max_lines.min(1024)),
                max_lines,
            })),
        }
    }

    /// Append a single line to the buffer.
    ///
    /// If the buffer is at capacity, the oldest line is evicted.
    pub fn push_line(&self, line: String) {
        if let Ok(mut inner) = self.inner.lock() {
            if inner.lines.len() >= inner.max_lines {
                inner.lines.pop_front();
            }
            inner.lines.push_back(line);
        }
    }

    /// Return a snapshot of all buffered lines joined by newlines.
    #[must_use]
    pub fn snapshot(&self) -> String {
        let mut s = String::new();
        self.snapshot_into(&mut s);
        s
    }

    /// Fill a caller-provided String with a snapshot of all buffered lines,
    /// reusing the string's capacity instead of allocating a new one.
    pub fn snapshot_into(&self, buf: &mut String) {
        buf.clear();
        if let Ok(inner) = self.inner.lock() {
            for line in &inner.lines {
                buf.push_str(line);
                buf.push('\n');
            }
        }
    }

    /// Return the number of lines currently in the buffer.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.inner.lock().map_or(0, |inner| inner.lines.len())
    }

    /// Drain all lines from the buffer and return them joined by newlines.
    ///
    /// The buffer is empty after this call.
    pub fn drain_all(&self) -> String {
        self.inner
            .lock()
            .map(|mut inner| {
                let mut s = String::new();
                for line in inner.lines.drain(..) {
                    s.push_str(&line);
                    s.push('\n');
                }
                s
            })
            .unwrap_or_default()
    }
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_log_buffer_exact_capacity() {
        let buf = LogBuffer::with_capacity(3);

        // Push exact capacity
        buf.push_line("1".to_string());
        buf.push_line("2".to_string());
        buf.push_line("3".to_string());

        assert_eq!(buf.line_count(), 3);
        assert_eq!(buf.snapshot(), "1\n2\n3\n");

        // Push one more, causing eviction
        buf.push_line("4".to_string());
        assert_eq!(buf.line_count(), 3);
        assert_eq!(buf.snapshot(), "2\n3\n4\n");
    }
    use super::*;

    #[test]
    fn test_push_and_snapshot() {
        let buf = LogBuffer::new();
        buf.push_line("hello".to_string());
        buf.push_line("world".to_string());
        assert_eq!(buf.snapshot(), "hello\nworld\n");
        assert_eq!(buf.line_count(), 2);
    }

    #[test]
    fn test_eviction() {
        let buf = LogBuffer::with_capacity(3);
        for i in 0..5 {
            buf.push_line(format!("line {i}"));
        }
        assert_eq!(buf.line_count(), 3);
        // Should contain lines 2, 3, 4 (oldest evicted)
        let snap = buf.snapshot();
        assert!(snap.contains("line 2"));
        assert!(snap.contains("line 4"));
        assert!(!snap.contains("line 0"));
    }

    #[test]
    fn test_drain_all() {
        let buf = LogBuffer::new();
        buf.push_line("a".to_string());
        buf.push_line("b".to_string());
        let drained = buf.drain_all();
        assert_eq!(drained, "a\nb\n");
        assert_eq!(buf.line_count(), 0);
        assert_eq!(buf.snapshot(), "");
    }

    #[test]
    fn test_clone_shares_state() {
        let buf1 = LogBuffer::new();
        let buf2 = buf1.clone();
        buf1.push_line("from buf1".to_string());
        assert_eq!(buf2.line_count(), 1);
    }
}

```

### Path: ./crates/rustodian-core/src/traits.rs
```
use std::path::Path;

use rustodian_types::{Project, ProjectId, ProjectLog, ScanConfig, ScanId, ScanRecord, VcsInfo};

use crate::error::CoreError;

/// A discovered but not-yet-stored project from a scan.
#[derive(Debug, Clone)]
pub struct DiscoveredProject {
    pub name: String,
    pub path: std::path::PathBuf,
    pub languages: Vec<rustodian_types::LanguageDetection>,
    pub commands: Vec<rustodian_types::ProjectCommand>,
}

/// Contract for project persistence.
///
/// Implementors provide the actual storage mechanism (e.g., `SQLite`).
pub trait ProjectStore: Send + Sync {
    /// Persist a project, returning its ID.
    fn save_project(&self, project: &Project) -> Result<ProjectId, CoreError>;

    /// Retrieve a project by ID.
    fn get_project(&self, id: &ProjectId) -> Result<Option<Project>, CoreError>;

    /// List all known projects.
    fn list_projects(&self) -> Result<Vec<Project>, CoreError>;

    /// Delete a project by ID. Returns true if it existed.
    fn delete_project(&self, id: &ProjectId) -> Result<bool, CoreError>;

    /// Find a project by its filesystem path.
    fn find_by_path(&self, path: &Path) -> Result<Option<Project>, CoreError>;

    /// Record a scan operation.
    fn save_scan(&self, scan: &ScanRecord) -> Result<ScanId, CoreError>;

    /// Get the most recent scan record.
    fn get_latest_scan(&self) -> Result<Option<ScanRecord>, CoreError>;

    /// Persist a command execution log.
    fn save_log(&self, log: &ProjectLog) -> Result<(), CoreError>;

    /// List execution logs for a project, ordered by most recent first.
    fn list_logs(&self, project_id: &str, limit: usize) -> Result<Vec<ProjectLog>, CoreError>;

    /// Get a specific log entry by ID.
    fn get_log(&self, id: &str) -> Result<Option<ProjectLog>, CoreError>;

    /// Get the most recent log entry for a project.
    fn get_latest_log(&self, project_id: &str) -> Result<Option<ProjectLog>, CoreError>;

    /// Prune old logs for a project, keeping only the `limit` most recent entries. Returns the number of deleted rows.
    fn prune_logs(&self, project_id: &str, limit: usize) -> Result<usize, CoreError>;
}

/// Contract for filesystem project discovery.
///
/// Implementors walk the filesystem to find software projects.
pub trait ProjectScanner: Send + Sync {
    /// Scan a directory tree for software projects.
    fn scan(&self, root: &Path, config: &ScanConfig) -> Result<Vec<DiscoveredProject>, CoreError>;
}

/// Contract for VCS inspection.
///
/// Implementors extract version control information from a project directory.
pub trait GitInspector: Send + Sync {
    /// Inspect a project directory for git information.
    /// Returns `None` if the directory is not a git repository.
    fn inspect(&self, project_path: &Path) -> Result<Option<VcsInfo>, CoreError>;

    /// Query the repository status for untracked, modified, or staged files.
    /// Returns an empty vec if the path is not a git repository.
    fn get_dirty_files(&self, project_path: &Path) -> Result<Vec<std::path::PathBuf>, CoreError>;
}
use rustodian_types::RemoteProject;

#[async_trait::async_trait]
pub trait RemoteDownloader: Send + Sync {
    async fn download_and_extract(
        &self,
        project: &RemoteProject,
        dest_dir: &std::path::Path,
        preserve_patterns: &[String],
    ) -> Result<(), crate::error::CoreError>;
}

pub trait RemoteProjectStore: Send + Sync {
    fn save_remote_project(&self, project: &RemoteProject) -> Result<(), crate::error::CoreError>;
    fn list_remote_projects(&self) -> Result<Vec<RemoteProject>, crate::error::CoreError>;
    fn delete_remote_project(&self, repo_slug: &str) -> Result<bool, crate::error::CoreError>;
}

use crate::runner::CommandSpec;

pub trait RunningProcess: Send + Sync {
    fn id(&self) -> u32;
    fn wait(&mut self) -> Result<Option<i32>, CoreError>;
    fn try_wait(&mut self) -> Result<Option<Option<i32>>, CoreError>;
    fn kill(&mut self) -> Result<(), CoreError>;
    fn stdout(&mut self) -> Option<Box<dyn std::io::Read + Send + Sync>>;
    fn stderr(&mut self) -> Option<Box<dyn std::io::Read + Send + Sync>>;
}

#[async_trait::async_trait]
pub trait PullRequestFetcher: Send + Sync {
    async fn fetch_open_prs(
        &self,
        repo_slug: &str,
    ) -> Result<Vec<rustodian_types::PullRequest>, CoreError>;
}

pub trait CommandRunner: Send + Sync {
    fn spawn(&self, spec: CommandSpec) -> Result<Box<dyn RunningProcess>, CoreError>;
}

```

### Path: ./crates/rustodian-cli/tests/cli_tests.rs
```
use std::fs;
use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn test_scan_and_list() {
    let dir = TempDir::new().unwrap();
    let proj_dir = dir.path().join("my-rust-proj");
    fs::create_dir(&proj_dir).unwrap();
    fs::write(proj_dir.join("Cargo.toml"), "[package]").unwrap();

    let js_dir = dir.path().join("my-js-proj");
    fs::create_dir(&js_dir).unwrap();
    fs::write(
        js_dir.join("package.json"),
        r#"{"scripts": {"build": "webpack"}}"#,
    )
    .unwrap();
    fs::write(
        js_dir.join("justfile"),
        "test:\n  echo test\n\nfmt:\n  prettier --write",
    )
    .unwrap();
    fs::write(
        js_dir.join(".rustodian.toml"),
        r#"[commands]
custom-cmd = "echo hello world"
"#,
    )
    .unwrap();

    // 1. Scan
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("scan")
        .arg("--path")
        .arg(dir.path());
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Projects Found:   2"));

    // 2. List
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("list");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("my-rust-proj"))
        .stdout(predicate::str::contains("my-js-proj"));

    // 3. Info for JS proj
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("info")
        .arg("my-js-proj");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Discovered Commands:"))
        .stdout(predicate::str::contains("test"))
        .stdout(predicate::str::contains("build"))
        .stdout(predicate::str::contains("custom-cmd"));

    // 4. Run custom command
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("run")
        .arg("my-js-proj")
        .arg("custom-cmd");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("hello world"));
}

#[test]
fn test_janitor() {
    let dir = TempDir::new().unwrap();
    let proj_dir = dir.path().join("my-rust-proj");
    fs::create_dir(&proj_dir).unwrap();
    fs::write(proj_dir.join("Cargo.toml"), "[package]").unwrap();

    let target_dir = proj_dir.join("target");
    fs::create_dir(&target_dir).unwrap();
    fs::write(target_dir.join("dummy.txt"), "dummy").unwrap();
    let build_dir = proj_dir.join("build");
    fs::create_dir(&build_dir).unwrap();
    fs::write(build_dir.join("keep.txt"), "keep").unwrap();
    let dist_dir = proj_dir.join("dist");
    fs::create_dir(&dist_dir).unwrap();
    fs::write(dist_dir.join("keep.txt"), "keep").unwrap();
    let node_modules_dir = proj_dir.join("node_modules");
    fs::create_dir(&node_modules_dir).unwrap();
    fs::write(node_modules_dir.join("keep.txt"), "keep").unwrap();

    // 1. Scan
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("scan")
        .arg("--path")
        .arg(dir.path());
    cmd.assert().success();

    // 2. Janitor dry-run
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("janitor")
        .arg("my-rust-proj")
        .arg("--dry-run");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target"))
        .stdout(predicate::str::contains("reclaimable"))
        .stdout(predicate::str::contains("5 B"));

    // verify file still exists
    assert!(target_dir.join("dummy.txt").exists());
    assert!(build_dir.exists());
    assert!(dist_dir.exists());
    assert!(node_modules_dir.exists());

    // 3. Structured JSON output exposes raw size values.
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    let output = cmd
        .env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("janitor")
        .arg("my-rust-proj")
        .arg("--format")
        .arg("json")
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["targets"][0]["target"], "target");
    assert_eq!(json["targets"][0]["outcome"], "reclaimable");
    assert_eq!(json["targets"][0]["size_bytes"], 5);

    // 4. Janitor purge
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("janitor")
        .arg("my-rust-proj")
        .arg("--purge");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target"))
        .stdout(predicate::str::contains("removed"));

    // Eligible cleanup is deleted; ambiguous and language-ineligible directories stay.
    assert!(!target_dir.exists());
    assert!(build_dir.exists());
    assert!(dist_dir.exists());
    assert!(node_modules_dir.exists());
}

#[cfg(unix)]
#[test]
fn test_janitor_refuses_symlink_target() {
    use std::os::unix::fs::symlink;

    let dir = TempDir::new().unwrap();
    let project = dir.path().join("my-rust-proj");
    fs::create_dir(&project).unwrap();
    fs::write(project.join("Cargo.toml"), "[package]").unwrap();
    let outside = TempDir::new().unwrap();
    let outside_file = outside.path().join("must-survive.txt");
    fs::write(&outside_file, "safe").unwrap();
    symlink(outside.path(), project.join("target")).unwrap();

    scan_project(dir.path(), "test.db");

    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("janitor")
        .arg("my-rust-proj")
        .arg("--purge");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skipped"))
        .stdout(predicate::str::contains("symbolic link"));

    assert!(
        project
            .join("target")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(outside_file.exists());
}

#[cfg(unix)]
#[test]
fn test_janitor_reports_partial_purge_failure() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let project = dir.path().join("mixed-project");
    fs::create_dir(&project).unwrap();
    fs::write(project.join("Cargo.toml"), "[package]").unwrap();
    fs::write(project.join("pyproject.toml"), "").unwrap();
    let target = project.join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("removed.txt"), "removed").unwrap();
    let venv = project.join(".venv");
    fs::create_dir(&venv).unwrap();
    fs::write(venv.join("locked.txt"), "not reclaimed").unwrap();

    scan_project(dir.path(), "test.db");
    fs::set_permissions(&venv, fs::Permissions::from_mode(0o555)).unwrap();

    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("janitor")
        .arg("mixed-project")
        .arg("--purge");
    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("target"))
        .stdout(predicate::str::contains(".venv"))
        .stdout(predicate::str::contains("removed"))
        .stdout(predicate::str::contains("failed"));

    assert!(!target.exists());
    assert!(venv.exists());
    fs::set_permissions(&venv, fs::Permissions::from_mode(0o755)).unwrap();
}

fn scan_project(root: &std::path::Path, db_name: &str) {
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", root.join(db_name))
        .arg("scan")
        .arg("--path")
        .arg(root);
    cmd.assert().success();
}

```

### Path: ./crates/rustodian-cli/src/commands/mod.rs
```
//! CLI command implementations.
pub mod config;

pub mod info;
pub mod janitor;
pub mod list;
pub mod logs;
pub mod remote;
pub mod run;
pub mod scan;
pub mod status;

```

### Path: ./crates/rustodian-cli/src/commands/remote.rs
```
use crate::OutputFormat;
use anyhow::{Context, Result};
use rustodian_core::Custodian;
use rustodian_core::traits::{RemoteDownloader, RemoteProjectStore};
use rustodian_remote::GithubDownloader;
use rustodian_storage::SqliteStore;
use rustodian_types::{RemoteProject, ScanConfig};
use tokio::runtime::Runtime;
use tracing::info;

pub fn execute_add(store: &SqliteStore, repo_slug: &str, preserve: &[String]) -> Result<()> {
    let project = RemoteProject {
        repo_slug: repo_slug.to_string(),
        preserve_patterns: preserve.to_vec(),
    };
    store
        .save_remote_project(&project)
        .context("failed to save remote project")?;
    info!("Added remote project {}", repo_slug);
    println!("Added remote project: {repo_slug}");
    Ok(())
}

pub fn execute_list(store: &SqliteStore, format: &OutputFormat) -> Result<()> {
    let projects = store
        .list_remote_projects()
        .context("failed to list remote projects")?;
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&projects)?);
        }
        OutputFormat::Table => {
            if projects.is_empty() {
                println!("No remote projects tracked.");
                return Ok(());
            }
            let mut table = comfy_table::Table::new();
            table.set_header(vec!["Repo Slug", "Preserve Patterns"]);
            for p in projects {
                let patterns = if p.preserve_patterns.is_empty() {
                    "(none)".to_string()
                } else {
                    p.preserve_patterns.join(", ")
                };
                table.add_row(vec![p.repo_slug, patterns]);
            }
            println!("{table}");
        }
    }
    Ok(())
}

pub fn execute_refresh(
    custodian: &Custodian,
    store: &SqliteStore,
    dest_dir: &std::path::Path,
) -> Result<()> {
    let projects = store
        .list_remote_projects()
        .context("failed to list remote projects")?;
    if projects.is_empty() {
        println!("No remote projects to refresh.");
        return Ok(());
    }
    let downloader = GithubDownloader::new();
    let rt = Runtime::new().context("failed to create tokio runtime")?;

    for project in projects {
        println!("Refreshing {}...", project.repo_slug);
        let project_dest = dest_dir.join(&project.repo_slug);
        let download_res = rt.block_on(async {
            downloader
                .download_and_extract(&project, &project_dest, &project.preserve_patterns)
                .await
        });

        match download_res {
            Ok(()) => {
                println!("Successfully refreshed {}", project.repo_slug);
                println!("Scanning project {}...", project.repo_slug);
                let scan_config = ScanConfig {
                    max_depth: rustodian_types::scan::DEFAULT_MAX_DEPTH,
                    follow_symlinks: false,
                    exclude_patterns: vec![],
                };
                match custodian.scan(&project_dest, &scan_config) {
                    Ok(report) => {
                        println!("Scan completed. Found {} projects.", report.projects_found);
                        match custodian.find_by_path(&project_dest) {
                            Ok(Some(proj)) => {
                                println!("Bootstrapping and verifying project {}...", proj.name);
                                match custodian.bootstrap_and_verify(&proj.id) {
                                    Ok(()) => println!(
                                        "Successfully bootstrapped and verified {}!",
                                        proj.name
                                    ),
                                    Err(e) => println!(
                                        "Failed to bootstrap and verify {}: {}",
                                        proj.name, e
                                    ),
                                }
                            }
                            Ok(None) => {
                                println!(
                                    "Could not find the project in database by path: {}",
                                    project_dest.display()
                                );
                            }
                            Err(e) => {
                                println!("Failed to query project by path: {e}");
                            }
                        }
                    }
                    Err(e) => {
                        println!("Failed to scan project {}: {}", project.repo_slug, e);
                    }
                }
            }
            Err(e) => println!("Failed to refresh {}: {}", project.repo_slug, e),
        }
    }
    Ok(())
}

```

### Path: ./crates/rustodian-cli/src/commands/janitor.rs
```
use anyhow::{Result, anyhow};
use comfy_table::Table;

use rustodian_core::{Custodian, janitor::JanitorTargetResult};

use crate::OutputFormat;

pub fn execute(
    custodian: &Custodian,
    project_query: &str,
    dry_run: bool,
    format: &OutputFormat,
) -> Result<()> {
    let project = custodian
        .find_project(project_query)?
        .ok_or_else(|| anyhow!("Project not found: {project_query}"))?;

    let janitor = rustodian_core::janitor::DigitalJanitor::new(custodian);
    let report = janitor.clean(&project, dry_run)?;

    match format {
        OutputFormat::Json => {
            let json = serde_json::json!({
                "targets": report.targets.iter().map(json_target).collect::<Vec<_>>(),
                "bytes_reclaimed": report.bytes_reclaimed,
                "dry_run": report.dry_run,
            });
            let json_str = serde_json::to_string_pretty(&json)?;
            println!("{json_str}");
        }
        OutputFormat::Table => {
            let mut table = Table::new();
            table.set_header(vec!["Cruft Target", "Outcome", "Size", "Reason"]);

            let mut targets: Vec<&JanitorTargetResult> = report.targets.iter().collect();
            targets.sort_by(|left, right| {
                let left_actionable = matches!(
                    left.outcome,
                    rustodian_core::janitor::JanitorOutcome::Reclaimable
                        | rustodian_core::janitor::JanitorOutcome::Removed
                );
                let right_actionable = matches!(
                    right.outcome,
                    rustodian_core::janitor::JanitorOutcome::Reclaimable
                        | rustodian_core::janitor::JanitorOutcome::Removed
                );
                right_actionable
                    .cmp(&left_actionable)
                    .then_with(|| right.size_bytes.cmp(&left.size_bytes))
                    .then_with(|| left.path.cmp(&right.path))
            });

            for target in targets {
                table.add_row(vec![
                    target.target.clone(),
                    target.outcome.as_str().to_string(),
                    target
                        .size_bytes
                        .map_or_else(|| "-".to_string(), format_bytes),
                    target
                        .reason
                        .as_deref()
                        .map_or_else(String::new, concise_reason),
                ]);
            }

            table.add_row(vec![
                "Total".to_string(),
                if report.dry_run {
                    "reclaimable".to_string()
                } else {
                    "reclaimed".to_string()
                },
                format_bytes(report.bytes_reclaimed),
                String::new(),
            ]);

            println!("{table}");
        }
    }

    if !dry_run && report.has_failures() {
        return Err(anyhow!("Janitor purge completed with target failures"));
    }

    Ok(())
}

fn json_target(target: &JanitorTargetResult) -> serde_json::Value {
    serde_json::json!({
        "target": target.target,
        "path": target.path.display().to_string(),
        "size_bytes": target.size_bytes,
        "outcome": target.outcome.as_str(),
        "reason": target.reason,
    })
}

#[allow(clippy::cast_precision_loss)]
fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn concise_reason(reason: &str) -> String {
    const LIMIT: usize = 72;
    if reason.chars().count() <= LIMIT {
        return reason.to_string();
    }
    format!("{}…", reason.chars().take(LIMIT - 1).collect::<String>())
}

```

### Path: ./crates/rustodian-cli/src/commands/config.rs
```
//! The `config` command.

use std::env;
use std::path::Path;

use anyhow::Result;

use crate::OutputFormat;

pub fn execute(db_path: &Path, format: &OutputFormat) -> Result<()> {
    let scan_root = env::var("RUSTODIAN_SCAN_ROOT").unwrap_or_else(|_| ".".to_string());

    match format {
        OutputFormat::Table => {
            let mut table = comfy_table::Table::new();
            table.set_header(vec!["Configuration", "Value"]);

            table.add_row(vec![
                "Database Path".to_string(),
                db_path.display().to_string(),
            ]);
            table.add_row(vec!["Scan Root".to_string(), scan_root]);

            println!("{table}");
        }
        OutputFormat::Json => {
            #[derive(serde::Serialize)]
            struct ConfigOutput<'a> {
                db_path: &'a Path,
                scan_root: &'a str,
            }

            let output = ConfigOutput {
                db_path,
                scan_root: &scan_root,
            };

            let json = serde_json::to_string_pretty(&output)?;
            println!("{json}");
        }
    }

    Ok(())
}

```

### Path: ./crates/rustodian-cli/src/commands/list.rs
```
//! The `list` command.

use anyhow::Result;

use rustodian_core::Custodian;

use crate::OutputFormat;

pub fn execute(custodian: &Custodian, language: Option<&str>, format: &OutputFormat) -> Result<()> {
    let mut projects = custodian.list()?;

    if let Some(lang) = language {
        let lang = lang.to_lowercase();
        projects.retain(|p| {
            p.languages
                .iter()
                .any(|l| format!("{:?}", l.language).to_lowercase() == lang)
        });
    }

    match format {
        OutputFormat::Table => {
            if projects.is_empty() {
                println!("No projects found.");
                return Ok(());
            }

            let mut table = comfy_table::Table::new();
            table.set_header(vec!["Name", "Path", "Languages", "VCS"]);

            for p in projects {
                let langs: Vec<String> = p
                    .languages
                    .iter()
                    .map(|l| format!("{:?}", l.language))
                    .collect();

                let vcs = if let Some(vcs) = p.vcs {
                    format!(
                        "{:?} ({})",
                        vcs.vcs_type,
                        vcs.branch.unwrap_or_else(|| "detached".to_string())
                    )
                } else {
                    "None".to_string()
                };

                table.add_row(vec![
                    p.name,
                    p.path.display().to_string(),
                    langs.join(", "),
                    vcs,
                ]);
            }

            println!("{table}");
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&projects)?;
            println!("{json}");
        }
    }

    Ok(())
}

```

### Path: ./crates/rustodian-cli/src/commands/run.rs
```
//! The `run` command.

use anyhow::{Context, Result};
use rustodian_core::Custodian;

pub fn execute(custodian: &Custodian, project_query: &str, command_name: &str) -> Result<()> {
    println!("Running command '{command_name}' in project '{project_query}'...");
    custodian
        .run_command(project_query, command_name)
        .context("Failed to run command")?;
    println!("Command executed successfully.");
    Ok(())
}

```

### Path: ./crates/rustodian-cli/src/commands/info.rs
```
//! The `info` command.

use anyhow::{Result, anyhow};
use comfy_table::Table;

use rustodian_core::Custodian;

use crate::OutputFormat;

pub fn execute(custodian: &Custodian, project_query: &str, format: &OutputFormat) -> Result<()> {
    let project = custodian
        .find_project(project_query)?
        .ok_or_else(|| anyhow!("Project not found: {project_query}"))?;

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&project)?);
        }
        OutputFormat::Table => {
            println!("Project Info: {}", project.name);
            println!("ID: {}", project.id);
            println!("Path: {}", project.path.display());
            if !project.languages.is_empty() {
                let langs = project
                    .languages
                    .iter()
                    .map(|l| l.language.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                println!("Languages: {langs}");
            }
            if let Some(vcs) = &project.vcs {
                if let Some(remote) = &vcs.remote_url {
                    println!("Git Remote: {remote}");
                }
                if let Some(commit) = &vcs.last_commit {
                    println!("Commit: {}", commit.sha);
                }
            }
            println!("Discovered: {}", project.discovered_at.to_rfc3339());

            if project.metadata.commands.is_empty() {
                println!("\nNo runnable commands discovered.");
            } else {
                println!("\nDiscovered Commands:");
                let mut table = Table::new();
                table.set_header(vec!["Name", "Command", "Source", "Description"]);
                for cmd in &project.metadata.commands {
                    table.add_row(vec![
                        &cmd.name,
                        &cmd.command,
                        &cmd.source,
                        cmd.description.as_deref().unwrap_or(""),
                    ]);
                }
                println!("{table}");
            }
        }
    }

    Ok(())
}

```

### Path: ./crates/rustodian-cli/src/commands/scan.rs
```
//! The `scan` command.

use std::path::Path;

use anyhow::Result;

use rustodian_core::Custodian;

use crate::OutputFormat;

pub fn execute(
    custodian: &Custodian,
    path: &Path,
    max_depth: usize,
    format: &OutputFormat,
) -> Result<()> {
    let config = rustodian_types::ScanConfig {
        max_depth,
        ..Default::default()
    };

    let report = custodian.scan(path, &config)?;

    match format {
        OutputFormat::Table => {
            println!("Scan Complete");
            println!("-------------");
            println!("Scan ID: {}", report.scan_id);
            println!("Projects Found:   {}", report.projects_found);
            println!("New Projects:     {}", report.projects_new);
            println!("Updated Projects: {}", report.projects_updated);
            if report.projects_purged > 0 {
                println!("Purged (dead):    {}", report.projects_purged);
            }
        }
        OutputFormat::Json => {
            println!(
                "{{\"scan_id\":\"{}\",\"projects_found\":{},\"projects_new\":{},\"projects_updated\":{},\"projects_purged\":{}}}",
                report.scan_id,
                report.projects_found,
                report.projects_new,
                report.projects_updated,
                report.projects_purged
            );
        }
    }

    Ok(())
}

```

### Path: ./crates/rustodian-cli/src/commands/logs.rs
```
//! The `logs` command.

use anyhow::{Context, Result};
use rustodian_core::Custodian;
use rustodian_storage::SqliteStore;

use crate::OutputFormat;

pub fn execute(
    custodian: &Custodian,
    store: &SqliteStore,
    project_query: &str,
    limit: usize,
    format: &OutputFormat,
) -> Result<()> {
    let project = custodian
        .find_project(project_query)
        .context("Failed to find project")?
        .ok_or_else(|| anyhow::anyhow!("Project not found: {project_query}"))?;

    let logs = store
        .list_logs(&project.id.to_string(), limit)
        .context("Failed to list logs")?;

    match format {
        OutputFormat::Table => {
            if logs.is_empty() {
                println!("No logs found for project '{}'", project.name);
                return Ok(());
            }
            let mut table = comfy_table::Table::new();
            table.set_header(vec!["ID", "Command", "Run At", "Exit Code", "Log Snippet"]);
            for log in logs {
                let snippet = log
                    .log_text
                    .lines()
                    .last()
                    .unwrap_or("")
                    .chars()
                    .take(50)
                    .collect::<String>();
                let exit_code = log
                    .exit_code
                    .map_or_else(|| "running".to_string(), |c| c.to_string());
                table.add_row(vec![
                    log.id,
                    log.command_name,
                    log.run_at.to_string(),
                    exit_code,
                    snippet,
                ]);
            }
            println!("{table}");
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&logs)?;
            println!("{json}");
        }
    }
    Ok(())
}

```

### Path: ./crates/rustodian-cli/src/commands/status.rs
```
//! The `status` command.

use anyhow::Result;

use rustodian_core::Custodian;

use crate::OutputFormat;

pub fn execute(custodian: &Custodian, format: &OutputFormat) -> Result<()> {
    let status = custodian.status()?;

    match format {
        OutputFormat::Table => {
            let mut table = comfy_table::Table::new();
            table.set_header(vec!["Metric", "Value"]);

            table.add_row(vec![
                "Total Projects".to_string(),
                status.total_projects.to_string(),
            ]);

            if let Some(scan) = &status.last_scan {
                let scan_time = if let Some(completed_at) = scan.completed_at {
                    completed_at.to_rfc3339()
                } else {
                    scan.started_at.to_rfc3339()
                };

                table.add_row(vec!["Last Scan Time".to_string(), scan_time]);
                table.add_row(vec![
                    "Last Scan Status".to_string(),
                    scan.status.to_string(),
                ]);
                table.add_row(vec![
                    "Last Scan Projects Found".to_string(),
                    scan.projects_found.to_string(),
                ]);
                table.add_row(vec![
                    "Last Scan Root Path".to_string(),
                    scan.root_path.display().to_string(),
                ]);
            } else {
                table.add_row(vec!["Last Scan", "None"]);
            }

            if status.languages.is_empty() {
                table.add_row(vec!["Languages", "None"]);
            } else {
                let langs: Vec<String> = status
                    .languages
                    .iter()
                    .map(|(lang, count)| format!("{lang} ({count})"))
                    .collect();
                table.add_row(vec!["Languages".to_string(), langs.join(", ")]);
            }

            println!("{table}");
        }
        OutputFormat::Json => {
            let json = serde_json::json!({
                "total_projects": status.total_projects,
                "last_scan": status.last_scan,
                "languages": status.languages,
            });
            let json_str = serde_json::to_string_pretty(&json)?;
            println!("{json_str}");
        }
    }

    Ok(())
}

```

### Path: ./crates/rustodian-cli/src/output.rs
```
//! Output formatting and tracing initialization.

use tracing_subscriber::EnvFilter;

/// Initialize tracing with the given verbosity level.
///
/// - 0: warn
/// - 1: info
/// - 2: debug
/// - 3+: trace
pub fn init_tracing(verbosity: u8) {
    let filter = match verbosity {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();
}

```

### Path: ./crates/rustodian-cli/src/main.rs
```
//! # Rustodian CLI

//!
//! Department of Project Custodianship 🏛️
//!
//! Command-line entry point for the Rustodian project observatory.
//! This is the composition root — it wires infrastructure implementations
//! to the core orchestrator and dispatches CLI commands.

mod commands;
mod output;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{ArgAction, Parser, Subcommand, ValueEnum};
use tracing::info;

use rustodian_core::Custodian;
use rustodian_git::Git2Inspector;
use rustodian_scanner::FsScanner;
use rustodian_storage::SqliteStore;

/// Rustodian: Department of Project Custodianship 🏛️
///
/// A personal project observatory that discovers, indexes,
/// and monitors your software projects.
#[derive(Parser)]
#[command(name = "rustodian", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output format
    #[arg(long, alias = "output", global = true, default_value = "table")]
    format: OutputFormat,

    /// Verbosity level (-v, -vv, -vvv)
    #[arg(short, long, global = true, action = ArgAction::Count)]
    verbose: u8,

    /// Path to database file
    #[arg(long, global = true, env = "RUSTODIAN_DB")]
    db: Option<PathBuf>,
}

/// Available output formats.
#[derive(Debug, Clone, ValueEnum)]
enum OutputFormat {
    Table,
    Json,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan a directory tree for software projects
    Scan {
        /// Root directory to scan
        #[arg(short, long, env = "RUSTODIAN_SCAN_ROOT", default_value = ".")]
        path: PathBuf,

        /// Maximum directory depth
        #[arg(long, default_value_t = rustodian_types::scan::DEFAULT_MAX_DEPTH)]
        max_depth: usize,
    },

    /// List all tracked projects
    List {
        /// Filter by language
        #[arg(long)]
        language: Option<String>,
    },

    /// Show observatory status summary
    Status,

    /// Manage remote GitHub projects
    Remote {
        #[command(subcommand)]
        command: RemoteCommands,
    },

    /// Show detailed info about a specific project
    Info {
        /// Project name or ID
        project: String,
    },

    /// Purge build artifacts and cruft from a project
    Janitor {
        /// Project name or ID
        project: String,

        /// Do not actually delete anything, just report what would be deleted
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true")]
        dry_run: bool,

        /// Actually delete the cruft (disable dry-run)
        #[arg(long, alias = "no-dry-run")]
        purge: bool,
    },

    /// Run a discovered command for a project
    Run {
        /// Project name or ID
        project: String,
        /// Command name to run
        command: String,
    },

    /// View logs for a project
    Logs {
        /// Project name or ID
        project: String,

        /// Limit number of logs shown
        #[arg(long, default_value = "10")]
        limit: usize,
    },

    /// Print active configuration
    Config,
}

#[derive(Subcommand)]
enum RemoteCommands {
    /// Add a remote project to track
    Add {
        /// GitHub repository slug (e.g., username/repo)
        repo_slug: String,

        /// Glob patterns of files to preserve during refresh
        #[arg(long, value_delimiter = ',')]
        preserve: Vec<String>,
    },

    /// List tracked remote projects
    List,

    /// Refresh (download) all tracked remote projects
    Refresh {
        /// Destination directory
        #[arg(long, default_value = ".")]
        dest: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing based on verbosity
    output::init_tracing(cli.verbose);

    info!("Rustodian starting");

    // Wire up infrastructure
    let db_path = match cli.db {
        Some(path) => path,
        None => SqliteStore::default_path().context("failed to determine database path")?,
    };

    let store = SqliteStore::open(&db_path).context("failed to open database")?;
    store.migrate().context("failed to run migrations")?;

    let scanner = FsScanner;
    let git = Git2Inspector;

    let runner = rustodian_core::runner::DefaultCommandRunner;
    let custodian = Custodian::new(
        Box::new(store.clone()),
        Box::new(scanner),
        Box::new(git),
        Box::new(runner),
    );

    // Dispatch command
    match cli.command {
        Commands::Scan { path, max_depth } => {
            commands::scan::execute(&custodian, &path, max_depth, &cli.format)
        }
        Commands::List { language } => {
            commands::list::execute(&custodian, language.as_deref(), &cli.format)
        }
        Commands::Status => commands::status::execute(&custodian, &cli.format),
        Commands::Remote { command } => match command {
            RemoteCommands::Add {
                repo_slug,
                preserve,
            } => commands::remote::execute_add(&store, &repo_slug, &preserve),
            RemoteCommands::List => commands::remote::execute_list(&store, &cli.format),
            RemoteCommands::Refresh { dest } => {
                commands::remote::execute_refresh(&custodian, &store, &dest)
            }
        },
        Commands::Info { project } => commands::info::execute(&custodian, &project, &cli.format),
        Commands::Janitor {
            project,
            dry_run,
            purge,
        } => {
            let is_dry_run = dry_run && !purge;
            commands::janitor::execute(&custodian, &project, is_dry_run, &cli.format)
        }
        Commands::Run { project, command } => {
            commands::run::execute(&custodian, &project, &command)
        }
        Commands::Logs { project, limit } => {
            commands::logs::execute(&custodian, &store, &project, limit, &cli.format)
        }
        Commands::Config => commands::config::execute(&db_path, &cli.format),
    }
}

```

### Path: ./crates/rustodian-types/src/lib.rs
```
//! # Rustodian Types
//!
//! Shared data structures and enums for the Rustodian project observatory.
//! This crate contains pure data — no behavior, no traits, no I/O.

pub mod language;
pub mod project;
pub mod scan;
pub mod vcs;

// Re-export key types for convenience
pub use language::{DetectionConfidence, Language, LanguageDetection, LanguageMarker};
pub use project::RemoteProject;
pub use project::{Project, ProjectCommand, ProjectId, ProjectLog, ProjectMetadata};
pub use scan::{ScanConfig, ScanId, ScanRecord, ScanStatus};
pub use vcs::{CommitInfo, PullRequest, VcsInfo, VcsType};

```

### Path: ./crates/rustodian-types/src/scan.rs
```
//! Scan operation types.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Opaque scan identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ScanId(pub Uuid);

impl ScanId {
    /// Create a new random scan ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ScanId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ScanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Record of a scan operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanRecord {
    pub id: ScanId,
    pub root_path: PathBuf,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub projects_found: usize,
    pub status: ScanStatus,
}

/// Current state of a scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanStatus {
    Running,
    Completed,
    Failed,
}

impl std::fmt::Display for ScanStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

pub const DEFAULT_MAX_DEPTH: usize = 5;

/// Configuration for a scan operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanConfig {
    /// Maximum directory depth to traverse.
    pub max_depth: usize,
    /// Directories to skip (in addition to .gitignore rules).
    pub exclude_patterns: Vec<String>,
    /// Whether to follow symbolic links.
    pub follow_symlinks: bool,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            max_depth: DEFAULT_MAX_DEPTH,
            exclude_patterns: Vec::new(),
            follow_symlinks: false,
        }
    }
}

```

### Path: ./crates/rustodian-types/src/vcs.rs
```
//! Version control system types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Information about a project's version control.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VcsInfo {
    pub vcs_type: VcsType,
    pub branch: Option<String>,
    pub remote_url: Option<String>,
    pub is_dirty: bool,
    pub last_commit: Option<CommitInfo>,
}

/// Supported version control systems.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VcsType {
    Git,
}

impl std::fmt::Display for VcsType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Git => write!(f, "Git"),
        }
    }
}

/// Information about a specific commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    pub sha: String,
    pub message: String,
    pub author: String,
    pub timestamp: DateTime<Utc>,
}

/// A pull request from a remote repository.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub branch: String,
    pub url: String,
    pub updated_at: DateTime<Utc>,
    pub is_draft: bool,
}

```

### Path: ./crates/rustodian-types/src/project.rs
```
//! Project domain types.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::language::LanguageDetection;
use crate::vcs::VcsInfo;

/// Opaque project identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectId(pub Uuid);

impl ProjectId {
    /// Create a new random project ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ProjectId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ProjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A discovered software project on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub path: PathBuf,
    pub languages: Vec<LanguageDetection>,
    pub vcs: Option<VcsInfo>,
    pub discovered_at: DateTime<Utc>,
    pub last_scanned_at: Option<DateTime<Utc>>,
    pub metadata: ProjectMetadata,
}

/// A runnable command discovered in a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCommand {
    pub name: String,
    pub description: Option<String>,
    pub command: String,
    pub source: String, // e.g., "Cargo.toml", "package.json", "justfile"
    #[serde(default)]
    pub use_shell: bool,
}

/// Extensible metadata bag.
///
/// Uses `serde(flatten)` with a JSON value to allow future fields
/// without requiring database schema migrations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectMetadata {
    pub description: Option<String>,
    pub tags: Vec<String>,
    #[serde(default)]
    pub commands: Vec<ProjectCommand>,
    /// Catch-all for future fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RemoteProject {
    pub repo_slug: String,
    pub preserve_patterns: Vec<String>,
}

/// A persisted record of a command execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectLog {
    pub id: String,
    pub project_id: String,
    pub command_name: String,
    pub exit_code: Option<i32>,
    pub log_text: String,
    pub run_at: DateTime<Utc>,
}

```

### Path: ./crates/rustodian-types/src/language.rs
```
//! Language detection types.

use serde::{Deserialize, Serialize};

/// Languages that Rustodian can detect.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Rust,
    Python,
    Node,
    Go,
    Ruby,
    Zig,
    /// A language we detected but don't have first-class support for.
    Unknown(String),
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rust => write!(f, "Rust"),
            Self::Python => write!(f, "Python"),
            Self::Node => write!(f, "Node"),
            Self::Go => write!(f, "Go"),
            Self::Ruby => write!(f, "Ruby"),
            Self::Zig => write!(f, "Zig"),
            Self::Unknown(name) => write!(f, "{name}"),
        }
    }
}

/// A language detection result with confidence and evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageDetection {
    pub language: Language,
    pub confidence: DetectionConfidence,
    pub markers: Vec<LanguageMarker>,
}

/// How confident we are in a language detection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DetectionConfidence {
    /// Found a definitive marker (e.g., Cargo.toml for Rust).
    High,
    /// Found supporting evidence (e.g., .rs files but no Cargo.toml).
    Medium,
    /// Weak signal (e.g., file extension only).
    Low,
}

impl std::fmt::Display for DetectionConfidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::High => write!(f, "high"),
            Self::Medium => write!(f, "medium"),
            Self::Low => write!(f, "low"),
        }
    }
}

/// Evidence for why a language was detected.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguageMarker {
    /// Found a package manifest (e.g., Cargo.toml, package.json).
    ManifestFile(String),
    /// Found a lock file (e.g., Cargo.lock, yarn.lock).
    LockFile(String),
    /// Found a configuration file (e.g., .eslintrc, pyproject.toml).
    ConfigFile(String),
    /// Found source files with this extension.
    FileExtension(String),
}

```

### Path: ./crates/rustodian-storage/src/store.rs
```
//! `SQLite` implementation of [`ProjectStore`].

use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};
use tracing::debug;

use r2d2_sqlite::SqliteConnectionManager;

use rustodian_core::CoreError;
use rustodian_core::traits::ProjectStore;
use rustodian_types::{
    Project, ProjectId, ProjectLog, ProjectMetadata, ScanId, ScanRecord, ScanStatus,
};

use crate::error::StorageError;
use crate::migrations;

/// `SQLite`-backed project store.
///
/// Uses an `r2d2` connection pool to allow concurrent reads/writes and prevent lock contention.
#[derive(Clone)]
pub struct SqliteStore {
    pub(crate) pool: std::sync::Arc<r2d2::Pool<SqliteConnectionManager>>,
}

impl SqliteStore {
    /// Open or create a database at the given path.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        debug!(path = %path.display(), "Opening database pool");
        let manager = SqliteConnectionManager::file(path).with_init(|c| {
            c.execute_batch(
                "
                    PRAGMA journal_mode = WAL;
                    PRAGMA synchronous = NORMAL;
                    PRAGMA busy_timeout = 5000;
                    PRAGMA foreign_keys = ON;
                ",
            )
        });
        let pool = r2d2::Pool::new(manager)
            .map_err(|e| StorageError::Migration(format!("failed to create database pool: {e}")))?;

        Ok(Self {
            pool: std::sync::Arc::new(pool),
        })
    }

    /// Create an in-memory database (for testing).
    pub fn open_in_memory() -> Result<Self, StorageError> {
        debug!("Opening in-memory database pool");
        let uuid = uuid::Uuid::new_v4().to_string();
        let db_url = format!("file:{uuid}?mode=memory&cache=shared");
        let manager = SqliteConnectionManager::file(&db_url).with_init(|c| {
            c.execute_batch(
                "
                    PRAGMA journal_mode = WAL;
                    PRAGMA synchronous = NORMAL;
                    PRAGMA busy_timeout = 5000;
                    PRAGMA foreign_keys = ON;
                ",
            )
        });
        let pool = r2d2::Pool::builder()
            .max_size(1)
            .build(manager)
            .map_err(|e| {
                StorageError::Migration(format!("failed to create in-memory pool: {e}"))
            })?;

        Ok(Self {
            pool: std::sync::Arc::new(pool),
        })
    }

    /// Run all pending database migrations.
    pub fn migrate(&self) -> Result<(), StorageError> {
        let conn = self
            .get_conn()
            .map_err(|e| StorageError::Migration(e.to_string()))?;
        migrations::run_migrations(&conn)
    }

    /// Get the path to the default database location.
    ///
    /// Uses `$RUSTODIAN_DB` if set, otherwise `~/.local/share/rustodian/rustodian.db`.
    pub fn default_path() -> Result<PathBuf, CoreError> {
        if let Ok(path) = std::env::var("RUSTODIAN_DB") {
            return Ok(PathBuf::from(path));
        }

        let data_dir = dirs_next_or_fallback();
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| CoreError::Internal(format!("failed to create data dir: {e}")))?;
        Ok(data_dir.join("rustodian.db"))
    }

    /// Get a pooled connection from the pool.
    pub(crate) fn get_conn(
        &self,
    ) -> Result<r2d2::PooledConnection<SqliteConnectionManager>, CoreError> {
        self.pool
            .get()
            .map_err(|e| CoreError::Storage(format!("failed to get database connection: {e}")))
    }
}

/// Get the data directory, with a fallback if dirs isn't available.
fn dirs_next_or_fallback() -> PathBuf {
    // Simple fallback: ~/.local/share/rustodian
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("rustodian")
}

/// Parse raw column values into a [`Project`].
///
/// Used by `get_project`, `list_projects`, and `find_by_path` to avoid
/// duplicating the deserialization logic.
fn parse_project_row(
    id_str: &str,
    name: String,
    path_str: String,
    disc_str: &str,
    scan_str: Option<String>,
    meta_str: &str,
) -> Result<Project, CoreError> {
    let id = ProjectId(
        uuid::Uuid::parse_str(id_str)
            .map_err(|e| CoreError::Storage(format!("invalid project UUID '{id_str}': {e}")))?,
    );
    let path = PathBuf::from(path_str);
    let discovered_at = chrono::DateTime::parse_from_rfc3339(disc_str)
        .map_err(|e| CoreError::Storage(format!("invalid timestamp '{disc_str}': {e}")))?
        .with_timezone(&chrono::Utc);
    let last_scanned_at = scan_str
        .map(|s| {
            chrono::DateTime::parse_from_rfc3339(&s)
                .map_err(|e| CoreError::Storage(format!("invalid timestamp '{s}': {e}")))
                .map(|dt| dt.with_timezone(&chrono::Utc))
        })
        .transpose()?;

    let meta_json: serde_json::Value = serde_json::from_str(meta_str).map_err(|e| {
        CoreError::Storage(format!("invalid metadata JSON for project '{name}': {e}"))
    })?;

    let meta_val = meta_json.get("meta").ok_or_else(|| {
        CoreError::Storage(format!(
            "metadata JSON for project '{name}' missing 'meta' field"
        ))
    })?;
    let metadata: ProjectMetadata = serde_json::from_value(meta_val.clone()).map_err(|e| {
        CoreError::Storage(format!(
            "failed to deserialize ProjectMetadata for project '{name}': {e}"
        ))
    })?;

    let vcs_val = meta_json.get("vcs").ok_or_else(|| {
        CoreError::Storage(format!(
            "metadata JSON for project '{name}' missing 'vcs' field"
        ))
    })?;
    let vcs = serde_json::from_value(vcs_val.clone()).map_err(|e| {
        CoreError::Storage(format!(
            "failed to deserialize VCS metadata for project '{name}': {e}"
        ))
    })?;

    let lang_val = meta_json.get("languages").ok_or_else(|| {
        CoreError::Storage(format!(
            "metadata JSON for project '{name}' missing 'languages' field"
        ))
    })?;
    let languages = serde_json::from_value(lang_val.clone()).map_err(|e| {
        CoreError::Storage(format!(
            "failed to deserialize languages metadata for project '{name}': {e}"
        ))
    })?;

    Ok(Project {
        id,
        name,
        path,
        languages,
        vcs,
        discovered_at,
        last_scanned_at,
        metadata,
    })
}

impl ProjectStore for SqliteStore {
    fn save_project(&self, project: &Project) -> Result<ProjectId, CoreError> {
        let mut conn = self.get_conn()?;
        let tx = conn
            .transaction()
            .map_err(|e| CoreError::Storage(format!("failed to begin transaction: {e}")))?;

        tx.execute(
            "INSERT INTO projects (id, name, path, discovered_at, last_scanned_at, metadata_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(path) DO UPDATE SET
                name=excluded.name,
                discovered_at=excluded.discovered_at,
                last_scanned_at=excluded.last_scanned_at,
                metadata_json=excluded.metadata_json;",
            params![
                project.id.to_string(),
                project.name,
                project.path.to_string_lossy(),
                project.discovered_at.to_rfc3339(),
                project.last_scanned_at.map(|d| d.to_rfc3339()),
                serde_json::json!({
                    "meta": project.metadata,
                    "vcs": project.vcs,
                    "languages": project.languages
                })
                .to_string()
            ],
        )
        .map_err(|e| CoreError::Storage(format!("failed to save project: {e}")))?;

        // we'll update the project languages table
        tx.execute(
            "DELETE FROM project_languages WHERE project_id = ?1",
            params![project.id.to_string()],
        )
        .map_err(|e| CoreError::Storage(format!("failed to clean languages: {e}")))?;

        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO project_languages (project_id, language, confidence) VALUES (?1, ?2, ?3)",
            ).map_err(|e| CoreError::Storage(format!("failed to prepare statement: {e}")))?;

            for detection in &project.languages {
                stmt.execute(params![
                    project.id.to_string(),
                    detection.language.to_string(),
                    detection.confidence.to_string()
                ])
                .map_err(|e| {
                    CoreError::Storage(format!("failed to save project languages: {e}"))
                })?;
            }
        }

        tx.commit()
            .map_err(|e| CoreError::Storage(format!("failed to commit transaction: {e}")))?;

        Ok(project.id.clone())
    }

    fn get_project(&self, id: &ProjectId) -> Result<Option<Project>, CoreError> {
        let conn = self.get_conn()?;

        let mut stmt = conn.prepare("SELECT id, name, path, discovered_at, last_scanned_at, metadata_json FROM projects WHERE id = ?1")
            .map_err(|e| CoreError::Storage(format!("prepare error: {e}")))?;

        let project = stmt
            .query_row(params![id.to_string()], |row| {
                let id_str: String = row.get(0)?;
                let name: String = row.get(1)?;
                let path_str: String = row.get(2)?;
                let disc_str: String = row.get(3)?;
                let scan_str: Option<String> = row.get(4)?;
                let meta_str: String = row.get(5)?;

                Ok((id_str, name, path_str, disc_str, scan_str, meta_str))
            })
            .optional()
            .map_err(|e| CoreError::Storage(format!("query error: {e}")))?;

        if let Some((id_str, name, path_str, disc_str, scan_str, meta_str)) = project {
            Ok(Some(parse_project_row(
                &id_str, name, path_str, &disc_str, scan_str, &meta_str,
            )?))
        } else {
            Ok(None)
        }
    }

    fn list_projects(&self) -> Result<Vec<Project>, CoreError> {
        let conn = self.get_conn()?;

        let mut stmt = conn.prepare("SELECT id, name, path, discovered_at, last_scanned_at, metadata_json FROM projects")
            .map_err(|e| CoreError::Storage(format!("prepare error: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                let id_str: String = row.get(0)?;
                let name: String = row.get(1)?;
                let path_str: String = row.get(2)?;
                let disc_str: String = row.get(3)?;
                let scan_str: Option<String> = row.get(4)?;
                let meta_str: String = row.get(5)?;
                Ok((id_str, name, path_str, disc_str, scan_str, meta_str))
            })
            .map_err(|e| CoreError::Storage(format!("query map error: {e}")))?;

        let mut projects = Vec::new();
        for row_result in rows {
            let (id_str, name, path_str, disc_str, scan_str, meta_str) = match row_result {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("Skipping malformed project row: {e}");
                    continue;
                }
            };
            match parse_project_row(
                &id_str,
                name,
                path_str.clone(),
                &disc_str,
                scan_str,
                &meta_str,
            ) {
                Ok(proj) => projects.push(proj),
                Err(e) => {
                    tracing::warn!("Skipping invalid project data for path '{path_str}': {e}");
                }
            }
        }
        Ok(projects)
    }

    fn delete_project(&self, id: &ProjectId) -> Result<bool, CoreError> {
        let conn = self.get_conn()?;
        let count = conn
            .execute(
                "DELETE FROM projects WHERE id = ?1",
                params![id.to_string()],
            )
            .map_err(|e| CoreError::Storage(format!("delete error: {e}")))?;
        Ok(count > 0)
    }

    fn find_by_path(&self, path: &Path) -> Result<Option<Project>, CoreError> {
        let conn = self.get_conn()?;

        let mut stmt = conn.prepare("SELECT id, name, path, discovered_at, last_scanned_at, metadata_json FROM projects WHERE path = ?1")
            .map_err(|e| CoreError::Storage(format!("prepare error: {e}")))?;

        let project = stmt
            .query_row(params![path.to_string_lossy()], |row| {
                let id_str: String = row.get(0)?;
                let name: String = row.get(1)?;
                let path_str: String = row.get(2)?;
                let disc_str: String = row.get(3)?;
                let scan_str: Option<String> = row.get(4)?;
                let meta_str: String = row.get(5)?;
                Ok((id_str, name, path_str, disc_str, scan_str, meta_str))
            })
            .optional()
            .map_err(|e| CoreError::Storage(format!("query error: {e}")))?;

        if let Some((id_str, name, path_str, disc_str, scan_str, meta_str)) = project {
            Ok(Some(parse_project_row(
                &id_str, name, path_str, &disc_str, scan_str, &meta_str,
            )?))
        } else {
            Ok(None)
        }
    }

    fn save_scan(&self, scan: &ScanRecord) -> Result<ScanId, CoreError> {
        let conn = self.get_conn()?;

        conn.execute(
            "INSERT INTO scans (id, root_path, started_at, completed_at, projects_found, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                completed_at=excluded.completed_at,
                projects_found=excluded.projects_found,
                status=excluded.status;",
            params![
                scan.id.to_string(),
                scan.root_path.to_string_lossy(),
                scan.started_at.to_rfc3339(),
                scan.completed_at.map(|d| d.to_rfc3339()),
                scan.projects_found,
                scan.status.to_string()
            ],
        )
        .map_err(|e| CoreError::Storage(format!("failed to save scan: {e}")))?;

        Ok(scan.id.clone())
    }

    fn get_latest_scan(&self) -> Result<Option<ScanRecord>, CoreError> {
        let conn = self.get_conn()?;

        let mut stmt = conn.prepare("SELECT id, root_path, started_at, completed_at, projects_found, status FROM scans ORDER BY started_at DESC LIMIT 1")
            .map_err(|e| CoreError::Storage(format!("prepare error: {e}")))?;

        let scan = stmt
            .query_row([], |row| {
                let id_str: String = row.get(0)?;
                let root_str: String = row.get(1)?;
                let start_str: String = row.get(2)?;
                let end_str: Option<String> = row.get(3)?;
                let found: usize = row.get(4)?;
                let status_str: String = row.get(5)?;
                Ok((id_str, root_str, start_str, end_str, found, status_str))
            })
            .optional()
            .map_err(|e| CoreError::Storage(format!("query error: {e}")))?;

        if let Some((id_str, root_str, start_str, end_str, found, status_str)) = scan {
            let id =
                ScanId(uuid::Uuid::parse_str(&id_str).map_err(|e| {
                    CoreError::Storage(format!("invalid scan UUID '{id_str}': {e}"))
                })?);
            let root_path = PathBuf::from(root_str);
            let started_at = chrono::DateTime::parse_from_rfc3339(&start_str)
                .map_err(|e| CoreError::Storage(format!("invalid timestamp '{start_str}': {e}")))?
                .with_timezone(&chrono::Utc);
            let completed_at = end_str
                .map(|s| {
                    chrono::DateTime::parse_from_rfc3339(&s)
                        .map_err(|e| CoreError::Storage(format!("invalid timestamp '{s}': {e}")))
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                })
                .transpose()?;
            let status = match status_str.as_str() {
                "running" => ScanStatus::Running,
                "completed" => ScanStatus::Completed,
                "failed" => ScanStatus::Failed,
                other => return Err(CoreError::Storage(format!("invalid scan status '{other}'"))),
            };

            Ok(Some(ScanRecord {
                id,
                root_path,
                started_at,
                completed_at,
                projects_found: found,
                status,
            }))
        } else {
            Ok(None)
        }
    }

    fn save_log(&self, log: &ProjectLog) -> Result<(), CoreError> {
        SqliteStore::save_log(self, log)
    }

    fn list_logs(&self, project_id: &str, limit: usize) -> Result<Vec<ProjectLog>, CoreError> {
        SqliteStore::list_logs(self, project_id, limit)
    }

    fn get_log(&self, id: &str) -> Result<Option<ProjectLog>, CoreError> {
        SqliteStore::get_log(self, id)
    }

    fn get_latest_log(&self, project_id: &str) -> Result<Option<ProjectLog>, CoreError> {
        SqliteStore::get_latest_log(self, project_id)
    }

    fn prune_logs(&self, project_id: &str, limit: usize) -> Result<usize, CoreError> {
        SqliteStore::prune_logs(self, project_id, limit)
    }
}

impl SqliteStore {
    pub fn get_setting(&self, key: &str) -> Result<Option<String>, CoreError> {
        let conn = self.get_conn()?;
        let mut stmt = conn
            .prepare("SELECT value FROM settings WHERE key = ?1")
            .map_err(|e| CoreError::Storage(format!("prepare error: {e}")))?;

        let value: Option<String> = stmt
            .query_row(params![key], |row| row.get(0))
            .optional()
            .map_err(|e| CoreError::Storage(format!("query error: {e}")))?;

        Ok(value)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), CoreError> {
        let conn = self.get_conn()?;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value;",
            params![key, value],
        )
        .map_err(|e| CoreError::Storage(format!("insert error: {e}")))?;

        Ok(())
    }

    pub fn list_settings(&self) -> Result<std::collections::HashMap<String, String>, CoreError> {
        let conn = self.get_conn()?;
        let mut stmt = conn
            .prepare("SELECT key, value FROM settings")
            .map_err(|e| CoreError::Storage(format!("prepare error: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                let key: String = row.get(0)?;
                let value: String = row.get(1)?;
                Ok((key, value))
            })
            .map_err(|e| CoreError::Storage(format!("query error: {e}")))?;

        let mut settings = std::collections::HashMap::new();
        for (k, v) in rows.flatten() {
            settings.insert(k, v);
        }
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_save_project_upsert_and_malformed_json() {
        use rustodian_core::traits::ProjectStore;
        use rustodian_types::{Project, ProjectId};
        use std::path::PathBuf;

        let store = SqliteStore::open_in_memory().unwrap();
        store.migrate().unwrap();

        let mut proj = Project {
            id: ProjectId::new(),
            name: "test_proj".to_string(),
            path: PathBuf::from("/test"),
            discovered_at: chrono::Utc::now(),
            last_scanned_at: None,
            vcs: None,
            languages: vec![],
            metadata: rustodian_types::ProjectMetadata::default(),
        };

        // Initial save
        let id = store.save_project(&proj).unwrap();

        // Upsert save
        proj.name = "test_proj_updated".to_string();
        store.save_project(&proj).unwrap();

        let loaded = store.get_project(&id).unwrap().unwrap();
        assert_eq!(loaded.name, "test_proj_updated");

        // Manually break the json
        let conn = store.get_conn().unwrap();
        conn.execute(
            "UPDATE projects SET metadata_json = 'not_json' WHERE id = ?1",
            rusqlite::params![id.to_string()],
        )
        .unwrap();
        drop(conn);

        let err = store.get_project(&id).unwrap_err();
        println!("{err}");
        assert!(err.to_string().contains("invalid metadata JSON"));
    }
    use super::*;

    #[test]
    fn test_open_in_memory() {
        let store = SqliteStore::open_in_memory().expect("should open in-memory db");
        store.migrate().expect("should run migrations");
    }

    #[test]
    fn test_migrations_idempotent() {
        let store = SqliteStore::open_in_memory().expect("should open");
        store.migrate().expect("first migration");
        store
            .migrate()
            .expect("second migration should be idempotent");
    }
}

```

### Path: ./crates/rustodian-storage/src/error.rs
```
//! Storage-specific error types.

use rustodian_core::CoreError;

/// Errors specific to the `SQLite` storage implementation.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// `SQLite` error.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Data serialization/deserialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Migration error.
    #[error("migration error: {0}")]
    Migration(String),
}

impl From<StorageError> for CoreError {
    fn from(err: StorageError) -> Self {
        CoreError::Storage(err.to_string())
    }
}

```

### Path: ./crates/rustodian-storage/src/migrations.rs
```
//! Database migration management.

use rusqlite::Connection;
use tracing::info;

use crate::error::StorageError;

/// The initial database schema.
const MIGRATION_001: &str = r"
CREATE TABLE IF NOT EXISTS projects (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    path            TEXT NOT NULL UNIQUE,
    discovered_at   TEXT NOT NULL,
    last_scanned_at TEXT,
    metadata_json   TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS project_languages (
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    language    TEXT NOT NULL,
    confidence  TEXT NOT NULL DEFAULT 'high',
    PRIMARY KEY (project_id, language)
);

CREATE TABLE IF NOT EXISTS scans (
    id              TEXT PRIMARY KEY,
    root_path       TEXT NOT NULL,
    started_at      TEXT NOT NULL,
    completed_at    TEXT,
    projects_found  INTEGER NOT NULL DEFAULT 0,
    status          TEXT NOT NULL DEFAULT 'running'
);

CREATE INDEX IF NOT EXISTS idx_projects_path ON projects(path);
CREATE INDEX IF NOT EXISTS idx_scans_started ON scans(started_at DESC);
";

/// Run all pending migrations.
pub fn run_migrations(conn: &Connection) -> Result<(), StorageError> {
    info!("Running database migrations");

    // Create migrations tracking table
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            id      INTEGER PRIMARY KEY,
            name    TEXT NOT NULL,
            applied TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .map_err(StorageError::Sqlite)?;

    // Check if migration 001 has been applied
    let applied: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM _migrations WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .map_err(StorageError::Sqlite)?;

    if !applied {
        info!("Applying migration 001: initial schema");
        conn.execute_batch(MIGRATION_001)
            .map_err(StorageError::Sqlite)?;
        conn.execute(
            "INSERT INTO _migrations (id, name) VALUES (1, 'initial_schema')",
            [],
        )
        .map_err(StorageError::Sqlite)?;
    }

    let applied_002: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM _migrations WHERE id = 2",
            [],
            |row| row.get(0),
        )
        .map_err(StorageError::Sqlite)?;
    if !applied_002 {
        info!("Applying migration 002: remote projects");
        conn.execute_batch(MIGRATION_002)
            .map_err(StorageError::Sqlite)?;
        conn.execute(
            "INSERT INTO _migrations (id, name) VALUES (2, 'remote_projects')",
            [],
        )
        .map_err(StorageError::Sqlite)?;
    }

    let applied_003: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM _migrations WHERE id = 3",
            [],
            |row| row.get(0),
        )
        .map_err(StorageError::Sqlite)?;
    if !applied_003 {
        info!("Applying migration 003: project logs");
        conn.execute_batch(MIGRATION_003)
            .map_err(StorageError::Sqlite)?;
        conn.execute(
            "INSERT INTO _migrations (id, name) VALUES (3, 'project_logs')",
            [],
        )
        .map_err(StorageError::Sqlite)?;
    }

    let applied_004: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM _migrations WHERE id = 4",
            [],
            |row| row.get(0),
        )
        .map_err(StorageError::Sqlite)?;
    if !applied_004 {
        info!("Applying migration 004: settings table");
        conn.execute_batch(MIGRATION_004)
            .map_err(StorageError::Sqlite)?;
        conn.execute(
            "INSERT INTO _migrations (id, name) VALUES (4, 'settings_table')",
            [],
        )
        .map_err(StorageError::Sqlite)?;
    }

    info!("Migrations complete");
    Ok(())
}
const MIGRATION_002: &str = r"
CREATE TABLE IF NOT EXISTS remote_projects (
    repo_slug         TEXT PRIMARY KEY,
    preserve_patterns TEXT NOT NULL DEFAULT '[]'
);
";

const MIGRATION_003: &str = r"
CREATE TABLE IF NOT EXISTS project_logs (
    id           TEXT PRIMARY KEY,
    project_id   TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    command_name TEXT NOT NULL,
    exit_code    INTEGER,
    log_text     TEXT NOT NULL DEFAULT '',
    run_at       TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_project_logs_project ON project_logs(project_id, run_at DESC);
";

const MIGRATION_004: &str = r"
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
";

```

### Path: ./crates/rustodian-storage/src/lib.rs
```
//! # Rustodian Storage
//!
//! SQLite-backed storage for Rustodian project data.
//!
//! This crate implements [`rustodian_core::ProjectStore`] using `rusqlite`.
//! It handles database initialization, migrations, and all persistence operations.

pub mod error;
pub mod log_store;
pub mod migrations;
pub mod store;

pub use log_store::ProjectLog;
pub use store::SqliteStore;
pub mod remote_store;

```

### Path: ./crates/rustodian-storage/src/log_store.rs
```
//! Persistence for command execution logs.

use chrono::Utc;
use rusqlite::{OptionalExtension, params};

use crate::store::SqliteStore;
use rustodian_core::CoreError;

pub use rustodian_types::ProjectLog;

impl SqliteStore {
    /// Persist a command execution log.
    pub fn save_log(&self, log: &ProjectLog) -> Result<(), CoreError> {
        let conn = self.get_conn()?;

        conn.execute(
            "INSERT INTO project_logs (id, project_id, command_name, exit_code, log_text, run_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                exit_code=excluded.exit_code,
                log_text=excluded.log_text",
            params![
                log.id,
                log.project_id,
                log.command_name,
                log.exit_code,
                log.log_text,
                log.run_at.to_rfc3339(),
            ],
        )
        .map_err(|e| CoreError::Storage(format!("failed to save log: {e}")))?;

        Ok(())
    }

    /// List execution logs for a project, ordered by most recent first.
    pub fn list_logs(&self, project_id: &str, limit: usize) -> Result<Vec<ProjectLog>, CoreError> {
        let conn = self.get_conn()?;

        let mut stmt = conn
            .prepare(
                "SELECT id, project_id, command_name, exit_code, log_text, run_at
                 FROM project_logs
                 WHERE project_id = ?1
                 ORDER BY run_at DESC
                 LIMIT ?2",
            )
            .map_err(|e| CoreError::Storage(format!("prepare error: {e}")))?;

        let rows = stmt
            .query_map(params![project_id, limit], |row| {
                let id: String = row.get(0)?;
                let project_id: String = row.get(1)?;
                let command_name: String = row.get(2)?;
                let exit_code: Option<i32> = row.get(3)?;
                let log_text: String = row.get(4)?;
                let run_at_str: String = row.get(5)?;
                Ok((
                    id,
                    project_id,
                    command_name,
                    exit_code,
                    log_text,
                    run_at_str,
                ))
            })
            .map_err(|e| CoreError::Storage(format!("query error: {e}")))?;

        let mut logs = Vec::new();
        for row in rows {
            let (id, project_id, command_name, exit_code, log_text, run_at_str) =
                row.map_err(|e| CoreError::Storage(format!("row error: {e}")))?;
            let run_at = chrono::DateTime::parse_from_rfc3339(&run_at_str)
                .map_err(|e| CoreError::Storage(format!("invalid timestamp '{run_at_str}': {e}")))?
                .with_timezone(&Utc);
            logs.push(ProjectLog {
                id,
                project_id,
                command_name,
                exit_code,
                log_text,
                run_at,
            });
        }
        Ok(logs)
    }

    /// Get a specific log entry by ID.
    pub fn get_log(&self, id: &str) -> Result<Option<ProjectLog>, CoreError> {
        let conn = self.get_conn()?;

        let mut stmt = conn
            .prepare(
                "SELECT id, project_id, command_name, exit_code, log_text, run_at
                 FROM project_logs
                 WHERE id = ?1",
            )
            .map_err(|e| CoreError::Storage(format!("prepare error: {e}")))?;

        let row = stmt
            .query_row(params![id], |row| {
                let id: String = row.get(0)?;
                let project_id: String = row.get(1)?;
                let command_name: String = row.get(2)?;
                let exit_code: Option<i32> = row.get(3)?;
                let log_text: String = row.get(4)?;
                let run_at_str: String = row.get(5)?;
                Ok((
                    id,
                    project_id,
                    command_name,
                    exit_code,
                    log_text,
                    run_at_str,
                ))
            })
            .optional()
            .map_err(|e| CoreError::Storage(format!("query error: {e}")))?;

        match row {
            Some((id, project_id, command_name, exit_code, log_text, run_at_str)) => {
                let run_at = chrono::DateTime::parse_from_rfc3339(&run_at_str)
                    .map_err(|e| {
                        CoreError::Storage(format!("invalid timestamp '{run_at_str}': {e}"))
                    })?
                    .with_timezone(&Utc);
                Ok(Some(ProjectLog {
                    id,
                    project_id,
                    command_name,
                    exit_code,
                    log_text,
                    run_at,
                }))
            }
            None => Ok(None),
        }
    }

    /// Get the most recent log entry for a project.
    pub fn get_latest_log(&self, project_id: &str) -> Result<Option<ProjectLog>, CoreError> {
        let conn = self.get_conn()?;

        let mut stmt = conn
            .prepare(
                "SELECT id, project_id, command_name, exit_code, log_text, run_at
                 FROM project_logs
                 WHERE project_id = ?1
                 ORDER BY run_at DESC
                 LIMIT 1",
            )
            .map_err(|e| CoreError::Storage(format!("prepare error: {e}")))?;

        let row = stmt
            .query_row(params![project_id], |row| {
                let id: String = row.get(0)?;
                let project_id: String = row.get(1)?;
                let command_name: String = row.get(2)?;
                let exit_code: Option<i32> = row.get(3)?;
                let log_text: String = row.get(4)?;
                let run_at_str: String = row.get(5)?;
                Ok((
                    id,
                    project_id,
                    command_name,
                    exit_code,
                    log_text,
                    run_at_str,
                ))
            })
            .optional()
            .map_err(|e| CoreError::Storage(format!("query error: {e}")))?;

        match row {
            Some((id, project_id, command_name, exit_code, log_text, run_at_str)) => {
                let run_at = chrono::DateTime::parse_from_rfc3339(&run_at_str)
                    .map_err(|e| {
                        CoreError::Storage(format!("invalid timestamp '{run_at_str}': {e}"))
                    })?
                    .with_timezone(&Utc);
                Ok(Some(ProjectLog {
                    id,
                    project_id,
                    command_name,
                    exit_code,
                    log_text,
                    run_at,
                }))
            }
            None => Ok(None),
        }
    }

    /// Prune old logs for a project, keeping only the `limit` most recent entries.
    pub fn prune_logs(&self, project_id: &str, limit: usize) -> Result<usize, CoreError> {
        let conn = self.get_conn()?;
        let count = conn
            .execute(
                "DELETE FROM project_logs WHERE id IN (SELECT id FROM project_logs WHERE project_id = ?1 ORDER BY run_at DESC LIMIT -1 OFFSET ?2)",
                params![project_id, limit],
            )
            .map_err(|e| CoreError::Storage(format!("delete error: {e}")))?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustodian_core::traits::ProjectStore;
    use rustodian_types::{Project, ProjectId};
    use std::path::PathBuf;

    #[test]
    fn test_list_logs_pagination_boundaries() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.migrate().unwrap();

        let proj = Project {
            id: ProjectId::new(),
            name: "test_proj".to_string(),
            path: PathBuf::from("/test"),
            discovered_at: chrono::Utc::now(),
            last_scanned_at: None,
            vcs: None,
            languages: vec![],
            metadata: rustodian_types::ProjectMetadata::default(),
        };
        store.save_project(&proj).unwrap();

        let log1 = ProjectLog {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: proj.id.to_string(),
            command_name: "test_cmd".to_string(),
            exit_code: Some(0),
            log_text: "log 1".to_string(),
            run_at: chrono::Utc::now(),
        };
        let log2 = ProjectLog {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: proj.id.to_string(),
            command_name: "test_cmd".to_string(),
            exit_code: Some(0),
            log_text: "log 2".to_string(),
            run_at: chrono::Utc::now(),
        };

        store.save_log(&log1).unwrap();
        store.save_log(&log2).unwrap();

        let logs_empty = store.list_logs(&proj.id.to_string(), 0).unwrap();
        assert!(logs_empty.is_empty());

        let logs_all = store.list_logs(&proj.id.to_string(), 10).unwrap();
        assert_eq!(logs_all.len(), 2);
    }
}

```

### Path: ./crates/rustodian-storage/src/remote_store.rs
```
use crate::store::SqliteStore;
use rusqlite::params;
use rustodian_core::error::CoreError;
use rustodian_core::traits::RemoteProjectStore;
use rustodian_types::RemoteProject;

impl RemoteProjectStore for SqliteStore {
    fn save_remote_project(&self, project: &RemoteProject) -> Result<(), CoreError> {
        let conn = self.get_conn()?;
        let patterns_json = serde_json::to_string(&project.preserve_patterns)
            .map_err(|e| CoreError::Storage(format!("failed to serialize patterns: {e}")))?;
        conn.execute(
            "INSERT INTO remote_projects (repo_slug, preserve_patterns)
             VALUES (?1, ?2)
             ON CONFLICT(repo_slug) DO UPDATE SET preserve_patterns = excluded.preserve_patterns",
            params![project.repo_slug, patterns_json],
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }
    fn list_remote_projects(&self) -> Result<Vec<RemoteProject>, CoreError> {
        let conn = self.get_conn()?;
        let mut stmt = conn
            .prepare("SELECT repo_slug, preserve_patterns FROM remote_projects")
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                let repo_slug: String = row.get(0)?;
                let patterns_json: String = row.get(1)?;
                let preserve_patterns = serde_json::from_str(&patterns_json).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?;
                Ok(RemoteProject {
                    repo_slug,
                    preserve_patterns,
                })
            })
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        let mut projects = Vec::new();
        for row in rows {
            projects.push(row.map_err(|e| CoreError::Storage(e.to_string()))?);
        }
        Ok(projects)
    }
    fn delete_remote_project(&self, repo_slug: &str) -> Result<bool, CoreError> {
        let conn = self.get_conn()?;
        let changes = conn
            .execute(
                "DELETE FROM remote_projects WHERE repo_slug = ?1",
                params![repo_slug],
            )
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(changes > 0)
    }
}

```

### Path: ./crates/rustodian-remote/src/error.rs
```
use thiserror::Error;
#[derive(Error, Debug)]
pub enum RemoteError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Archive extraction error: {0}")]
    Extraction(String),
    #[error("Not found: {0}")]
    NotFound(String),
}

```

### Path: ./crates/rustodian-remote/src/lib.rs
```
pub mod downloader;
pub mod error;
pub use downloader::GithubDownloader;

```

### Path: ./crates/rustodian-remote/src/downloader.rs
```
use std::fs;
use std::path::Path;

use flate2::read::GzDecoder;
use globset::{Glob, GlobSetBuilder};
use reqwest::Client;
use tar::Archive;
use tracing::{debug, info};

use rustodian_core::traits::RemoteDownloader;
use rustodian_types::RemoteProject;

#[derive(Clone)]
pub struct GithubDownloader {
    client: Client,
    api_base_url: String,
}

impl GithubDownloader {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_base_url: "https://api.github.com".to_string(),
        }
    }

    pub fn with_api_base_url(mut self, url: String) -> Self {
        self.api_base_url = url;
        self
    }
}

impl Default for GithubDownloader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl RemoteDownloader for GithubDownloader {
    async fn download_and_extract(
        &self,
        project: &RemoteProject,
        dest_dir: &Path,
        preserve_patterns: &[String],
    ) -> Result<(), rustodian_core::CoreError> {
        info!("Downloading project {}", project.repo_slug);

        let canonical_dest = dest_dir
            .canonicalize()
            .unwrap_or_else(|_| dest_dir.to_path_buf());
        let mut builder = GlobSetBuilder::new();
        for pat in preserve_patterns {
            if let Ok(glob) = Glob::new(pat) {
                builder.add(glob);
            }
        }
        let preserve_set = builder
            .build()
            .unwrap_or_else(|_| GlobSetBuilder::new().build().unwrap());

        // Try main then master
        let dl_base = if self.api_base_url == "https://api.github.com" {
            "https://github.com".to_string()
        } else {
            self.api_base_url.clone()
        };
        let mut response = self
            .client
            .get(format!(
                "{}/{}/archive/refs/heads/main.tar.gz",
                dl_base, project.repo_slug
            ))
            .send()
            .await
            .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            response = self
                .client
                .get(format!(
                    "{}/{}/archive/refs/heads/master.tar.gz",
                    dl_base, project.repo_slug
                ))
                .send()
                .await
                .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;
        }

        if !response.status().is_success() {
            return Err(rustodian_core::CoreError::Internal(format!(
                "Failed to download {}: status {}",
                project.repo_slug,
                response.status()
            )));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;

        let tar = GzDecoder::new(std::io::Cursor::new(bytes));
        let mut archive = Archive::new(tar);

        let entries = archive
            .entries()
            .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;

        for entry in entries {
            let mut entry =
                entry.map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;
            let path = entry
                .path()
                .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;

            let mut components = path.components();
            components.next();
            let stripped_path = components.as_path();

            if stripped_path.as_os_str().is_empty() {
                continue;
            }

            // Security Fix: Prevent Path Traversal (Zip Slip)
            // Ensure the path does not contain components that escape the intended directory
            if stripped_path.components().any(|c| {
                !matches!(
                    c,
                    std::path::Component::Normal(_) | std::path::Component::CurDir
                )
            }) {
                return Err(rustodian_core::CoreError::Internal(format!(
                    "Security violation: Path traversal detected in archive entry {:?}",
                    path
                )));
            }

            if preserve_set.is_match(stripped_path) {
                debug!("Preserving file matching pattern: {:?}", stripped_path);
                continue;
            }

            let dest_path = dest_dir.join(stripped_path);
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;

                // Security Fix: Prevent Zip Slip via symlinks
                let canonical_parent = parent
                    .canonicalize()
                    .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;

                if !canonical_parent.starts_with(&canonical_dest) {
                    return Err(rustodian_core::CoreError::Internal(format!(
                        "Security violation: Zip Slip path traversal detected in archive entry {:?}",
                        path
                    )));
                }
            }

            entry
                .unpack(&dest_path)
                .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;
        }

        info!(
            "Successfully downloaded and extracted {}",
            project.repo_slug
        );
        Ok(())
    }
}

#[async_trait::async_trait]
impl rustodian_core::traits::PullRequestFetcher for GithubDownloader {
    async fn fetch_open_prs(
        &self,
        repo_slug: &str,
    ) -> Result<Vec<rustodian_types::PullRequest>, rustodian_core::CoreError> {
        let url = format!("{}/repos/{}/pulls?state=open", self.api_base_url, repo_slug);

        let mut req = self
            .client
            .get(&url)
            .header(reqwest::header::USER_AGENT, "rustodian");

        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            req = req.bearer_auth(token);
        }

        let response = req
            .send()
            .await
            .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;

        if response.status() == reqwest::StatusCode::FORBIDDEN
            && let Some(limit) = response.headers().get("X-RateLimit-Remaining")
            && limit.to_str().unwrap_or("") == "0"
        {
            return Err(rustodian_core::CoreError::RateLimitExceeded);
        }

        if !response.status().is_success() {
            return Err(rustodian_core::CoreError::Internal(format!(
                "Failed to fetch PRs for {}: status {}",
                repo_slug,
                response.status()
            )));
        }

        #[derive(serde::Deserialize)]
        struct GithubPR {
            number: u64,
            title: String,
            user: GithubUser,
            head: GithubHead,
            html_url: String,
            updated_at: chrono::DateTime<chrono::Utc>,
            draft: bool,
        }

        #[derive(serde::Deserialize)]
        struct GithubUser {
            login: String,
        }

        #[derive(serde::Deserialize)]
        struct GithubHead {
            #[serde(rename = "ref")]
            ref_name: String,
        }

        let gh_prs: Vec<GithubPR> = response
            .json()
            .await
            .map_err(|e| rustodian_core::CoreError::Internal(e.to_string()))?;

        Ok(gh_prs
            .into_iter()
            .map(|pr| rustodian_types::PullRequest {
                number: pr.number,
                title: pr.title,
                author: pr.user.login,
                branch: pr.head.ref_name,
                url: pr.html_url,
                updated_at: pr.updated_at,
                is_draft: pr.draft,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;
    use rustodian_core::traits::PullRequestFetcher;

    #[tokio::test]
    async fn test_fetch_open_prs_success() {
        let mut server = Server::new_async().await;

        let m = server
            .mock("GET", "/repos/drawmeanelephant/rustodian/pulls?state=open")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"
            [
                {
                    "number": 42,
                    "title": "Add Pull Request fetching",
                    "user": { "login": "jules" },
                    "head": { "ref": "feature/pr-fetch" },
                    "html_url": "https://github.com/drawmeanelephant/rustodian/pull/42",
                    "updated_at": "2023-10-01T12:00:00Z",
                    "draft": false
                }
            ]
            "#,
            )
            .create_async()
            .await;

        let downloader = GithubDownloader::new().with_api_base_url(server.url());
        let prs = downloader
            .fetch_open_prs("drawmeanelephant/rustodian")
            .await
            .unwrap();

        assert_eq!(prs.len(), 1);
        assert_eq!(prs[0].number, 42);
        assert_eq!(prs[0].title, "Add Pull Request fetching");
        assert_eq!(prs[0].author, "jules");
        assert_eq!(prs[0].branch, "feature/pr-fetch");
        assert!(!prs[0].is_draft);

        m.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_open_prs_rate_limit() {
        let mut server = Server::new_async().await;

        let m = server
            .mock("GET", "/repos/drawmeanelephant/rustodian/pulls?state=open")
            .with_status(403)
            .with_header("X-RateLimit-Remaining", "0")
            .create_async()
            .await;

        let downloader = GithubDownloader::new().with_api_base_url(server.url());
        let err = downloader
            .fetch_open_prs("drawmeanelephant/rustodian")
            .await
            .unwrap_err();

        assert!(matches!(err, rustodian_core::CoreError::RateLimitExceeded));
        m.assert_async().await;
    }
}

#[tokio::test]
async fn test_download_and_extract_zip_slip_symlink() {
    let temp_dir = tempfile::tempdir().unwrap();
    let extract_dir = temp_dir.path().join("extract");
    std::fs::create_dir_all(&extract_dir).unwrap();

    // Target directory outside the extraction path (simulating a system dir)
    let system_dir = temp_dir.path().join("system");
    std::fs::create_dir_all(&system_dir).unwrap();

    // Create a malicious tarball in memory
    let mut tar_builder = tar::Builder::new(Vec::new());

    // 1. Add a directory (this is usually stripped as root dir)
    let mut header = tar::Header::new_gnu();
    header.set_size(0);
    header.set_entry_type(tar::EntryType::Directory);
    tar_builder
        .append_data(&mut header, "root/", &[][..])
        .unwrap();

    // 2. Add a symlink named 'foo' pointing to our system_dir
    let mut header = tar::Header::new_gnu();
    header.set_size(0);
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_link_name(system_dir.to_str().unwrap()).unwrap();
    tar_builder
        .append_data(&mut header, "root/foo", &[][..])
        .unwrap();

    // 3. Add a file 'bar' inside the symlinked directory 'foo'
    // If Zip Slip is possible, this will extract to system_dir/bar
    let mut header = tar::Header::new_gnu();
    header.set_size(12);
    header.set_entry_type(tar::EntryType::Regular);
    tar_builder
        .append_data(&mut header, "root/foo/bar", &b"pwned content"[..])
        .unwrap();

    let tar_data = tar_builder.into_inner().unwrap();

    // Gzip it
    use std::io::Write;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&tar_data).unwrap();
    let tar_gz_data = encoder.finish().unwrap();

    // Mock the server
    let mut server = mockito::Server::new_async().await;
    let _m = server
        .mock(
            "GET",
            "/drawmeanelephant/rustodian/archive/refs/heads/main.tar.gz",
        )
        .with_status(200)
        .with_body(tar_gz_data)
        .create_async()
        .await;

    let downloader = GithubDownloader::new().with_api_base_url(server.url());

    // Try to download and extract
    let project = rustodian_types::RemoteProject {
        repo_slug: "drawmeanelephant/rustodian".to_string(),
        preserve_patterns: vec![],
    };

    let result = downloader
        .download_and_extract(&project, &extract_dir, &[])
        .await;

    // Ensure it failed with a security error
    println!("Result: {:?}", result);
    assert!(
        result.is_err(),
        "Extraction should have failed due to Zip Slip protection"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Security violation")
            || err_msg.contains("Zip Slip")
            || err_msg.contains("already exists")
            || err_msg.contains("Cannot create a file")
            || err_msg.contains("os error 183")
    );

    // Ensure the file was NOT written to the system dir
    assert!(
        !system_dir.join("bar").exists(),
        "Zip slip attack succeeded!"
    );
}

```

### Path: ./crates/rustodian-scanner/src/error.rs
```
//! Scanner-specific error types.

use std::path::PathBuf;

use rustodian_core::CoreError;

/// Errors specific to filesystem scanning.
#[derive(Debug, thiserror::Error)]
pub enum ScannerError {
    /// IO error during filesystem traversal.
    #[error("io error at {}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    /// The scan root doesn't exist or isn't a directory.
    #[error("scan root is not a directory: {}", .0.display())]
    NotADirectory(PathBuf),
}

impl From<ScannerError> for CoreError {
    fn from(err: ScannerError) -> Self {
        CoreError::Scan(err.to_string())
    }
}

```

### Path: ./crates/rustodian-scanner/src/lib.rs
```
//! # Rustodian Scanner
//!
//! Filesystem-based project discovery for Rustodian.
//!
//! Uses the `ignore` crate for `.gitignore`-aware directory traversal.
//! Detects projects by looking for language-specific marker files
//! (e.g., `Cargo.toml` for Rust, `package.json` for Node).

pub mod commands;
pub mod detection;
pub mod error;
pub mod scanner;

pub use scanner::FsScanner;

```

### Path: ./crates/rustodian-scanner/src/detection.rs
```
//! Language detection from filesystem markers.
//!
//! Each language detector is a pure function that examines a project directory
//! and returns detection evidence. Adding a new language is as simple as
//! adding a new function and registering it in [`detect_languages`].

use std::path::Path;

use rustodian_types::{DetectionConfidence, Language, LanguageDetection, LanguageMarker};

/// Detect all languages present in a project directory.
///
/// Runs all registered language detectors and collects results.
pub fn detect_languages(project_path: &Path) -> Vec<LanguageDetection> {
    let mut detections = Vec::new();

    // Run each detector — order doesn't matter, they're independent
    if let Some(d) = detect_rust(project_path) {
        detections.push(d);
    }
    if let Some(d) = detect_python(project_path) {
        detections.push(d);
    }
    if let Some(d) = detect_node(project_path) {
        detections.push(d);
    }
    if let Some(d) = detect_go(project_path) {
        detections.push(d);
    }
    if let Some(d) = detect_ruby(project_path) {
        detections.push(d);
    }
    if let Some(d) = detect_zig(project_path) {
        detections.push(d);
    }

    detections
}

/// Detect Rust projects by looking for Cargo.toml.
fn detect_rust(path: &Path) -> Option<LanguageDetection> {
    let mut markers = Vec::new();

    if path.join("Cargo.toml").exists() {
        markers.push(LanguageMarker::ManifestFile("Cargo.toml".to_string()));
    }
    if path.join("Cargo.lock").exists() {
        markers.push(LanguageMarker::LockFile("Cargo.lock".to_string()));
    }

    if markers.is_empty() {
        return None;
    }

    let confidence = if markers
        .iter()
        .any(|m| matches!(m, LanguageMarker::ManifestFile(_)))
    {
        DetectionConfidence::High
    } else {
        DetectionConfidence::Medium
    };

    Some(LanguageDetection {
        language: Language::Rust,
        confidence,
        markers,
    })
}

/// Detect Python projects.
fn detect_python(path: &Path) -> Option<LanguageDetection> {
    let mut markers = Vec::new();

    for manifest in &["pyproject.toml", "setup.py", "setup.cfg"] {
        if path.join(manifest).exists() {
            markers.push(LanguageMarker::ManifestFile((*manifest).to_string()));
        }
    }
    for lock in &["poetry.lock", "Pipfile.lock", "uv.lock"] {
        if path.join(lock).exists() {
            markers.push(LanguageMarker::LockFile((*lock).to_string()));
        }
    }
    if path.join("requirements.txt").exists() {
        markers.push(LanguageMarker::ConfigFile("requirements.txt".to_string()));
    }

    if markers.is_empty() {
        return None;
    }

    let confidence = if markers
        .iter()
        .any(|m| matches!(m, LanguageMarker::ManifestFile(_)))
    {
        DetectionConfidence::High
    } else {
        DetectionConfidence::Medium
    };

    Some(LanguageDetection {
        language: Language::Python,
        confidence,
        markers,
    })
}

/// Detect Node.js projects.
fn detect_node(path: &Path) -> Option<LanguageDetection> {
    let mut markers = Vec::new();

    if path.join("package.json").exists() {
        markers.push(LanguageMarker::ManifestFile("package.json".to_string()));
    }
    for lock in &[
        "package-lock.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "bun.lockb",
    ] {
        if path.join(lock).exists() {
            markers.push(LanguageMarker::LockFile((*lock).to_string()));
        }
    }

    if markers.is_empty() {
        return None;
    }

    Some(LanguageDetection {
        language: Language::Node,
        confidence: DetectionConfidence::High,
        markers,
    })
}

/// Detect Go projects.
fn detect_go(path: &Path) -> Option<LanguageDetection> {
    let mut markers = Vec::new();

    if path.join("go.mod").exists() {
        markers.push(LanguageMarker::ManifestFile("go.mod".to_string()));
    }
    if path.join("go.sum").exists() {
        markers.push(LanguageMarker::LockFile("go.sum".to_string()));
    }

    if markers.is_empty() {
        return None;
    }

    Some(LanguageDetection {
        language: Language::Go,
        confidence: DetectionConfidence::High,
        markers,
    })
}

/// Detect Ruby projects.
fn detect_ruby(path: &Path) -> Option<LanguageDetection> {
    let mut markers = Vec::new();

    if path.join("Gemfile").exists() {
        markers.push(LanguageMarker::ManifestFile("Gemfile".to_string()));
    }

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Some(name) = entry
                .file_name()
                .to_str()
                .filter(|n| n.ends_with(".gemspec"))
            {
                markers.push(LanguageMarker::ManifestFile(name.to_string()));
            }
        }
    }

    if path.join("Gemfile.lock").exists() {
        markers.push(LanguageMarker::LockFile("Gemfile.lock".to_string()));
    }

    if markers.is_empty() {
        return None;
    }

    let confidence = if markers
        .iter()
        .any(|m| matches!(m, LanguageMarker::ManifestFile(_)))
    {
        DetectionConfidence::High
    } else {
        DetectionConfidence::Medium
    };

    Some(LanguageDetection {
        language: Language::Ruby,
        confidence,
        markers,
    })
}

/// Detect Zig projects.
fn detect_zig(path: &Path) -> Option<LanguageDetection> {
    let mut markers = Vec::new();

    if path.join("build.zig").exists() {
        markers.push(LanguageMarker::ManifestFile("build.zig".to_string()));
    }

    if path.join("build.zig.zon").exists() {
        markers.push(LanguageMarker::LockFile("build.zig.zon".to_string()));
    }

    if markers.is_empty() {
        return None;
    }

    let confidence = if markers
        .iter()
        .any(|m| matches!(m, LanguageMarker::ManifestFile(_)))
    {
        DetectionConfidence::High
    } else {
        DetectionConfidence::Medium
    };

    Some(LanguageDetection {
        language: Language::Zig,
        confidence,
        markers,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_detect_rust_project() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        fs::write(dir.path().join("Cargo.lock"), "").unwrap();

        let detections = detect_languages(dir.path());
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].language, Language::Rust);
        assert_eq!(detections[0].confidence, DetectionConfidence::High);
        assert_eq!(detections[0].markers.len(), 2);
    }

    #[test]
    fn test_detect_python_project() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("pyproject.toml"), "").unwrap();

        let detections = detect_languages(dir.path());
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].language, Language::Python);
    }

    #[test]
    fn test_detect_node_project() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();

        let detections = detect_languages(dir.path());
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].language, Language::Node);
    }

    #[test]
    fn test_detect_go_project() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("go.mod"), "module example").unwrap();

        let detections = detect_languages(dir.path());
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].language, Language::Go);
    }

    #[test]
    fn test_detect_ruby_project() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Gemfile"), "source 'https://rubygems.org'").unwrap();

        let detections = detect_languages(dir.path());
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].language, Language::Ruby);
    }

    #[test]
    fn test_detect_multi_language() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();

        let detections = detect_languages(dir.path());
        assert_eq!(detections.len(), 2);
    }

    #[test]
    fn test_detect_zig_project() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("build.zig"), "").unwrap();

        let detections = detect_languages(dir.path());
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].language, Language::Zig);
    }

    #[test]
    fn test_detect_empty_directory() {
        let dir = TempDir::new().unwrap();
        let detections = detect_languages(dir.path());
        assert!(detections.is_empty());
    }
}

```

### Path: ./crates/rustodian-scanner/src/scanner.rs
```
//! Filesystem scanner implementation.

use std::path::Path;

use tracing::{debug, instrument};

use rustodian_core::CoreError;
use rustodian_core::traits::{DiscoveredProject, ProjectScanner};
use rustodian_types::ScanConfig;

/// Filesystem-based project scanner.
///
/// Walks directory trees using the `ignore` crate (respects `.gitignore`)
/// and detects software projects by looking for marker files.
#[derive(Debug, Default)]
pub struct FsScanner;

impl ProjectScanner for FsScanner {
    #[instrument(skip(self), fields(root = %root.display()))]
    fn scan(&self, root: &Path, config: &ScanConfig) -> Result<Vec<DiscoveredProject>, CoreError> {
        debug!(max_depth = config.max_depth, "Starting filesystem scan");

        if config.max_depth == 0 {
            tracing::warn!(
                "ScanConfig::max_depth is 0. Returning empty results as this is treated as 'no traversal'."
            );
            return Ok(vec![]);
        }

        let mut builder = ignore::WalkBuilder::new(root);
        builder.max_depth(Some(config.max_depth));
        builder.follow_links(config.follow_symlinks);

        // Apply user-specified exclude patterns using globset.
        if !config.exclude_patterns.is_empty() {
            let mut gsb = globset::GlobSetBuilder::new();
            for pat in &config.exclude_patterns {
                if let Ok(glob) = globset::Glob::new(pat) {
                    gsb.add(glob);
                } else {
                    tracing::warn!("Invalid exclude pattern '{pat}'");
                }
            }
            if let Ok(excl) = gsb.build() {
                builder.filter_entry(move |e| !excl.is_match(e.path()));
            } else {
                tracing::warn!("Failed to build exclude globset");
            }
        }

        // Use parallel walking for better performance on large trees.
        builder.threads(0); // auto-detect CPU count

        let projects: std::sync::Arc<std::sync::Mutex<Vec<DiscoveredProject>>> =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let project_roots: std::sync::Arc<
            std::sync::Mutex<std::collections::HashSet<std::path::PathBuf>>,
        > = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));

        let walker = builder.build_parallel();
        walker.run(|| {
            let projects = std::sync::Arc::clone(&projects);
            let project_roots = std::sync::Arc::clone(&project_roots);
            Box::new(move |result| {
                let entry = match result {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::warn!("Error reading directory entry: {e}");
                        return ignore::WalkState::Continue;
                    }
                };

                let path = entry.path();
                if !path.is_dir() {
                    return ignore::WalkState::Continue;
                }

                // Skip if this directory is a child of an already-discovered
                // project root. This prevents detecting nested sub-projects
                // (e.g. a workspace member inside a Cargo workspace root).
                {
                    let roots = project_roots
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    for root in roots.iter() {
                        if path.starts_with(root) && path != root {
                            return ignore::WalkState::Skip;
                        }
                    }
                }

                let languages = crate::detection::detect_languages(path);
                if !languages.is_empty() {
                    let name = path
                        .file_name()
                        .unwrap_or_else(|| std::ffi::OsStr::new("unknown"))
                        .to_string_lossy()
                        .to_string();

                    let commands = crate::commands::CommandDiscoverer::discover(path);

                    if let Ok(mut projs) = projects.lock() {
                        projs.push(DiscoveredProject {
                            name,
                            path: path.to_path_buf(),
                            languages,
                            commands,
                        });
                    }

                    // Record this as a project root so children are skipped.
                    if let Ok(mut roots) = project_roots.lock() {
                        roots.insert(path.to_path_buf());
                    }

                    // Skip descending into this directory's children.
                    return ignore::WalkState::Skip;
                }

                ignore::WalkState::Continue
            })
        });

        let mut projects = match std::sync::Arc::try_unwrap(projects) {
            Ok(mutex) => mutex
                .into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            Err(arc) => arc
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone(),
        };

        // Sort by path for deterministic output regardless of walk order.
        projects.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(projects)
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_scanner_symlink_loop() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let a = root.join("a");
        let b = root.join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&b, a.join("link_to_b")).unwrap();
            std::os::unix::fs::symlink(&a, b.join("link_to_a")).unwrap();
        }

        File::create(a.join("Cargo.toml")).unwrap();

        let scanner = FsScanner;
        let config = ScanConfig {
            max_depth: 5,
            follow_symlinks: true,
            exclude_patterns: vec![],
        };

        let projs = scanner.scan(root, &config).unwrap();
        assert!(!projs.is_empty());
    }

    #[test]
    fn test_scanner_no_read_permissions() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let proj = root.join("my_proj");
        fs::create_dir_all(&proj).unwrap();
        File::create(proj.join("Cargo.toml")).unwrap();

        let unreadable = root.join("unreadable");
        fs::create_dir_all(&unreadable).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();
        }

        let scanner = FsScanner;
        let config = ScanConfig {
            max_depth: 3,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };
        let projs = scanner.scan(root, &config).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o755)).unwrap();
        }

        assert_eq!(projs.len(), 1);
        assert_eq!(projs[0].name, "my_proj");
    }

    #[test]
    fn test_scanner_malformed_manifest() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let proj = root.join("multi_proj");
        fs::create_dir_all(&proj).unwrap();
        File::create(proj.join("Cargo.toml")).unwrap();
        File::create(proj.join("package.json")).unwrap();

        let scanner = FsScanner;
        let config = ScanConfig {
            max_depth: 3,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };
        let projs = scanner.scan(root, &config).unwrap();

        assert_eq!(projs.len(), 1);
        assert_eq!(projs[0].name, "multi_proj");
        assert_eq!(projs[0].languages.len(), 2);
    }
    use super::*;
    use std::fs::{self, File};
    use tempfile::tempdir;

    #[test]
    fn test_scanner_basic_and_exclusions() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create project A (Rust project)
        let proj_a = root.join("project_a");
        fs::create_dir_all(&proj_a).unwrap();
        File::create(proj_a.join("Cargo.toml")).unwrap();

        // Create project B (Python project)
        let proj_b = root.join("project_b");
        fs::create_dir_all(&proj_b).unwrap();
        File::create(proj_b.join("requirements.txt")).unwrap();

        // Create excluded folder
        let excl_dir = root.join("excluded_folder");
        fs::create_dir_all(&excl_dir).unwrap();
        File::create(excl_dir.join("Cargo.toml")).unwrap();

        let scanner = FsScanner;

        // Scan without exclusions
        let config_no_excl = ScanConfig {
            max_depth: 3,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };
        let projs = scanner.scan(root, &config_no_excl).unwrap();
        assert_eq!(projs.len(), 3);

        // Scan with exclusions
        let config_excl = ScanConfig {
            max_depth: 3,
            follow_symlinks: false,
            exclude_patterns: vec!["**/excluded_folder".to_string()],
        };
        let projs_excl = scanner.scan(root, &config_excl).unwrap();
        assert_eq!(projs_excl.len(), 2);
        assert_eq!(projs_excl[0].name, "project_a");
        assert_eq!(projs_excl[1].name, "project_b");
    }

    #[test]
    fn test_scanner_nested_skipping() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create parent project (Rust project)
        let parent_proj = root.join("parent_proj");
        fs::create_dir_all(&parent_proj).unwrap();
        File::create(parent_proj.join("Cargo.toml")).unwrap();

        // Create nested project inside parent (Node project)
        let nested_proj = parent_proj.join("nested_node_proj");
        fs::create_dir_all(&nested_proj).unwrap();
        File::create(nested_proj.join("package.json")).unwrap();

        let scanner = FsScanner;
        let config = ScanConfig {
            max_depth: 5,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };
        let projs = scanner.scan(root, &config).unwrap();

        // It should only find "parent_proj" and skip descending into "nested_node_proj"
        assert_eq!(projs.len(), 1);
        assert_eq!(projs[0].name, "parent_proj");
    }
}

```

### Path: ./crates/rustodian-scanner/src/commands.rs
```
use std::fs;
use std::path::Path;

use rustodian_types::ProjectCommand;

pub struct CommandDiscoverer;

impl CommandDiscoverer {
    pub fn discover(root: &Path) -> Vec<ProjectCommand> {
        fn needs_shell(cmd: &str) -> bool {
            cmd.contains("&&")
                || cmd.contains("||")
                || cmd.contains('|')
                || cmd.contains('>')
                || cmd.contains('<')
                || cmd.contains("$(")
        }

        let mut commands = Vec::new();

        // 1. Rustodian config (.rustodian.toml)
        let toml_content = fs::read_to_string(root.join(".rustodian.toml"));
        let toml_config = toml_content
            .ok()
            .and_then(|c| toml::from_str::<toml::Value>(&c).ok());
        if let Some(commands_table) = toml_config
            .as_ref()
            .and_then(|config| config.get("commands"))
            .and_then(|c| c.as_table())
        {
            for (name, cmd) in commands_table {
                if let Some(cmd_str) = cmd.as_str() {
                    commands.push(ProjectCommand {
                        name: name.clone(),
                        description: Some("rustodian config".to_string()),
                        command: cmd_str.to_string(),
                        source: ".rustodian.toml".to_string(),
                        use_shell: needs_shell(cmd_str),
                    });
                }
            }
        }

        // 2. Rust standard commands if Cargo.toml exists
        if root.join("Cargo.toml").exists() {
            commands.extend(Self::rust_defaults());
        }

        // 3. Node.js scripts if package.json exists
        let pkg_content = fs::read_to_string(root.join("package.json"));
        let pkg_json = pkg_content
            .ok()
            .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok());
        if let Some(scripts) = pkg_json
            .as_ref()
            .and_then(|json| json.get("scripts"))
            .and_then(|s| s.as_object())
        {
            for (name, _) in scripts {
                commands.push(ProjectCommand {
                    name: name.clone(),
                    description: Some("npm run script".to_string()),
                    command: format!("npm run {name}"),
                    source: "package.json".to_string(),
                    use_shell: needs_shell(name),
                });
            }
        }

        // 3. Justfile recipes
        let justfile_paths = [root.join("justfile"), root.join("Justfile")];
        for path in justfile_paths {
            if let Ok(content) = fs::read_to_string(&path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty()
                        || trimmed.starts_with('#')
                        || line.starts_with(' ')
                        || line.starts_with('\t')
                    {
                        continue;
                    }
                    if let Some(idx) = trimmed.find(':') {
                        let recipe_def = &trimmed[..idx];
                        if let Some(n) = recipe_def.split_whitespace().next().filter(|n| {
                            !n.is_empty()
                                && n.chars()
                                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                        }) {
                            commands.push(ProjectCommand {
                                name: n.to_string(),
                                description: Some("just recipe".to_string()),
                                command: format!("just {n}"),
                                source: "justfile".to_string(),
                                use_shell: needs_shell(n),
                            });
                        }
                    }
                }
                break; // stop after first found justfile
            }
        }

        commands
    }

    fn rust_defaults() -> Vec<ProjectCommand> {
        vec![
            ProjectCommand {
                name: "test".to_string(),
                description: Some("Run cargo test".to_string()),
                command: "cargo test".to_string(),
                source: "Cargo.toml".to_string(),
                use_shell: false,
            },
            ProjectCommand {
                name: "build".to_string(),
                description: Some("Run cargo build".to_string()),
                command: "cargo build".to_string(),
                source: "Cargo.toml".to_string(),
                use_shell: false,
            },
            ProjectCommand {
                name: "check".to_string(),
                description: Some("Run cargo check".to_string()),
                command: "cargo check".to_string(),
                source: "Cargo.toml".to_string(),
                use_shell: false,
            },
            ProjectCommand {
                name: "clippy".to_string(),
                description: Some("Run cargo clippy".to_string()),
                command: "cargo clippy".to_string(),
                source: "Cargo.toml".to_string(),
                use_shell: false,
            },
            ProjectCommand {
                name: "fmt".to_string(),
                description: Some("Run cargo fmt".to_string()),
                command: "cargo fmt".to_string(),
                source: "Cargo.toml".to_string(),
                use_shell: false,
            },
        ]
    }
}

```

### Path: ./fix_test.sh
```
sed -i 's/conn.execute("UPDATE projects SET metadata_json = '"'not_json'"' WHERE id = ?1", rusqlite::params!\[id.to_string()\]).unwrap();/conn.execute("UPDATE projects SET metadata_json = '"'not_json'"' WHERE id = ?1", rusqlite::params!\[id.to_string()\]).unwrap(); drop(conn);/' crates/rustodian-storage/src/store.rs

```

