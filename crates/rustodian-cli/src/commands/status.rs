//! The `status` command.

use anyhow::Result;

use rustodian_core::Custodian;

use crate::OutputFormat;

pub fn execute(custodian: &Custodian, format: &OutputFormat) -> Result<()> {
    let _ = (custodian, format);
    todo!("Implement status command")
}
