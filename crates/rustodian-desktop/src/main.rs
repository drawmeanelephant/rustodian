#![allow(clippy::collapsible_if)]

use anyhow::{Context, Result};
use rustodian_storage::SqliteStore;
use std::sync::Arc;

mod markdown;
mod message;
mod worker;

use message::GuiMessage;

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let store = match setup_db() {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("Failed to setup DB: {e}");
            return Ok(());
        }
    };

    let (gui_tx, worker_rx) = std::sync::mpsc::channel();
    let (worker_tx, _gui_rx) = std::sync::mpsc::channel();

    let store_clone = store.clone();
    std::thread::spawn(move || {
        worker::run_worker(store_clone, &worker_rx, &worker_tx);
    });

    let window = PipelineWindow::new()?;
    let window_weak = window.as_weak();

    let gui_tx_clone = gui_tx.clone();
    window.on_trigger_ingest(move |repo_slug| {
        let repo_slug_str = repo_slug.to_string();

        // As per prompt: send a GuiMessage::ScanProjects or add remote project message
        // down the existing worker_tx channel.
        let path = std::path::PathBuf::from(&repo_slug_str);

        let _ = gui_tx_clone.send(GuiMessage::ScanProjects { path });

        // Set the window's repo_slug text variable to update reactively
        if let Some(window) = window_weak.upgrade() {
            window.set_repo_slug(repo_slug.clone());
        }
    });

    window.run()
}

fn setup_db() -> Result<SqliteStore> {
    let db_path = SqliteStore::default_path().context("failed to determine database path")?;
    let store = SqliteStore::open(&db_path).context("failed to open database")?;
    store.migrate().context("failed to run migrations")?;
    Ok(store)
}
