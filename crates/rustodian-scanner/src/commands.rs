use std::collections::HashMap;
use std::fs;
use std::path::Path;

use rustodian_types::ProjectCommand;

pub struct CommandDiscoverer;

impl CommandDiscoverer {
    pub fn discover(root: &Path) -> Vec<ProjectCommand> {
        fn needs_shell(cmd: &str) -> bool {
            cmd.contains("&&")
                || cmd.contains("||")
                || cmd.contains('|')
                || cmd.contains('>')
                || cmd.contains('<')
                || cmd.contains("$(")
        }

        // Resolution: exactly one `ProjectCommand` per command name. When the
        // same name is provided by several sources, the highest-priority one
        // wins, in this order:
        //   1. .rustodian.toml
        //   2. Justfile / justfile
        //   3. package.json
        //   4. generated language defaults (e.g. Cargo.toml)
        // Sources are collected lowest-priority first so that a later insert
        // under the same name overwrites the earlier, lower-priority
        // definition. The resulting set is sorted by name at the end.
        let mut resolved: HashMap<String, ProjectCommand> = HashMap::new();

        // 4 (lowest priority). Rust standard commands if Cargo.toml exists.
        if root.join("Cargo.toml").exists() {
            for cmd in Self::rust_defaults() {
                resolved.insert(cmd.name.clone(), cmd);
            }
        }

        // 3. Node.js scripts if package.json exists.
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
                resolved.insert(
                    name.clone(),
                    ProjectCommand {
                        name: name.clone(),
                        description: Some("npm run script".to_string()),
                        command: format!("npm run {name}"),
                        source: "package.json".to_string(),
                        use_shell: needs_shell(name),
                    },
                );
            }
        }

        // 2. Justfile recipes.
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
                            resolved.insert(
                                n.to_string(),
                                ProjectCommand {
                                    name: n.to_string(),
                                    description: Some("just recipe".to_string()),
                                    command: format!("just {n}"),
                                    source: "justfile".to_string(),
                                    use_shell: needs_shell(n),
                                },
                            );
                        }
                    }
                }
                break; // stop after first found justfile
            }
        }

        // 1 (highest priority). Rustodian config (.rustodian.toml).
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
                    resolved.insert(
                        name.clone(),
                        ProjectCommand {
                            name: name.clone(),
                            description: Some("rustodian config".to_string()),
                            command: cmd_str.to_string(),
                            source: ".rustodian.toml".to_string(),
                            use_shell: needs_shell(cmd_str),
                        },
                    );
                }
            }
        }

        // Deterministic output order: alphabetical by command name.
        let mut commands: Vec<ProjectCommand> = resolved.into_values().collect();
        commands.sort_by(|a, b| a.name.cmp(&b.name));
        commands
    }

    fn rust_defaults() -> Vec<ProjectCommand> {
        vec![
            ProjectCommand {
                name: "test".to_string(),
                description: Some("Run cargo test".to_string()),
                command: "cargo test".to_string(),
                source: "Cargo.toml".to_string(),
                use_shell: false,
            },
            ProjectCommand {
                name: "build".to_string(),
                description: Some("Run cargo build".to_string()),
                command: "cargo build".to_string(),
                source: "Cargo.toml".to_string(),
                use_shell: false,
            },
            ProjectCommand {
                name: "check".to_string(),
                description: Some("Run cargo check".to_string()),
                command: "cargo check".to_string(),
                source: "Cargo.toml".to_string(),
                use_shell: false,
            },
            ProjectCommand {
                name: "clippy".to_string(),
                description: Some("Run cargo clippy".to_string()),
                command: "cargo clippy".to_string(),
                source: "Cargo.toml".to_string(),
                use_shell: false,
            },
            ProjectCommand {
                name: "fmt".to_string(),
                description: Some("Run cargo fmt".to_string()),
                command: "cargo fmt".to_string(),
                source: "Cargo.toml".to_string(),
                use_shell: false,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Names of the commands discovered for `root`.
    fn command_names(root: &Path) -> Vec<String> {
        CommandDiscoverer::discover(root)
            .into_iter()
            .map(|c| c.name)
            .collect()
    }

    /// The single discovered command with the given name.
    fn command_by_name(root: &Path, name: &str) -> ProjectCommand {
        CommandDiscoverer::discover(root)
            .into_iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("expected a command named {name:?}"))
    }

    #[test]
    fn duplicate_test_across_all_sources_resolves_to_rustodian_toml() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join("Cargo.toml"), "[package]\n").unwrap();
        fs::write(
            root.join("package.json"),
            r#"{"scripts": {"test": "jest"}}"#,
        )
        .unwrap();
        fs::write(root.join("justfile"), "test:\n  echo t\n").unwrap();
        fs::write(
            root.join(".rustodian.toml"),
            "[commands]\ntest = \"cargo test --workspace\"\n",
        )
        .unwrap();

        let cmd = command_by_name(root, "test");
        assert_eq!(cmd.source, ".rustodian.toml");
        assert_eq!(cmd.command, "cargo test --workspace");

        // Every source defines exactly one `test`; only the toml one survives.
        assert_eq!(
            command_names(root).iter().filter(|n| *n == "test").count(),
            1
        );
    }

    #[test]
    fn rustodian_toml_overrides_rust_default() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join("Cargo.toml"), "[package]\n").unwrap();
        fs::write(
            root.join(".rustodian.toml"),
            "[commands]\ncheck = \"cargo check --all-features\"\n",
        )
        .unwrap();

        let cmd = command_by_name(root, "check");
        assert_eq!(cmd.source, ".rustodian.toml");
        assert_eq!(cmd.command, "cargo check --all-features");

        // The non-colliding rust defaults remain available.
        for default in ["test", "build", "clippy", "fmt"] {
            let default_cmd = command_by_name(root, default);
            assert_eq!(default_cmd.source, "Cargo.toml");
        }
    }

    #[test]
    fn justfile_overrides_package_json() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(
            root.join("package.json"),
            r#"{"scripts": {"build": "webpack"}}"#,
        )
        .unwrap();
        fs::write(root.join("justfile"), "build:\n  make build\n").unwrap();

        let cmd = command_by_name(root, "build");
        assert_eq!(cmd.source, "justfile");
        assert_eq!(cmd.command, "just build");
        assert_eq!(
            command_names(root).iter().filter(|n| *n == "build").count(),
            1
        );
    }

    #[test]
    fn unique_commands_from_all_sources_survive() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join("Cargo.toml"), "[package]\n").unwrap();
        fs::write(
            root.join("package.json"),
            r#"{"scripts": {"dev": "vite", "lint": "eslint"}}"#,
        )
        .unwrap();
        fs::write(root.join("justfile"), "clean:\n  rm -rf out\n").unwrap();
        fs::write(
            root.join(".rustodian.toml"),
            "[commands]\ndeploy = \"rsync ./dist host:/srv/app\"\n",
        )
        .unwrap();

        let names = command_names(root);
        assert_eq!(names.len(), 9); // 5 rust defaults + dev + lint + clean + deploy
        for name in [
            "test", "build", "check", "clippy", "fmt", "dev", "lint", "clean", "deploy",
        ] {
            assert!(names.contains(&name.to_string()), "missing {name}");
        }
    }

    #[test]
    fn output_is_sorted_and_stable_across_runs() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        fs::write(root.join("Cargo.toml"), "[package]\n").unwrap();
        fs::write(
            root.join("package.json"),
            r#"{"scripts": {"zzz": "sleep", "aaa": "echo"}}"#,
        )
        .unwrap();
        fs::write(root.join("justfile"), "mid:\n  echo mid\n").unwrap();
        fs::write(
            root.join(".rustodian.toml"),
            "[commands]\nbbb = \"echo b\"\n",
        )
        .unwrap();

        let first = CommandDiscoverer::discover(root);
        let second = CommandDiscoverer::discover(root);

        // Compare on (name, command, source) tuples: `ProjectCommand` has no
        // `PartialEq` impl, and the key fields fully determine each entry.
        let keyed = |cmds: &[ProjectCommand]| {
            cmds.iter()
                .map(|c| (c.name.clone(), c.command.clone(), c.source.clone()))
                .collect::<Vec<_>>()
        };
        let first_keys = keyed(&first);
        let second_keys = keyed(&second);

        // Deterministic ordering: alphabetical by command name.
        let mut sorted_keys = first_keys.clone();
        sorted_keys.sort();
        assert_eq!(first_keys, sorted_keys);

        // Identical output across runs.
        assert_eq!(first_keys, second_keys);
        assert_eq!(
            command_names(root),
            [
                "aaa", "bbb", "build", "check", "clippy", "fmt", "mid", "test", "zzz"
            ]
        );
    }
}
