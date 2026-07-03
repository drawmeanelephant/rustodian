#![allow(clippy::collapsible_if)]
slint::include_modules!();

mod message;
use message::WorkerMessage;

fn main() {
    println!("Migrating to Slint...");
}

pub fn bind_pipeline_worker_stream(
    ui_handle: slint::Weak<PipelineWindow>,
    rx: std::sync::mpsc::Receiver<WorkerMessage>,
) {
    std::thread::spawn(move || {
        while let Ok(msg) = rx.recv() {
            if let WorkerMessage::CommandStatus {
                is_running,
                log_buffer,
                ..
            } = msg
            {
                let snapshot = log_buffer.snapshot();
                let ui_handle_clone = ui_handle.clone();

                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_handle_clone.upgrade() {
                        ui.set_stream_logs(snapshot.into());
                        ui.set_working(is_running);
                    }
                });
            }
        }
    });
}
