sed -i '/ui.heading("🏛️ Projects");/a \
                ui.horizontal(|ui| {\
                    ui.label("Root:");\
                    ui.text_edit_singleline(\&mut self.scan_root_input);\
                    if ui.button("Browse...").clicked() {\
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {\
                            self.scan_root_input = path.to_string_lossy().to_string();\
                        }\
                    }\
                });\
                if ui.button("Scan").clicked() {\
                    let path = std::path::PathBuf::from(\&self.scan_root_input);\
                    self.send(GuiMessage::ScanProjects { path: path.clone() });\
                    self.scan_status = Some("Scanning...".to_string());\
                    // Update the setting in the DB via worker or do it when scanning.\
                    // Since worker does the scan, we can just let worker do it, or we do it here.\
                    // Actually, let'\''s just trigger the scan.\
                }\
                if let Some(status) = \&self.scan_status {\
                    ui.label(status);\
                }\
                ui.separator();\
' crates/rustodian-desktop/src/main.rs
