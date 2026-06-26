use std::fs;
use std::path::Path;

use rustodian_types::ProjectCommand;

pub struct CommandDiscoverer;

impl CommandDiscoverer {
    pub fn discover(root: &Path) -> Vec<ProjectCommand> {
        let mut commands = Vec::new();

        // 1. Rustodian config (.rustodian.toml)
        let toml_content = fs::read_to_string(root.join(".rustodian.toml"));
        let toml_config = toml_content
            .ok()
            .and_then(|c| toml::from_str::<toml::Value>(&c).ok());
        if let Some(commands_table) = toml_config
            .as_ref()
            .and_then(|config| config.get("commands"))
            .and_then(|c| c.as_table())
        {
            for (name, cmd) in commands_table {
                if let Some(cmd_str) = cmd.as_str() {
                    commands.push(ProjectCommand {
                        name: name.clone(),
                        description: Some("rustodian config".to_string()),
                        command: cmd_str.to_string(),
                        source: ".rustodian.toml".to_string(),
                    });
                }
            }
        }

        // 2. Rust standard commands if Cargo.toml exists
        if root.join("Cargo.toml").exists() {
            commands.extend(Self::rust_defaults());
        }

        // 3. Node.js scripts if package.json exists
        let pkg_content = fs::read_to_string(root.join("package.json"));
        let pkg_json = pkg_content
            .ok()
            .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok());
        if let Some(scripts) = pkg_json
            .as_ref()
            .and_then(|json| json.get("scripts"))
            .and_then(|s| s.as_object())
        {
            for (name, _) in scripts {
                commands.push(ProjectCommand {
                    name: name.clone(),
                    description: Some("npm run script".to_string()),
                    command: format!("npm run {name}"),
                    source: "package.json".to_string(),
                });
            }
        }

        // 3. Justfile recipes
        let justfile_paths = [root.join("justfile"), root.join("Justfile")];
        for path in justfile_paths {
            if let Ok(content) = fs::read_to_string(&path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty()
                        || trimmed.starts_with('#')
                        || line.starts_with(' ')
                        || line.starts_with('\t')
                    {
                        continue;
                    }
                    if let Some(idx) = trimmed.find(':') {
                        let recipe_def = &trimmed[..idx];
                        if let Some(n) = recipe_def.split_whitespace().next().filter(|n| {
                            !n.is_empty()
                                && n.chars()
                                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                        }) {
                            commands.push(ProjectCommand {
                                name: n.to_string(),
                                description: Some("just recipe".to_string()),
                                command: format!("just {n}"),
                                source: "justfile".to_string(),
                            });
                        }
                    }
                }
                break; // stop after first found justfile
            }
        }

        commands
    }

    fn rust_defaults() -> Vec<ProjectCommand> {
        vec![
            ProjectCommand {
                name: "test".to_string(),
                description: Some("Run cargo test".to_string()),
                command: "cargo test".to_string(),
                source: "Cargo.toml".to_string(),
            },
            ProjectCommand {
                name: "build".to_string(),
                description: Some("Run cargo build".to_string()),
                command: "cargo build".to_string(),
                source: "Cargo.toml".to_string(),
            },
            ProjectCommand {
                name: "check".to_string(),
                description: Some("Run cargo check".to_string()),
                command: "cargo check".to_string(),
                source: "Cargo.toml".to_string(),
            },
            ProjectCommand {
                name: "clippy".to_string(),
                description: Some("Run cargo clippy".to_string()),
                command: "cargo clippy".to_string(),
                source: "Cargo.toml".to_string(),
            },
            ProjectCommand {
                name: "fmt".to_string(),
                description: Some("Run cargo fmt".to_string()),
                command: "cargo fmt".to_string(),
                source: "Cargo.toml".to_string(),
            },
        ]
    }
}
