use anyhow::{Result, anyhow};
use comfy_table::Table;

use rustodian_core::Custodian;

use crate::OutputFormat;

pub fn execute(
    custodian: &Custodian,
    project_query: &str,
    dry_run: bool,
    format: &OutputFormat,
) -> Result<()> {
    let project = custodian
        .find_project(project_query)?
        .ok_or_else(|| anyhow!("Project not found: {project_query}"))?;

    let janitor = rustodian_core::janitor::DigitalJanitor::new(custodian);
    let report = janitor.clean(&project, dry_run)?;

    match format {
        OutputFormat::Json => {
            let json = serde_json::json!({
                "targets_found": report.targets_found,
                "bytes_reclaimed": report.bytes_reclaimed,
                "dry_run": report.dry_run,
            });
            let json_str = serde_json::to_string_pretty(&json)?;
            println!("{json_str}");
        }
        OutputFormat::Table => {
            let mut table = Table::new();
            table.set_header(vec!["Cruft Target", "Status", "Bytes"]);

            let status = if report.dry_run {
                "Reclaimable (Dry Run)"
            } else {
                "Reclaimed"
            };

            for target in &report.targets_found {
                table.add_row(vec![target.clone(), status.to_string(), String::new()]);
            }

            table.add_row(vec![
                "Total".to_string(),
                status.to_string(),
                report.bytes_reclaimed.to_string(),
            ]);

            println!("{table}");
        }
    }

    Ok(())
}
