use anyhow::{Result, anyhow};
use comfy_table::Table;

use rustodian_core::{Custodian, janitor::JanitorTargetResult};

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
                "targets": report.targets.iter().map(json_target).collect::<Vec<_>>(),
                "bytes_reclaimed": report.bytes_reclaimed,
                "dry_run": report.dry_run,
            });
            let json_str = serde_json::to_string_pretty(&json)?;
            println!("{json_str}");
        }
        OutputFormat::Table => {
            let mut table = Table::new();
            table.set_header(vec!["Cruft Target", "Outcome", "Size", "Reason"]);

            let mut targets: Vec<&JanitorTargetResult> = report.targets.iter().collect();
            targets.sort_by(|left, right| {
                let left_actionable = matches!(
                    left.outcome,
                    rustodian_core::janitor::JanitorOutcome::Reclaimable
                        | rustodian_core::janitor::JanitorOutcome::Removed
                );
                let right_actionable = matches!(
                    right.outcome,
                    rustodian_core::janitor::JanitorOutcome::Reclaimable
                        | rustodian_core::janitor::JanitorOutcome::Removed
                );
                right_actionable
                    .cmp(&left_actionable)
                    .then_with(|| right.size_bytes.cmp(&left.size_bytes))
                    .then_with(|| left.path.cmp(&right.path))
            });

            for target in targets {
                table.add_row(vec![
                    target.target.clone(),
                    target.outcome.as_str().to_string(),
                    target
                        .size_bytes
                        .map_or_else(|| "-".to_string(), format_bytes),
                    target
                        .reason
                        .as_deref()
                        .map_or_else(String::new, concise_reason),
                ]);
            }

            table.add_row(vec![
                "Total".to_string(),
                if report.dry_run {
                    "reclaimable".to_string()
                } else {
                    "reclaimed".to_string()
                },
                format_bytes(report.bytes_reclaimed),
                String::new(),
            ]);

            println!("{table}");
        }
    }

    if !dry_run && report.has_failures() {
        return Err(anyhow!("Janitor purge completed with target failures"));
    }

    Ok(())
}

fn json_target(target: &JanitorTargetResult) -> serde_json::Value {
    serde_json::json!({
        "target": target.target,
        "path": target.path.display().to_string(),
        "size_bytes": target.size_bytes,
        "outcome": target.outcome.as_str(),
        "reason": target.reason,
    })
}

#[allow(clippy::cast_precision_loss)]
fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn concise_reason(reason: &str) -> String {
    const LIMIT: usize = 72;
    if reason.chars().count() <= LIMIT {
        return reason.to_string();
    }
    format!("{}…", reason.chars().take(LIMIT - 1).collect::<String>())
}
