//! The `brief` command.

use anyhow::{Context, Result};
use rustodian_core::{BriefCategory, Custodian, ProjectBrief, SuggestedAction};

use crate::OutputFormat;

pub fn execute(custodian: &Custodian, query: Option<&str>, format: &OutputFormat) -> Result<()> {
    let projects = if let Some(query) = query {
        vec![
            custodian
                .find_project(query)
                .context("Failed to find project")?
                .ok_or_else(|| anyhow::anyhow!("Project not found: {query}"))?,
        ]
    } else {
        custodian.list()?
    };
    let report = custodian.brief(projects)?;

    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        OutputFormat::Table => print_table(&report.projects),
    }
    Ok(())
}

fn print_table(projects: &[ProjectBrief]) {
    for category in [
        BriefCategory::NeedsAttention,
        BriefCategory::WorkInProgress,
        BriefCategory::Ready,
        BriefCategory::Unverified,
    ] {
        let matching: Vec<_> = projects
            .iter()
            .filter(|project| project.category == category)
            .collect();
        println!("{}", category.heading());
        if matching.is_empty() {
            println!("  None");
            continue;
        }
        let mut table = comfy_table::Table::new();
        table.set_header(vec![
            "Name",
            "Branch",
            "Working Tree",
            "Latest Command",
            "Result",
            "Timestamp",
            "Suggested Action",
        ]);
        for project in matching {
            let branch = project
                .live_vcs
                .as_ref()
                .and_then(|vcs| vcs.branch.as_deref())
                .unwrap_or("-");
            let working_tree = project.live_vcs.as_ref().map_or("unavailable", |vcs| {
                if vcs.is_dirty { "dirty" } else { "clean" }
            });
            let (command, result, timestamp) = project.latest_command.as_ref().map_or(
                ("-".into(), "-".into(), "-".into()),
                |log| {
                    (
                        log.command_name.clone(),
                        log.exit_code
                            .map_or("running".into(), |code| code.to_string()),
                        log.run_at.to_rfc3339(),
                    )
                },
            );
            let action = project
                .suggested_action
                .as_ref()
                .map_or_else(|| "-".into(), SuggestedAction::text);
            table.add_row(vec![
                project.name.clone(),
                branch.into(),
                working_tree.into(),
                command,
                result,
                timestamp,
                action,
            ]);
        }
        println!("{table}");
    }
}
