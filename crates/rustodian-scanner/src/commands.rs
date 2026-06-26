use std::fs;
use std::path::Path;

use rustodian_types::ProjectCommand;

pub struct CommandDiscoverer;

impl CommandDiscoverer {
    pub fn discover(root: &Path) -> Vec<ProjectCommand> {
        let mut commands = Vec::new();

        // 1. Rust standard commands if Cargo.toml exists
        if root.join("Cargo.toml").exists() {
            commands.extend(Self::rust_defaults());
        }

        // 2. Node.js scripts if package.json exists
        if let Ok(content) = fs::read_to_string(root.join("package.json")) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(scripts) = json.get("scripts").and_then(|s| s.as_object()) {
                    for (name, _) in scripts {
                        commands.push(ProjectCommand {
                            name: name.clone(),
                            description: Some("npm run script".to_string()),
                            command: format!("npm run {}", name),
                            source: "package.json".to_string(),
                        });
                    }
                }
            }
        }

        // 3. Justfile recipes
        let justfile_paths = [root.join("justfile"), root.join("Justfile")];
        for path in justfile_paths {
            if let Ok(content) = fs::read_to_string(&path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('#') || line.starts_with(' ') || line.starts_with('\t') {
                        continue;
                    }
                    if let Some(idx) = trimmed.find(':') {
                        let recipe_def = &trimmed[..idx];
                        if let Some(n) = recipe_def.split_whitespace().next() {
                            if !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                                commands.push(ProjectCommand {
                                    name: n.to_string(),
                                    description: Some("just recipe".to_string()),
                                    command: format!("just {}", n),
                                    source: "justfile".to_string(),
                                });
                            }
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
