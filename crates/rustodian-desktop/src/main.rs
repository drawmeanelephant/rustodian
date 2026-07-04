pub mod ui_mapping {
    use rustodian_types::{Project, ProjectCommand};
    use slint::{ModelRc, SharedString, VecModel};

    slint::slint! {
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

        export component Dummy inherits Window {}
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

    #[cfg(test)]
    mod tests {
        use super::*;
        use chrono::Utc;
        use rustodian_types::{ProjectId, ProjectMetadata};
        use slint::Model;
        use std::path::PathBuf;

        #[test]
        fn test_map_project() {
            let project = Project {
                id: ProjectId::new(),
                name: "TestProject".to_string(),
                path: PathBuf::from("/test/path"),
                languages: vec![],
                vcs: None,
                discovered_at: Utc::now(),
                last_scanned_at: None,
                metadata: ProjectMetadata::default(),
            };

            let slint_project = map_project(&project);

            assert_eq!(slint_project.id.as_str(), project.id.to_string());
            assert_eq!(slint_project.name.as_str(), "TestProject");
            assert_eq!(slint_project.path.as_str(), "/test/path");
        }

        #[test]
        fn test_map_project_command() {
            let cmd = ProjectCommand {
                name: "test-cmd".to_string(),
                description: Some("Test description".to_string()),
                command: "cargo test".to_string(),
                source: "Cargo.toml".to_string(),
                use_shell: false,
            };

            let slint_cmd = map_project_command(&cmd);

            assert_eq!(slint_cmd.name.as_str(), "test-cmd");
            assert_eq!(slint_cmd.description.as_str(), "Test description");
            assert_eq!(slint_cmd.command.as_str(), "cargo test");
            assert_eq!(slint_cmd.source.as_str(), "Cargo.toml");
            assert_eq!(slint_cmd.use_shell, false);
        }

        #[test]
        fn test_map_project_command_no_desc() {
            let cmd = ProjectCommand {
                name: "test-cmd".to_string(),
                description: None,
                command: "cargo test".to_string(),
                source: "Cargo.toml".to_string(),
                use_shell: true,
            };

            let slint_cmd = map_project_command(&cmd);
            assert_eq!(slint_cmd.description.as_str(), "");
            assert_eq!(slint_cmd.use_shell, true);
        }

        #[test]
        fn test_map_projects_array() {
            let p1 = Project {
                id: ProjectId::new(),
                name: "P1".to_string(),
                path: PathBuf::from("/p1"),
                languages: vec![],
                vcs: None,
                discovered_at: Utc::now(),
                last_scanned_at: None,
                metadata: ProjectMetadata::default(),
            };
            let p2 = Project {
                id: ProjectId::new(),
                name: "P2".to_string(),
                path: PathBuf::from("/p2"),
                languages: vec![],
                vcs: None,
                discovered_at: Utc::now(),
                last_scanned_at: None,
                metadata: ProjectMetadata::default(),
            };

            let models = map_projects(&[p1.clone(), p2.clone()]);
            assert_eq!(models.row_count(), 2);
            assert_eq!(models.row_data(0).unwrap().name.as_str(), "P1");
            assert_eq!(models.row_data(1).unwrap().name.as_str(), "P2");
        }
    }
}

fn main() {
    println!("Migrating to Slint...");
}
