//! The `prune` command.
//!
//! Removes stale *database* records for tracked projects whose stored paths no
//! longer exist. Defaults to a dry run; `--purge` performs the deletion.
//! Project files are never touched.

use anyhow::Result;

use rustodian_core::{Custodian, PruneProjectResult};

use crate::OutputFormat;

pub fn execute(custodian: &Custodian, purge: bool, format: &OutputFormat) -> Result<()> {
    let mut report = custodian.prune(purge)?;
    // Stable ordering for auditable, reproducible output.
    report.projects.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.id.to_string().cmp(&right.id.to_string()))
    });

    match format {
        OutputFormat::Json => {
            let json = serde_json::json!({
                "dry_run": report.dry_run,
                "stale_project_count": report.stale_project_count,
                "projects": report.projects.iter().map(json_project).collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        OutputFormat::Table => {
            if report.projects.is_empty() {
                println!("No stale projects found. The database is up to date.");
                return Ok(());
            }

            let mut table = comfy_table::Table::new();
            table.set_header(vec!["Name", "ID", "Path", "Outcome"]);
            for project in &report.projects {
                table.add_row(vec![
                    project.name.clone(),
                    project.id.to_string(),
                    project.path.display().to_string(),
                    project.outcome.as_str().to_string(),
                ]);
            }
            println!("{table}");

            if report.dry_run {
                println!(
                    "{} stale project(s) found (dry run). Re-run with --purge to remove their database records.",
                    report.stale_project_count
                );
            } else {
                println!(
                    "Removed {} stale project(s) from the database. Project files were not touched.",
                    report.stale_project_count
                );
            }
        }
    }

    Ok(())
}

fn json_project(project: &PruneProjectResult) -> serde_json::Value {
    serde_json::json!({
        "id": project.id.to_string(),
        "name": project.name,
        "path": project.path.display().to_string(),
        "outcome": project.outcome.as_str(),
    })
}
