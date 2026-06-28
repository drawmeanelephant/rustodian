//! Project domain types.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::language::LanguageDetection;
use crate::vcs::VcsInfo;

/// Opaque project identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectId(pub Uuid);

impl ProjectId {
    /// Create a new random project ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ProjectId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ProjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A discovered software project on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub path: PathBuf,
    pub languages: Vec<LanguageDetection>,
    pub vcs: Option<VcsInfo>,
    pub discovered_at: DateTime<Utc>,
    pub last_scanned_at: Option<DateTime<Utc>>,
    pub metadata: ProjectMetadata,
}

/// A runnable command discovered in a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCommand {
    pub name: String,
    pub description: Option<String>,
    pub command: String,
    pub source: String, // e.g., "Cargo.toml", "package.json", "justfile"
    #[serde(default)]
    pub use_shell: bool,
}

/// Extensible metadata bag.
///
/// Uses `serde(flatten)` with a JSON value to allow future fields
/// without requiring database schema migrations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectMetadata {
    pub description: Option<String>,
    pub tags: Vec<String>,
    #[serde(default)]
    pub commands: Vec<ProjectCommand>,
    /// Catch-all for future fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RemoteProject {
    pub repo_slug: String,
    pub preserve_patterns: Vec<String>,
}
