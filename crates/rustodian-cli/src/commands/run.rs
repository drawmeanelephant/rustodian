//! The `run` command.

use anyhow::{Context, Result};
use rustodian_core::Custodian;

pub fn execute(custodian: &Custodian, project_query: &str, command_name: &str) -> Result<()> {
    println!("Running command '{command_name}' in project '{project_query}'...");
    custodian
        .run_command(project_query, command_name)
        .context("Failed to run command")?;
    println!("Command executed successfully.");
    Ok(())
}
