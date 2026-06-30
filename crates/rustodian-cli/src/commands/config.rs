//! The `config` command.

use std::env;
use std::path::Path;

use anyhow::Result;

use crate::OutputFormat;

pub fn execute(db_path: &Path, format: &OutputFormat) -> Result<()> {
    let scan_root = env::var("RUSTODIAN_SCAN_ROOT").unwrap_or_else(|_| ".".to_string());

    match format {
        OutputFormat::Table => {
            let mut table = comfy_table::Table::new();
            table.set_header(vec!["Configuration", "Value"]);

            table.add_row(vec![
                "Database Path".to_string(),
                db_path.display().to_string(),
            ]);
            table.add_row(vec!["Scan Root".to_string(), scan_root]);

            println!("{table}");
        }
        OutputFormat::Json => {
            #[derive(serde::Serialize)]
            struct ConfigOutput<'a> {
                db_path: &'a Path,
                scan_root: &'a str,
            }

            let output = ConfigOutput {
                db_path,
                scan_root: &scan_root,
            };

            let json = serde_json::to_string_pretty(&output)?;
            println!("{json}");
        }
    }

    Ok(())
}
