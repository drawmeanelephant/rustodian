//! Message passing types for the Desktop GUI.

/// Messages sent from the GUI thread to the Background Worker thread.
pub enum GuiMessage {
    /// Trigger an ingest operation.
    TriggerIngest {
        repo_slug: String,
        target_project: String,
    },
    /// Trigger an agent export.
    TriggerAgentExport { target_project: String },
    /// Request a markdown file payload.
    LoadDocContent { path: String },
    /// Update a specific task markdown line checkbox state.
    ToggleTask { task_id: String, completed: bool },
    /// Signal clean exit.
    Shutdown,
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

/// Messages sent from the Background Worker thread to the GUI thread.
pub enum WorkerMessage {
    /// Streams incremental chunked log lines back to the UI.
    CommandStatus { status: String, log: Option<String> },
    /// Signals end of an ingest or scan run.
    ScanComplete { success: bool, message: String },
    /// Notifies UI of projects available in the store.
    ProjectsLoaded(Vec<String>),
    /// Returns structural parsed markdown blocks.
    DocLoaded {
        path: String,
        blocks: Vec<MarkdownBlock>,
    },
}
