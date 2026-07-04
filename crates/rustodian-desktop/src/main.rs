pub mod markdown;
pub mod message;
pub mod worker;

pub mod ui_mapping {
    use rustodian_types::{Project, ProjectCommand};
    use slint::{ModelRc, SharedString, VecModel};

    slint::slint! {
        export { PipelineWindow } from "ui/pipeline.slint";

        export enum BlockType { Header, CodeFence, HorizontalRule, Task, BulletList, NumberedList, Text, BlankLine }

        export struct SlintMarkdownBlock {
            block_type: BlockType,
            text: string,
            is_checked: bool,
            level: int,
            number: string,
        }

        export struct SlintProject {
            id: string,
            name: string,
            path: string,
            discovered_at: string,
        }

        export struct SlintProjectCommand {
            name: string,
            description: string,
            command: string,
            source: string,
            use_shell: bool,
        }


    }

    pub fn map_project(project: &Project) -> SlintProject {
        SlintProject {
            id: SharedString::from(project.id.to_string()),
            name: SharedString::from(project.name.clone()),
            path: SharedString::from(project.path.to_string_lossy().into_owned()),
            discovered_at: SharedString::from(project.discovered_at.to_rfc3339()),
        }
    }

    pub fn map_project_command(command: &ProjectCommand) -> SlintProjectCommand {
        SlintProjectCommand {
            name: SharedString::from(command.name.clone()),
            description: SharedString::from(command.description.clone().unwrap_or_default()),
            command: SharedString::from(command.command.clone()),
            source: SharedString::from(command.source.clone()),
            use_shell: command.use_shell,
        }
    }

    pub fn map_projects(projects: &[Project]) -> ModelRc<SlintProject> {
        let slint_projects: Vec<SlintProject> = projects.iter().map(map_project).collect();
        ModelRc::new(VecModel::from(slint_projects))
    }

    pub fn map_commands(commands: &[ProjectCommand]) -> ModelRc<SlintProjectCommand> {
        let slint_commands: Vec<SlintProjectCommand> =
            commands.iter().map(map_project_command).collect();
        ModelRc::new(VecModel::from(slint_commands))
    }
}

use message::{GuiMessage, MarkdownBlock, WorkerMessage};
use rustodian_storage::SqliteStore;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::sync::{Arc, mpsc};
use ui_mapping::{BlockType, PipelineWindow, SlintMarkdownBlock};

