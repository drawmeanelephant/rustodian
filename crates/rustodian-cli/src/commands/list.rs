//! The `list` command.

use anyhow::Result;

use rustodian_core::Custodian;

use crate::OutputFormat;

pub fn execute(
    custodian: &Custodian,
    language: Option<&str>,
    format: &OutputFormat,
) -> Result<()> {
    let _ = (custodian, language, format);
    todo!("Implement list command")
}
