//! The `info` command.

use anyhow::{Result, anyhow};
use comfy_table::Table;

use rustodian_core::Custodian;

use crate::OutputFormat;

pub fn execute(custodian: &Custodian, project_query: &str, format: &OutputFormat) -> Result<()> {
    let project = custodian
        .find_project(project_query)?
        .ok_or_else(|| anyhow!("Project not found: {project_query}"))?;

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&project)?);
        }
        OutputFormat::Table => {
            println!("Project Info: {}", project.name);
            println!("ID: {}", project.id);
            println!("Path: {}", project.path.display());
            if !project.languages.is_empty() {
                let langs = project
                    .languages
                    .iter()
                    .map(|l| l.language.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                println!("Languages: {langs}");
            }
            if let Some(vcs) = &project.vcs {
                if let Some(remote) = &vcs.remote_url {
                    println!("Git Remote: {remote}");
                }
                if let Some(commit) = &vcs.last_commit {
                    println!("Commit: {}", commit.sha);
                }
            }
            println!("Discovered: {}", project.discovered_at.to_rfc3339());

            if project.metadata.commands.is_empty() {
                println!("\nNo runnable commands discovered.");
            } else {
                println!("\nDiscovered Commands:");
                let mut table = Table::new();
                table.set_header(vec!["Name", "Command", "Source", "Description"]);
                for cmd in &project.metadata.commands {
                    table.add_row(vec![
                        &cmd.name,
                        &cmd.command,
                        &cmd.source,
                        cmd.description.as_deref().unwrap_or(""),
                    ]);
                }
                println!("{table}");
            }
        }
    }

    Ok(())
}
