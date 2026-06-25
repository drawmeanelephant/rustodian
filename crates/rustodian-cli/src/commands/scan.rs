//! The `scan` command.

use std::path::Path;

use anyhow::Result;

use rustodian_core::Custodian;

use crate::OutputFormat;

pub fn execute(
    custodian: &Custodian,
    path: &Path,
    max_depth: usize,
    format: &OutputFormat,
) -> Result<()> {
    let config = rustodian_types::ScanConfig {
        max_depth,
        ..Default::default()
    };

    let report = custodian.scan(path, &config)?;

    match format {
        OutputFormat::Table => {
            println!("Scan Complete");
            println!("-------------");
            println!("Scan ID: {}", report.scan_id);
            println!("Projects Found:   {}", report.projects_found);
            println!("New Projects:     {}", report.projects_new);
            println!("Updated Projects: {}", report.projects_updated);
        }
        OutputFormat::Json => {
            println!(
                "{{\"scan_id\":\"{}\",\"projects_found\":{},\"projects_new\":{},\"projects_updated\":{}}}",
                report.scan_id, report.projects_found, report.projects_new, report.projects_updated
            );
        }
    }

    Ok(())
}
