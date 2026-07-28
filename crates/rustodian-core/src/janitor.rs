//! The Digital Janitor — language-aware workspace artifact cleanup.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tracing::{info, instrument, warn};

use rustodian_types::{Language, Project, ProjectLog};

use crate::Custodian;
use crate::error::CoreError;

/// The disposition of one cleanup target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JanitorOutcome {
    Reclaimable,
    Removed,
    Skipped,
    Failed,
}

impl JanitorOutcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Reclaimable => "reclaimable",
            Self::Removed => "removed",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
        }
    }

    #[must_use]
    const fn is_actionable(self) -> bool {
        matches!(self, Self::Reclaimable | Self::Removed)
    }
}

/// The result of inspecting or removing one cleanup target.
#[derive(Debug, Clone)]
pub struct JanitorTargetResult {
    pub target: String,
    pub path: PathBuf,
    pub size_bytes: Option<u64>,
    pub outcome: JanitorOutcome,
    pub reason: Option<String>,
}

/// Result of a janitor inspection or clean operation.
#[derive(Debug, Clone)]
pub struct JanitorReport {
    /// Results for every discovered cleanup target and any validation failure.
    pub targets: Vec<JanitorTargetResult>,
    /// Total bytes reclaimable (or actually removed when `dry_run` is false).
    pub bytes_reclaimed: u64,
    /// Whether this was an inspection only.
    pub dry_run: bool,
}

impl JanitorReport {
    #[must_use]
    pub fn has_failures(&self) -> bool {
        self.targets
            .iter()
            .any(|target| target.outcome == JanitorOutcome::Failed)
    }
}

#[derive(Debug)]
struct Candidate {
    target: &'static str,
    path: PathBuf,
}

/// The autonomous Digital Janitor orchestrator.
pub struct DigitalJanitor<'a> {
    custodian: &'a Custodian,
}

impl<'a> DigitalJanitor<'a> {
    pub fn new(custodian: &'a Custodian) -> Self {
        Self { custodian }
    }

    /// Inspect a project for language-supported artifacts and optionally purge them.
    ///
    /// A purge always records one audit log, including failed targets. Dry runs do
    /// not mutate either the filesystem or the project database.
    #[instrument(skip(self), fields(project = %project.name, dry_run))]
    pub fn clean(&self, project: &Project, dry_run: bool) -> Result<JanitorReport, CoreError> {
        let mut report = JanitorReport {
            targets: Vec::new(),
            bytes_reclaimed: 0,
            dry_run,
        };

        match validated_project_root(&project.path) {
            Ok(root) => {
                let mut candidates = Vec::new();
                collect_direct_candidates(project, &root, &mut candidates, &mut report.targets);
                if supports_language(project, |language| matches!(language, Language::Python)) {
                    collect_python_caches(&root, &mut candidates, &mut report.targets);
                }

                for candidate in candidates {
                    let result = inspect_candidate(&root, candidate, dry_run);
                    if result.outcome.is_actionable() {
                        report.bytes_reclaimed += result.size_bytes.unwrap_or(0);
                    }
                    report.targets.push(result);
                }
            }
            Err(reason) => report.targets.push(JanitorTargetResult {
                target: "project root".to_string(),
                path: project.path.clone(),
                size_bytes: None,
                outcome: JanitorOutcome::Failed,
                reason: Some(reason),
            }),
        }

        if !dry_run {
            self.save_purge_log(project, &report)?;
            if let Err(error) = self
                .custodian
                .store()
                .prune_logs(&project.id.to_string(), 50)
            {
                warn!(error = %error, "Failed to prune old Janitor logs");
            }
        }

        Ok(report)
    }

