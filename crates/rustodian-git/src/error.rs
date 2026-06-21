//! Git-specific error types.

use rustodian_core::CoreError;

/// Errors specific to git inspection.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// Error from libgit2.
    #[error("git2 error: {0}")]
    Git2(#[from] git2::Error),
}

impl From<GitError> for CoreError {
    fn from(err: GitError) -> Self {
        CoreError::Git(err.to_string())
    }
}
