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
    let _ = (custodian, path, max_depth, format);
    todo!("Implement scan command")
}
