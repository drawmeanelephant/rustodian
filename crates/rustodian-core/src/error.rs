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

    /// A discovered command exited with a nonzero status.
    #[error("command '{command_name}' failed with exit code {exit_code}")]
    CommandFailed {
        /// Name of the discovered command that failed.
        command_name: String,
        /// The nonzero exit code reported by the child process.
        exit_code: i32,
    },

    /// A discovered command was terminated without reporting an exit code.
    #[error("command '{command_name}' was terminated without reporting an exit code")]
    CommandTerminated {
        /// Name of the discovered command that was terminated.
        command_name: String,
    },

    /// Rate limit exceeded on a remote API.
    #[error("API rate limit exceeded")]
    RateLimitExceeded,

    /// An unexpected internal error.
    #[error("internal error: {0}")]
    Internal(String),
}
