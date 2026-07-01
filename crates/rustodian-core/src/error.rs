//! Core domain errors.

use std::path::PathBuf;

use rustodian_types::ProjectId;

/// Errors that can occur in the Rustodian domain.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// A project was not found.
    #[error("project not found: {0}")]
    ProjectNotFound(ProjectId),

    /// A path was not found or inaccessible.
    #[error("path not found: {}", .0.display())]
    PathNotFound(PathBuf),

    /// A storage operation failed.
    #[error("storage error: {0}")]
    Storage(String),

    /// A scan operation failed.
    #[error("scan error: {0}")]
    Scan(String),

    /// A git operation failed.
    #[error("git error: {0}")]
    Git(String),

    /// Rate limit exceeded on a remote API.
    #[error("API rate limit exceeded")]
    RateLimitExceeded,

    /// An unexpected internal error.
    #[error("internal error: {0}")]
    Internal(String),
}
