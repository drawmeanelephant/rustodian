//! The `info` command.

use anyhow::Result;

use rustodian_core::Custodian;

use crate::OutputFormat;

pub fn execute(custodian: &Custodian, project: &str, format: &OutputFormat) -> Result<()> {
    let _ = (custodian, project, format);
    todo!("Implement info command")
}
