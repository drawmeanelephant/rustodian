//! The `logs` command.

use anyhow::{Context, Result};
use rustodian_core::Custodian;
use rustodian_storage::SqliteStore;

use crate::OutputFormat;

pub fn execute(
    custodian: &Custodian,
    store: &SqliteStore,
    project_query: &str,
    limit: usize,
    format: &OutputFormat,
) -> Result<()> {
    let project = custodian
        .find_project(project_query)
        .context("Failed to find project")?
        .ok_or_else(|| anyhow::anyhow!("Project not found: {}", project_query))?;

    let logs = store
        .list_logs(&project.id.to_string(), limit)
        .context("Failed to list logs")?;

    match format {
        OutputFormat::Table => {
            if logs.is_empty() {
                println!("No logs found for project '{}'", project.name);
                return Ok(());
            }
            let mut table = comfy_table::Table::new();
            table.set_header(vec!["ID", "Command", "Run At", "Exit Code", "Log Snippet"]);
            for log in logs {
                let snippet = log.log_text.lines().last().unwrap_or("").chars().take(50).collect::<String>();
                let exit_code = log.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "running".to_string());
                table.add_row(vec![
                    log.id,
                    log.command_name,
                    log.run_at.to_string(),
                    exit_code,
                    snippet,
                ]);
            }
            println!("{table}");
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&logs)?;
            println!("{json}");
        }
    }
    Ok(())
}
