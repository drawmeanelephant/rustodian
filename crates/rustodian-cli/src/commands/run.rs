//! The `run` command.

use anyhow::{Result, Context};
use rustodian_core::Custodian;

pub fn execute(custodian: &Custodian, project_query: &str, command_name: &str) -> Result<()> {
    println!("Running command '{}' in project '{}'...", command_name, project_query);
    custodian
        .run_command(project_query, command_name)
        .context("Failed to run command")?;
    println!("Command executed successfully.");
    Ok(())
}
