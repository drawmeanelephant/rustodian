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