    fn save_purge_log(&self, project: &Project, report: &JanitorReport) -> Result<(), CoreError> {
        let failures: Vec<&JanitorTargetResult> = report
            .targets
            .iter()
            .filter(|target| target.outcome == JanitorOutcome::Failed)
            .collect();
        let targets = report
            .targets
            .iter()
            .map(|target| {
                format!(
                    "target={} path={} outcome={} size_bytes={} reason={}",
                    target.target,
                    target.path.display(),
                    target.outcome.as_str(),
                    target
                        .size_bytes
                        .map_or_else(|| "unavailable".to_string(), |size| size.to_string()),
                    target.reason.as_deref().unwrap_or("none"),
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        let failure_paths = failures
            .iter()
            .map(|target| target.path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");

        let log_record = ProjectLog {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: project.id.to_string(),
            command_name: "janitor:clean".to_string(),
            exit_code: Some(i32::from(!failures.is_empty())),
            log_text: format!(
                "Digital Janitor purge: targets=[{targets}]; bytes_reclaimed={}; failures=[{failure_paths}]; success={}",
                report.bytes_reclaimed,
                failures.is_empty(),
            ),
            run_at: chrono::Utc::now(),
        };
        self.custodian.store().save_log(&log_record)
    }
}

fn supports_language(project: &Project, predicate: impl Fn(&Language) -> bool) -> bool {
    project
        .languages
        .iter()
        .any(|detection| predicate(&detection.language))
}

fn collect_direct_candidates(
    project: &Project,
    root: &Path,
    candidates: &mut Vec<Candidate>,
    results: &mut Vec<JanitorTargetResult>,
) {
    let mut targets = Vec::new();
    if supports_language(project, |language| matches!(language, Language::Rust)) {
        targets.push("target");
    }
    if supports_language(project, |language| matches!(language, Language::Node)) {
        targets.extend(["node_modules", ".next"]);
    }
    if supports_language(project, |language| matches!(language, Language::Python)) {
        targets.push(".venv");
    }
    if supports_language(project, |language| matches!(language, Language::Go)) {
        targets.push(".gopath");
    }

    for target in targets {
        let path = root.join(target);
        match fs::symlink_metadata(&path) {
            Ok(_) => candidates.push(Candidate { target, path }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => results.push(failed_result(target, path, None, &error)),
        }
    }
}

fn collect_python_caches(
    root: &Path,
    candidates: &mut Vec<Candidate>,
    results: &mut Vec<JanitorTargetResult>,
) {
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                results.push(failed_result(
                    "__pycache__ discovery",
                    directory,
                    None,
                    &error,
                ));
                continue;
            }
        };

        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    results.push(failed_result(
                        "__pycache__ discovery",
                        directory.clone(),
                        None,
                        &error,
                    ));
                    continue;
                }
            };
            let path = entry.path();
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    results.push(failed_result("__pycache__ discovery", path, None, &error));
                    continue;
                }
            };
            if entry.file_name() == "__pycache__" {
                candidates.push(Candidate {
                    target: "__pycache__",
                    path,
                });
            } else if !metadata.file_type().is_symlink()
                && metadata.is_dir()
                && !is_cleanup_directory(&entry.file_name())
            {
                stack.push(path);
            }
        }
    }
}

fn is_cleanup_directory(name: &std::ffi::OsStr) -> bool {
    matches!(
        name.to_str(),
        Some("target" | "node_modules" | ".next" | ".venv" | ".gopath")
    )
}

fn inspect_candidate(root: &Path, candidate: Candidate, dry_run: bool) -> JanitorTargetResult {
    if !candidate.path.starts_with(root) {
        return failed_result(
            candidate.target,
            candidate.path,
            None,
            &io::Error::other("candidate is not lexically contained in the project root"),
        );
    }

    let metadata = match fs::symlink_metadata(&candidate.path) {
        Ok(metadata) => metadata,
        Err(error) => return failed_result(candidate.target, candidate.path, None, &error),
    };
    if metadata.file_type().is_symlink() {
        return JanitorTargetResult {
            target: candidate.target.to_string(),
            path: candidate.path,
            size_bytes: None,
            outcome: JanitorOutcome::Skipped,
            reason: Some("refusing symbolic link cleanup target".to_string()),
        };
    }
    if !metadata.is_dir() {
        return JanitorTargetResult {
            target: candidate.target.to_string(),
            path: candidate.path,
            size_bytes: None,
            outcome: JanitorOutcome::Skipped,
            reason: Some("cleanup target is not a directory".to_string()),
        };
    }

    let canonical = match fs::canonicalize(&candidate.path) {
        Ok(path) => path,
        Err(error) => return failed_result(candidate.target, candidate.path, None, &error),
    };
    if !canonical.starts_with(root) {
        return failed_result(
            candidate.target,
            candidate.path,
            None,
            &io::Error::other("candidate is not canonically contained in the project root"),
        );
    }

    let size = match dir_size(&candidate.path) {
        Ok(size) => size,
        Err(error) => return failed_result(candidate.target, candidate.path, None, &error),
    };
    info!(
        target = candidate.target,
        size_bytes = size,
        "Found cleanup target"
    );

    if dry_run {
        return JanitorTargetResult {
            target: candidate.target.to_string(),
            path: candidate.path,
            size_bytes: Some(size),
            outcome: JanitorOutcome::Reclaimable,
            reason: None,
        };
    }

    remove_candidate(root, candidate, size)
}

