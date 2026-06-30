//! The `status` command.

use anyhow::Result;

use rustodian_core::Custodian;

use crate::OutputFormat;

pub fn execute(custodian: &Custodian, format: &OutputFormat) -> Result<()> {
    let status = custodian.status()?;

    match format {
        OutputFormat::Table => {
            let mut table = comfy_table::Table::new();
            table.set_header(vec!["Metric", "Value"]);

            table.add_row(vec![
                "Total Projects".to_string(),
                status.total_projects.to_string(),
            ]);

            if let Some(scan) = &status.last_scan {
                let scan_time = if let Some(completed_at) = scan.completed_at {
                    completed_at.to_rfc3339()
                } else {
                    scan.started_at.to_rfc3339()
                };

                table.add_row(vec!["Last Scan Time".to_string(), scan_time]);
                table.add_row(vec![
                    "Last Scan Status".to_string(),
                    scan.status.to_string(),
                ]);
                table.add_row(vec![
                    "Last Scan Projects Found".to_string(),
                    scan.projects_found.to_string(),
                ]);
                table.add_row(vec![
                    "Last Scan Root Path".to_string(),
                    scan.root_path.display().to_string(),
                ]);
            } else {
                table.add_row(vec!["Last Scan", "None"]);
            }

            if status.languages.is_empty() {
                table.add_row(vec!["Languages", "None"]);
            } else {
                let langs: Vec<String> = status
                    .languages
                    .iter()
                    .map(|(lang, count)| format!("{lang} ({count})"))
                    .collect();
                table.add_row(vec!["Languages".to_string(), langs.join(", ")]);
            }

            println!("{table}");
        }
        OutputFormat::Json => {
            let json = serde_json::json!({
                "total_projects": status.total_projects,
                "last_scan": status.last_scan,
                "languages": status.languages,
            });
            let json_str = serde_json::to_string_pretty(&json)?;
            println!("{json_str}");
        }
    }

    Ok(())
}
