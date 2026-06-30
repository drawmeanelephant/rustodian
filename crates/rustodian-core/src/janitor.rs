//! The Digital Janitor — autonomous workspace cruft purger.
//!
//! Inspects tracked projects for bloated build artifacts and temporary
//! directories, calculates reclaimable bytes, and optionally purges them.
//! Every operation is logged to the project's log table for full auditability.

use std::fs;
use std::path::Path;

use tracing::{info, instrument, warn};

use rustodian_types::{Project, ProjectLog};

use crate::Custodian;
use crate::error::CoreError;

/// Well-known artifact directories that are safe to remove.
const CRUFT_TARGETS: &[&str] = &[
    "target",       // Rust
    "node_modules", // Node / JS
    ".venv",        // Python virtualenv
    ".gopath",      // Go (Rustodian-isolated)
    ".next",        // Next.js
    "dist",         // Generic build output
    "build",        // Generic build output
    "__pycache__",  // Python bytecode cache
];

/// Result of a janitor inspection or clean operation.
#[derive(Debug, Clone)]
pub struct JanitorReport {
    /// Directories that were found (and optionally removed).
    pub targets_found: Vec<String>,
    /// Total bytes reclaimable (or reclaimed if `dry_run` was false).
    pub bytes_reclaimed: u64,
    /// Whether this was a dry-run (inspection only).
    pub dry_run: bool,
}

/// The autonomous Digital Janitor orchestrator.
pub struct DigitalJanitor<'a> {
    custodian: &'a Custodian,
}

impl<'a> DigitalJanitor<'a> {
    pub fn new(custodian: &'a Custodian) -> Self {
        Self { custodian }
    }

    /// Inspect a project for workspace cruft and optionally purge it.
    ///
    /// When `dry_run` is `true`, sizes are calculated but nothing is deleted.
    /// When `dry_run` is `false`, artifacts are removed and the operation is
    /// logged to the `project_logs` table via `store.save_log()`.
    #[instrument(skip(self), fields(project = %project.name, dry_run))]
    pub fn clean(&self, project: &Project, dry_run: bool) -> Result<JanitorReport, CoreError> {
        let mut targets_found = Vec::new();
        let mut bytes_reclaimed: u64 = 0;

        for &target in CRUFT_TARGETS {
            let path = project.path.join(target);
            if path.exists() && path.is_dir() {
                let size = dir_size(&path).unwrap_or(0);
                info!(target, size_bytes = size, "Found artifact directory");

                bytes_reclaimed += size;
                targets_found.push(target.to_string());

                if !dry_run && let Err(e) = fs::remove_dir_all(&path) {
                    warn!(
                        target,
                        error = %e,
                        "Failed to remove artifact directory"
                    );
                }
            }
        }

        if !dry_run && !targets_found.is_empty() {
            #[allow(clippy::cast_precision_loss)]
            let log_text = format!(
                "Digital Janitor: purged {:?}. Reclaimed {} bytes ({:.2} MB).",
                targets_found,
                bytes_reclaimed,
                bytes_reclaimed as f64 / 1_048_576.0,
            );

            let log_record = ProjectLog {
                id: uuid::Uuid::new_v4().to_string(),
                project_id: project.id.to_string(),
                command_name: "janitor:clean".to_string(),
                exit_code: Some(0),
                log_text,
                run_at: chrono::Utc::now(),
            };

            self.custodian.store().save_log(&log_record)?;
        }

        Ok(JanitorReport {
            targets_found,
            bytes_reclaimed,
            dry_run,
        })
    }
}

/// Recursively calculate the total size of a directory in bytes.
fn dir_size(path: &Path) -> std::io::Result<u64> {
    let mut total: u64 = 0;
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_dir() {
                total += dir_size(&p)?;
            } else {
                total += entry.metadata()?.len();
            }
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dir_size_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let size = dir_size(dir.path()).unwrap();
        assert_eq!(size, 0);
    }

    #[test]
    fn test_dir_size_with_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "hello world").unwrap();
        let size = dir_size(dir.path()).unwrap();
        assert_eq!(size, 11); // "hello world" is 11 bytes
    }
}