/// Re-check immediately before deletion so a target swapped for a symlink is
/// never removed by this operation.
fn remove_candidate(root: &Path, candidate: Candidate, size: u64) -> JanitorTargetResult {
    let deletion_metadata = match fs::symlink_metadata(&candidate.path) {
        Ok(metadata) => metadata,
        Err(error) => return failed_result(candidate.target, candidate.path, Some(size), &error),
    };
    if deletion_metadata.file_type().is_symlink() {
        return JanitorTargetResult {
            target: candidate.target.to_string(),
            path: candidate.path,
            size_bytes: Some(size),
            outcome: JanitorOutcome::Skipped,
            reason: Some("refusing symbolic link cleanup target".to_string()),
        };
    }
    if !deletion_metadata.is_dir() {
        return JanitorTargetResult {
            target: candidate.target.to_string(),
            path: candidate.path,
            size_bytes: Some(size),
            outcome: JanitorOutcome::Skipped,
            reason: Some("cleanup target is no longer a directory".to_string()),
        };
    }
    match fs::canonicalize(&candidate.path) {
        Ok(path) if path.starts_with(root) => {}
        Ok(_) => {
            return failed_result(
                candidate.target,
                candidate.path,
                Some(size),
                &io::Error::other("candidate is not canonically contained in the project root"),
            );
        }
        Err(error) => return failed_result(candidate.target, candidate.path, Some(size), &error),
    }
    match fs::remove_dir_all(&candidate.path) {
        Ok(()) => JanitorTargetResult {
            target: candidate.target.to_string(),
            path: candidate.path,
            size_bytes: Some(size),
            outcome: JanitorOutcome::Removed,
            reason: None,
        },
        Err(error) => failed_result(candidate.target, candidate.path, Some(size), &error),
    }
}

fn failed_result(
    target: impl Into<String>,
    path: PathBuf,
    size_bytes: Option<u64>,
    error: &io::Error,
) -> JanitorTargetResult {
    JanitorTargetResult {
        target: target.into(),
        path,
        size_bytes,
        outcome: JanitorOutcome::Failed,
        reason: Some(error.to_string()),
    }
}

fn validated_project_root(path: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_dir() && !metadata.file_type().is_symlink() {
        return Err("project root is not a directory".to_string());
    }
    let root = fs::canonicalize(path).map_err(|error| error.to_string())?;
    if !fs::metadata(&root)
        .map_err(|error| error.to_string())?
        .is_dir()
    {
        return Err("resolved project root is not a directory".to_string());
    }
    Ok(root)
}

/// Recursively calculate a directory's size without following symbolic links.
fn dir_size(path: &Path) -> io::Result<u64> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(io::Error::other("refusing to size symbolic link"));
    }
    if !metadata.is_dir() {
        return Ok(metadata.len());
    }

    let mut total = 0;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        let metadata = fs::symlink_metadata(&entry_path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            total += dir_size(&entry_path)?;
        } else {
            total += metadata.len();
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
        assert_eq!(dir_size(dir.path()).unwrap(), 0);
    }

    #[test]
    fn test_dir_size_with_file() {
        let dir = tempfile::TempDir::new().unwrap();
        fs::write(dir.path().join("test.txt"), "hello world").unwrap();
        assert_eq!(dir_size(dir.path()).unwrap(), 11);
    }

    #[cfg(unix)]
    #[test]
    fn test_dir_size_skips_nested_symlink() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        fs::write(outside.path().join("large.txt"), vec![0_u8; 128]).unwrap();
        fs::write(dir.path().join("inside.txt"), "safe").unwrap();
        symlink(outside.path(), dir.path().join("outside-link")).unwrap();

        assert_eq!(dir_size(dir.path()).unwrap(), 4);
    }
}
