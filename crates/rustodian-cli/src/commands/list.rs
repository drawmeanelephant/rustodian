//! The `list` command.

use anyhow::Result;

use rustodian_core::Custodian;

use crate::OutputFormat;

pub fn execute(custodian: &Custodian, language: Option<&str>, format: &OutputFormat) -> Result<()> {
    let mut projects = custodian.list()?;

    if let Some(lang) = language {
        let lang = lang.to_lowercase();
        projects.retain(|p| {
            p.languages.iter().any(|l| {
                format!("{:?}", l.language).to_lowercase() == lang
            })
        });
    }

    match format {
        OutputFormat::Table => {
            if projects.is_empty() {
                println!("No projects found.");
                return Ok(());
            }

            let mut table = comfy_table::Table::new();
            table.set_header(vec!["Name", "Path", "Languages", "VCS"]);

            for p in projects {
                let langs: Vec<String> = p
                    .languages
                    .iter()
                    .map(|l| format!("{:?}", l.language))
                    .collect();
                
                let vcs = if let Some(vcs) = p.vcs {
                    format!("{:?} ({})", vcs.vcs_type, vcs.branch.unwrap_or_else(|| "detached".to_string()))
                } else {
                    "None".to_string()
                };

                table.add_row(vec![
                    p.name,
                    p.path.display().to_string(),
                    langs.join(", "),
                    vcs,
                ]);
            }

            println!("{table}");
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&projects)?;
            println!("{json}");
        }
    }

    Ok(())
}