#[allow(clippy::too_many_lines)]
fn main() -> Result<(), slint::PlatformError> {
    let ui = PipelineWindow::new()?;
    let ui_weak = ui.as_weak();

    let store = Arc::new(SqliteStore::open(&SqliteStore::default_path().unwrap()).unwrap());

    let (tx_gui, rx_gui) = mpsc::channel();
    let (tx_worker, rx_worker) = mpsc::channel();

    // Slint invoke mapping to safely trigger UI repaints from the background thread
    let repaint_fn: Arc<dyn Fn() + Send + Sync> = Arc::new({
        let ui_weak = ui_weak.clone();
        move || {
            let _ = slint::invoke_from_event_loop({
                let ui_weak = ui_weak.clone();
                move || {
                    if let Some(_ui) = ui_weak.upgrade() {
                        // Empty invoke just to trigger event loop wakeup
                    }
                }
            });
        }
    });

    // Spin up the worker thread
    let worker_handle = {
        let store = store.clone();
        let repaint_fn_clone = repaint_fn.clone();
        std::thread::spawn(move || {
            worker::run_worker(store, &rx_gui, &tx_worker, &repaint_fn_clone);
        })
    };

    // 2. Map Slint kebab-case properties and events to their Rust snake_case equivalents securely.
    // When the Slint UI fires on_trigger_ingest or on_trigger_agent_export,
    // extract the data cleanly as a standard String payload from slint::SharedString allocations,
    // immediately toggle ui.set_working(true) on the main thread, and dispatch the message variant to the background worker.

    ui.on_trigger_ingest({
        let tx = tx_gui.clone();
        let ui_weak = ui_weak.clone();
        move |repo_slug: SharedString| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_working(true);
                let slug = repo_slug.to_string();
                let _ = tx.send(GuiMessage::Ingest(slug));
            }
        }
    });

    ui.on_trigger_agent_export({
        let tx = tx_gui.clone();
        let ui_weak = ui_weak.clone();
        move |target_project: SharedString| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_working(true);
                let target = target_project.to_string();
                let _ = tx.send(GuiMessage::AgentExport(target));
            }
        }
    });

    // Spin up an asynchronous channel listener thread.
    // Use slint::invoke_from_event_loop to safely append incoming stream log chunks to our stream-logs property.
    // Implement a line rotation cap inside the UI loop to truncate old lines if the buffer expands past 2000 lines to mitigate UI stutter.
    let ui_weak_for_thread = ui.as_weak();
    std::thread::spawn(move || {
        while let Ok(msg) = rx_worker.recv() {
            match msg {
                WorkerMessage::StreamLogChunk(line) => {
                    let ui_weak_clone = ui_weak_for_thread.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = ui_weak_clone.upgrade() {
                            let current_logs = ui.get_stream_logs().to_string();
                            let mut lines: Vec<&str> = current_logs.lines().collect();
                            lines.push(&line);
                            if lines.len() > 2000 {
                                lines.remove(0);
                            }
                            ui.set_stream_logs(SharedString::from(lines.join("\n")));
                        }
                    });
                }
                WorkerMessage::DocLoaded { parsed, .. } => {
                    let ui_weak_clone = ui_weak_for_thread.clone();
                    let blocks: Vec<SlintMarkdownBlock> = parsed
                        .blocks
                        .into_iter()
                        .map(|b| match b {
                            MarkdownBlock::Header { level, text } => SlintMarkdownBlock {
                                block_type: BlockType::Header,
                                text: SharedString::from(text),
                                level: level.try_into().unwrap_or(0),
                                is_checked: false,
                                number: SharedString::default(),
                            },
                            MarkdownBlock::CodeFence { text } => SlintMarkdownBlock {
                                block_type: BlockType::CodeFence,
                                text: SharedString::from(text),
                                level: 0,
                                is_checked: false,
                                number: SharedString::default(),
                            },
                            MarkdownBlock::HorizontalRule => SlintMarkdownBlock {
                                block_type: BlockType::HorizontalRule,
                                text: SharedString::default(),
                                level: 0,
                                is_checked: false,
                                number: SharedString::default(),
                            },
                            MarkdownBlock::Task { text, checked } => SlintMarkdownBlock {
                                block_type: BlockType::Task,
                                text: SharedString::from(text),
                                level: 0,
                                is_checked: checked,
                                number: SharedString::default(),
                            },
                            MarkdownBlock::BulletList { text } => SlintMarkdownBlock {
                                block_type: BlockType::BulletList,
                                text: SharedString::from(text),
                                level: 0,
                                is_checked: false,
                                number: SharedString::default(),
                            },
                            MarkdownBlock::NumberedList { number, text } => SlintMarkdownBlock {
                                block_type: BlockType::NumberedList,
                                text: SharedString::from(text),
                                level: 0,
                                is_checked: false,
                                number: SharedString::from(number),
                            },
                            MarkdownBlock::Text { text } => SlintMarkdownBlock {
                                block_type: BlockType::Text,
                                text: SharedString::from(text),
                                level: 0,
                                is_checked: false,
                                number: SharedString::default(),
                            },
                            MarkdownBlock::BlankLine => SlintMarkdownBlock {
                                block_type: BlockType::BlankLine,
                                text: SharedString::default(),
                                level: 0,
                                is_checked: false,
                                number: SharedString::default(),
                            },
                        })
                        .collect();

                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = ui_weak_clone.upgrade() {
                            ui.set_doc_blocks(ModelRc::new(VecModel::from(blocks)));
                        }
                    });
                }
                _ => {}
            }
        }
    });

    ui.on_toggle_task({
        let tx = tx_gui.clone();
        let ui_weak = ui_weak.clone();
        move |index, checked| {
            if let Some(ui) = ui_weak.upgrade() {
                let blocks_model = ui.get_doc_blocks();
                if let Some(mut block) = blocks_model.row_data(index.try_into().unwrap_or(0)) {
                    block.is_checked = checked;
                    blocks_model.set_row_data(index.try_into().unwrap_or(0), block);
                    let _ = tx.send(GuiMessage::ToggleTask(index, checked));
                }
            }
        }
    });

    // Run the UI
    ui.run()?;

    // Catch window drops to execute clean shutdown
    let _ = tx_gui.send(GuiMessage::Shutdown);
    worker_handle.join().expect("Worker thread panicked");

    Ok(())
}
