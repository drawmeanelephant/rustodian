//! Deterministic project triage reports.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use rustodian_types::{LanguageDetection, Project, ProjectLog, VcsInfo};
use serde::{Deserialize, Serialize};

use crate::{CoreError, Custodian};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefReport {
    pub generated_at: DateTime<Utc>,
    pub project_count: usize,
    pub category_counts: BriefCounts,
    pub projects: Vec<ProjectBrief>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BriefCounts {
    pub needs_attention: usize,
    pub work_in_progress: usize,
    pub ready: usize,
    pub unverified: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BriefCategory {
    NeedsAttention,
    WorkInProgress,
    Ready,
    Unverified,
}

impl BriefCategory {
    pub fn heading(&self) -> &'static str {
        match self {
            Self::NeedsAttention => "Needs Attention",
            Self::WorkInProgress => "Work in Progress",
            Self::Ready => "Ready",
            Self::Unverified => "Unverified",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AttentionReason {
    MissingProjectPath,
    GitInspectionFailed {
        message: String,
    },
    LatestCommandFailed {
        command: String,
        exit_code: Option<i32>,
    },
    DirtyWorkingTree,
    NoCommandHistory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SuggestedAction {
    InspectLatestCommandFailure { command: String },
    RefreshTrackedProjects,
    InspectRepositoryGitState,
    ReviewUncommittedChanges,
    RunCommand { command: String },
}

impl SuggestedAction {
    pub fn text(&self) -> String {
        match self {
            Self::InspectLatestCommandFailure { command } => {
                format!("Inspect the latest {command} failure")
            }
            Self::RefreshTrackedProjects => "Run rustodian scan to refresh tracked projects".into(),
            Self::InspectRepositoryGitState => "Inspect repository Git state".into(),
            Self::ReviewUncommittedChanges => "Review uncommitted changes".into(),
            Self::RunCommand { command } => format!("Run rustodian run <project> {command}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectBrief {
    pub project_id: rustodian_types::ProjectId,
    pub name: String,
    pub path: PathBuf,
    pub languages: Vec<LanguageDetection>,
    pub live_vcs: Option<VcsInfo>,
    pub latest_command: Option<ProjectLog>,
    pub category: BriefCategory,
    pub attention_reasons: Vec<AttentionReason>,
    pub suggested_action: Option<SuggestedAction>,
}

impl Custodian {
    /// Build a bounded, non-persisted report from current Git state and logs.
    #[allow(clippy::too_many_lines)]
    pub fn brief(&self, projects: Vec<Project>) -> Result<BriefReport, CoreError> {
        let mut records = Vec::with_capacity(projects.len());
        for project in projects {
            let path_exists = project.path.exists();
            let (live_vcs, git_error) = if path_exists {
                match self.git_inspector().inspect(&project.path) {
                    Ok(vcs) => (vcs, None),
                    Err(error) => (None, Some(error.to_string())),
                }
            } else {
                (None, None)
            };

            let logs = self.store().list_logs(&project.id.to_string(), 50)?;
            let latest_command = logs
                .into_iter()
                .filter(|log| !log.command_name.trim().is_empty())
                .max_by_key(|log| log.run_at);

            let mut reasons = Vec::new();
            if !path_exists {
                reasons.push(AttentionReason::MissingProjectPath);
            }
            if let Some(message) = git_error {
                reasons.push(AttentionReason::GitInspectionFailed { message });
            }
            if let Some(log) = &latest_command
                && log.exit_code.is_some_and(|code| code != 0)
            {
                reasons.push(AttentionReason::LatestCommandFailed {
                    command: log.command_name.clone(),
                    exit_code: log.exit_code,
                });
            }

            let dirty = live_vcs.as_ref().is_some_and(|vcs| vcs.is_dirty);
            if dirty {
                reasons.push(AttentionReason::DirtyWorkingTree);
            }
            if latest_command.is_none() {
                reasons.push(AttentionReason::NoCommandHistory);
            }

            let category = if !path_exists
                || git_error_was_present(&reasons)
                || latest_command
                    .as_ref()
                    .is_some_and(|l| l.exit_code.is_some_and(|code| code != 0))
            {
                BriefCategory::NeedsAttention
            } else if dirty {
                BriefCategory::WorkInProgress
            } else if live_vcs.is_some()
                && latest_command
                    .as_ref()
                    .is_some_and(|l| l.exit_code == Some(0))
            {
                BriefCategory::Ready
            } else {
                BriefCategory::Unverified
            };

            let suggested_action = if let Some(log) = &latest_command {
                if log.exit_code.is_some_and(|code| code != 0) {
                    Some(SuggestedAction::InspectLatestCommandFailure {
                        command: log.command_name.clone(),
                    })
                } else if !path_exists {
                    Some(SuggestedAction::RefreshTrackedProjects)
                } else if git_error_was_present(&reasons) {
                    Some(SuggestedAction::InspectRepositoryGitState)
                } else if dirty {
                    Some(SuggestedAction::ReviewUncommittedChanges)
                } else {
                    None
                }
            } else if !path_exists {
                Some(SuggestedAction::RefreshTrackedProjects)
            } else if git_error_was_present(&reasons) {
                Some(SuggestedAction::InspectRepositoryGitState)
            } else if dirty {
                Some(SuggestedAction::ReviewUncommittedChanges)
            } else {
                ["test", "check", "verify"]
                    .iter()
                    .find_map(|name| {
                        project
                            .metadata
                            .commands
                            .iter()
                            .find(|command| command.name == *name)
                    })
                    .map(|command| SuggestedAction::RunCommand {
                        command: command.name.clone(),
                    })
            };

            records.push(ProjectBrief {
                project_id: project.id,
                name: project.name,
                path: project.path,
                languages: project.languages,
                live_vcs,
                latest_command,
                category,
                attention_reasons: reasons,
                suggested_action,
            });
        }

        let mut counts = BriefCounts::default();
        for record in &records {
            match record.category {
                BriefCategory::NeedsAttention => counts.needs_attention += 1,
                BriefCategory::WorkInProgress => counts.work_in_progress += 1,
                BriefCategory::Ready => counts.ready += 1,
                BriefCategory::Unverified => counts.unverified += 1,
            }
        }
        Ok(BriefReport {
            generated_at: Utc::now(),
            project_count: records.len(),
            category_counts: counts,
            projects: records,
        })
    }
}

fn git_error_was_present(reasons: &[AttentionReason]) -> bool {
    reasons
        .iter()
        .any(|reason| matches!(reason, AttentionReason::GitInspectionFailed { .. }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::CommandSpec;
    use crate::traits::{
        CommandRunner, DiscoveredProject, GitInspector, ProjectScanner, ProjectStore,
        RunningProcess,
    };
    use rustodian_types::{ProjectId, ScanConfig, ScanId, ScanRecord, VcsType};
    use std::path::{Path, PathBuf};

    struct Store {
        projects: Vec<Project>,
        logs: Vec<ProjectLog>,
    }
    impl ProjectStore for Store {
        fn save_project(&self, _: &Project) -> Result<ProjectId, CoreError> {
            Ok(ProjectId::new())
        }
        fn get_project(&self, id: &ProjectId) -> Result<Option<Project>, CoreError> {
            Ok(self.projects.iter().find(|p| &p.id == id).cloned())
        }
        fn list_projects(&self) -> Result<Vec<Project>, CoreError> {
            Ok(self.projects.clone())
        }
        fn delete_project(&self, _: &ProjectId) -> Result<bool, CoreError> {
            Ok(false)
        }
        fn find_by_path(&self, _: &Path) -> Result<Option<Project>, CoreError> {
            Ok(None)
        }
        fn save_scan(&self, _: &ScanRecord) -> Result<ScanId, CoreError> {
            Ok(ScanId::new())
        }
        fn get_latest_scan(&self) -> Result<Option<ScanRecord>, CoreError> {
            Ok(None)
        }
        fn save_log(&self, _: &ProjectLog) -> Result<(), CoreError> {
            Ok(())
        }
        fn list_logs(&self, id: &str, _: usize) -> Result<Vec<ProjectLog>, CoreError> {
            Ok(self
                .logs
                .iter()
                .filter(|log| log.project_id == id)
                .cloned()
                .collect())
        }
        fn get_log(&self, _: &str) -> Result<Option<ProjectLog>, CoreError> {
            Ok(None)
        }
        fn get_latest_log(&self, _: &str) -> Result<Option<ProjectLog>, CoreError> {
            Ok(None)
        }
        fn prune_logs(&self, _: &str, _: usize) -> Result<usize, CoreError> {
            Ok(0)
        }
    }
    struct Scanner;
    impl ProjectScanner for Scanner {
        fn scan(&self, _: &Path, _: &ScanConfig) -> Result<Vec<DiscoveredProject>, CoreError> {
            Ok(vec![])
        }
    }
    enum GitResult {
        Vcs(bool),
        Error,
    }
    struct Git(GitResult);
    impl GitInspector for Git {
        fn inspect(&self, path: &Path) -> Result<Option<VcsInfo>, CoreError> {
            match self.0 {
                GitResult::Vcs(dirty) => Ok(Some(VcsInfo {
                    vcs_type: VcsType::Git,
                    branch: Some("main".into()),
                    remote_url: None,
                    is_dirty: dirty,
                    last_commit: None,
                })),
                GitResult::Error if path.ends_with("broken") => {
                    Err(CoreError::Git("broken repository".into()))
                }
                GitResult::Error => Ok(None),
            }
        }
        fn get_dirty_files(&self, _: &Path) -> Result<Vec<PathBuf>, CoreError> {
            Ok(vec![])
        }
    }
    struct Runner;
    impl CommandRunner for Runner {
        fn spawn(&self, _: CommandSpec) -> Result<Box<dyn RunningProcess>, CoreError> {
            Err(CoreError::Internal("unused".into()))
        }
    }
    fn project(path: &Path) -> Project {
        Project {
            id: ProjectId::new(),
            name: "demo".into(),
            path: path.into(),
            languages: vec![],
            vcs: None,
            discovered_at: Utc::now(),
            last_scanned_at: None,
            metadata: rustodian_types::ProjectMetadata::default(),
        }
    }
    fn log(project: &Project, exit_code: Option<i32>) -> ProjectLog {
        ProjectLog {
            id: "log".into(),
            project_id: project.id.to_string(),
            command_name: "test".into(),
            exit_code,
            log_text: String::new(),
            run_at: Utc::now(),
        }
    }
    fn custodian(project: Project, logs: Vec<ProjectLog>, git: GitResult) -> Custodian {
        Custodian::new(
            Box::new(Store {
                projects: vec![project],
                logs,
            }),
            Box::new(Scanner),
            Box::new(Git(git)),
            Box::new(Runner),
        )
    }

    #[test]
    fn brief_categories_obey_precedence_and_empty_history() {
        let dir = tempfile::tempdir().unwrap();
        let failed = project(dir.path());
        let report = custodian(
            failed.clone(),
            vec![log(&failed, Some(1))],
            GitResult::Vcs(true),
        )
        .brief(vec![failed])
        .unwrap();
        assert_eq!(report.projects[0].category, BriefCategory::NeedsAttention);
        assert!(
            report.projects[0]
                .attention_reasons
                .iter()
                .any(|reason| matches!(reason, AttentionReason::LatestCommandFailed { .. }))
        );

        let dirty = project(dir.path());
        let report = custodian(dirty.clone(), vec![], GitResult::Vcs(true))
            .brief(vec![dirty])
            .unwrap();
        assert_eq!(report.projects[0].category, BriefCategory::WorkInProgress);

        let empty = project(dir.path());
        let report = custodian(empty.clone(), vec![], GitResult::Vcs(false))
            .brief(vec![empty])
            .unwrap();
        assert_eq!(report.projects[0].category, BriefCategory::Unverified);
    }

    #[test]
    fn git_error_is_project_scoped_and_json_is_structured() {
        let dir = tempfile::tempdir().unwrap();
        let broken_path = dir.path().join("broken");
        let healthy_path = dir.path().join("healthy");
        std::fs::create_dir_all(&broken_path).unwrap();
        std::fs::create_dir_all(&healthy_path).unwrap();
        let broken = project(&broken_path);
        let healthy = project(&healthy_path);
        let report = custodian(broken.clone(), vec![], GitResult::Error)
            .brief(vec![broken, healthy])
            .unwrap();
        assert_eq!(report.projects.len(), 2);
        assert!(matches!(
            report.projects[0].attention_reasons[0],
            AttentionReason::GitInspectionFailed { .. }
        ));
        assert_eq!(report.projects[1].category, BriefCategory::Unverified);
        let json = serde_json::to_value(report).unwrap();
        assert!(json["category_counts"]["needs_attention"].is_number());
        assert!(json["projects"][0]["attention_reasons"].is_array());
    }

    #[test]
    fn failed_latest_command_gets_failure_action() {
        let dir = tempfile::tempdir().unwrap();
        let p = project(dir.path());
        let report = custodian(p.clone(), vec![log(&p, Some(2))], GitResult::Vcs(false))
            .brief(vec![p])
            .unwrap();
        assert!(matches!(
            report.projects[0].suggested_action,
            Some(SuggestedAction::InspectLatestCommandFailure { .. })
        ));
    }

    #[test]
    fn missing_path_is_reported_without_inspection() {
        let p = project(Path::new("/definitely/not/a/rustodian/project"));
        let report = custodian(p.clone(), vec![], GitResult::Error)
            .brief(vec![p])
            .unwrap();
        assert_eq!(report.projects[0].category, BriefCategory::NeedsAttention);
        assert!(matches!(
            report.projects[0].suggested_action,
            Some(SuggestedAction::RefreshTrackedProjects)
        ));
    }
}
