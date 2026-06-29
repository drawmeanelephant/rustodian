//! The `status` command.

use anyhow::Result;

use rustodian_core::Custodian;

use crate::OutputFormat;

pub fn execute(custodian: &Custodian, format: &OutputFormat) -> Result<()> {
    let _ = (custodian, format);
    Err(anyhow::anyhow!(
        "The 'status' command is not yet implemented"
    ))
}
