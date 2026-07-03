#![allow(clippy::collapsible_if)]

use anyhow::{Context as AnyhowContext, Result};
use rustodian_storage::SqliteStore;
use std::sync::Arc;

mod markdown;
mod message;
mod worker;

use message::{GuiMessage, WorkerMessage};

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let main_window = PipelineWindow::new()?;

    let store = match setup_db() {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("Failed to setup DB: {e}");
            return Ok(());
        }
    };

    let (gui_tx, worker_rx) = std::sync::mpsc::channel();
    let (worker_tx, gui_rx) = std::sync::mpsc::channel();

    let window_weak = main_window.as_weak();
    let repaint_fn: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(move || {
        let _ = window_weak.upgrade_in_event_loop(|_w| {
            // Wake up event loop
        });
    });

    let store_clone = store.clone();
    std::thread::spawn(move || {
        worker::run_worker(store_clone, &worker_rx, &worker_tx, &repaint_fn);
    });

    let gui_tx_clone1 = gui_tx.clone();
    main_window.on_trigger_ingest(move |slug| {
        let path = std::path::PathBuf::from(slug.to_string());
        let _ = gui_tx_clone1.send(GuiMessage::ScanProjects { path });
    });

    let gui_tx_clone2 = gui_tx.clone();
    main_window.on_trigger_agent_export(move |_target| {
        let _ = gui_tx_clone2.send(GuiMessage::LoadProjects);
    });

    let window_weak_for_timer = main_window.as_weak();
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(50),
        move || {
            if let Some(window) = window_weak_for_timer.upgrade() {
                while let Ok(msg) = gui_rx.try_recv() {
                    match msg {
                        WorkerMessage::CommandStatus {
                            log_buffer,
                            is_running,
                            ..
                        } => {
                            window
                                .set_stream_logs(slint::SharedString::from(log_buffer.snapshot()));
                            window.set_working(is_running);
                        }
                        WorkerMessage::ProjectsLoaded(_) | WorkerMessage::ScanComplete(_) => {
                            window.set_working(false);
                        }
                        _ => {}
                    }
                }
            }
        },
    );

    main_window.run()
}

fn setup_db() -> Result<SqliteStore> {
    let db_path = SqliteStore::default_path().context("failed to determine database path")?;
    let store = SqliteStore::open(&db_path).context("failed to open database")?;
    store.migrate().context("failed to run migrations")?;
    Ok(store)
}
