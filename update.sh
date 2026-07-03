cat crates/rustodian-desktop/src/main.rs | head -n 840 > tmp.rs
cat << 'INNER_EOF' >> tmp.rs
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
INNER_EOF
mv tmp.rs crates/rustodian-desktop/src/main.rs
cargo fmt --all
cargo test --workspace && cargo clippy --workspace -- -D warnings
