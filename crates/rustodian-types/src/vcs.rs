//! Version control system types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Information about a project's version control.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VcsInfo {
    pub vcs_type: VcsType,
    pub branch: Option<String>,
    pub remote_url: Option<String>,
    pub is_dirty: bool,
    pub last_commit: Option<CommitInfo>,
}

/// Supported version control systems.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VcsType {
    Git,
}

impl std::fmt::Display for VcsType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Git => write!(f, "Git"),
        }
    }
}

/// Information about a specific commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    pub sha: String,
    pub message: String,
    pub author: String,
    pub timestamp: DateTime<Utc>,
}
