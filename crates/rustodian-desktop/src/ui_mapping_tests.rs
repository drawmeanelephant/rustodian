#[cfg(test)]
mod tests {
    use crate::ui_mapping::*;
    use chrono::Utc;
    use rustodian_types::{Project, ProjectCommand, ProjectId, ProjectMetadata};
    use slint::{Model, SharedString};

    #[test]
    fn test_map_commands() {
        let cmd = ProjectCommand {
            name: "test".to_string(),
            description: Some("a test command".to_string()),
            command: "cargo test".to_string(),
            source: "Cargo.toml".to_string(),
            use_shell: false,
        };
        let model = map_commands(&[cmd]);
        assert_eq!(model.row_count(), 1);
        let slint_cmd = model.row_data(0).unwrap();
        assert_eq!(slint_cmd.name, SharedString::from("test"));
        assert_eq!(slint_cmd.description, SharedString::from("a test command"));
        assert_eq!(slint_cmd.command, SharedString::from("cargo test"));
        assert_eq!(slint_cmd.source, SharedString::from("Cargo.toml"));
    }

    #[test]
    fn test_map_project() {
        let proj = Project {
            id: ProjectId::new(),
            name: "my_project".to_string(),
            path: std::path::PathBuf::from("/tmp/test"),
            languages: vec![],
            vcs: None,
            discovered_at: Utc::now(),
            last_scanned_at: None,
            metadata: ProjectMetadata {
                description: Some("a description".to_string()),
                commands: vec![ProjectCommand {
                    name: "run".to_string(),
                    description: None,
                    command: "cargo run".to_string(),
                    source: "Cargo.toml".to_string(),
                    use_shell: false,
                }],
                tags: vec![],
                extra: serde_json::Value::Null,
            },
        };
        let slint_proj = map_project(&proj);
        assert_eq!(slint_proj.name, SharedString::from("my_project"));
        assert_eq!(slint_proj.path, SharedString::from("/tmp/test"));
        assert_eq!(slint_proj.description, SharedString::from("a description"));
        assert_eq!(slint_proj.commands.row_count(), 1);
    }
}
