//! Message passing types for the Desktop GUI.

use std::path::PathBuf;
use std::time::SystemTime;

use rustodian_core::log_buffer::LogBuffer;
use rustodian_types::{Project, ProjectId};

/// Messages sent from the GUI thread to the Background Worker thread.
pub enum GuiMessage {
    /// Load all projects from the database.
    LoadProjects,
    /// Run a command for a project.
    RunCommand {
        project_id: ProjectId,
        project_path: PathBuf,
        command_name: String,
        command_str: String,
        use_shell: bool,
    },
    /// Kill the currently running command (if any).
    /// Request to scan projects.
    ScanProjects {
        path: PathBuf,
    },

    KillCommand,
    /// Discover documentation files in a project root.
    DiscoverDocs {
        project_path: PathBuf,
    },
    /// Check if a specific document file is fresh.
    CheckDocFreshness {
        path: PathBuf,
        known_mtime: Option<SystemTime>,
    },
    /// Load the content of a specific document file.
    /// Save a setting to the database.
    SaveSetting {
        key: String,
        value: String,
    },

    LoadDocContent {
        path: PathBuf,
        known_hash: Option<u64>,
    },
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
    /// Result of loading projects.
    /// Result of scanning projects.
    ScanComplete(Result<rustodian_core::custodian::ScanReport, String>),

    ProjectsLoaded(Result<Vec<Project>, String>),

    /// Status update for a running command.
    CommandStatus {
        command_name: String,
        is_running: bool,
        exit_status: Option<String>,
        log_buffer: LogBuffer,
    },

    /// Result of discovering documentation files.
    DocsDiscovered {
        project_path: PathBuf,
        available_docs: Vec<(String, PathBuf)>,
    },

    DocStale {
        #[allow(dead_code)]
        path: PathBuf,
    },
    DocFresh {
        #[allow(dead_code)]
        path: PathBuf,
    },

    /// Result of loading and parsing a document.
    DocLoaded {
        content: String,
        parsed: ParsedMarkdown,
        last_modified: Option<SystemTime>,
        content_hash: u64,
    },

    /// Result when content has not changed.
    DocUnchanged,
}
