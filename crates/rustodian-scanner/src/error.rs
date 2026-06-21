//! Scanner-specific error types.

use std::path::PathBuf;

use rustodian_core::CoreError;

/// Errors specific to filesystem scanning.
#[derive(Debug, thiserror::Error)]
pub enum ScannerError {
    /// IO error during filesystem traversal.
    #[error("io error at {}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    /// The scan root doesn't exist or isn't a directory.
    #[error("scan root is not a directory: {}", .0.display())]
    NotADirectory(PathBuf),
}

impl From<ScannerError> for CoreError {
    fn from(err: ScannerError) -> Self {
        CoreError::Scan(err.to_string())
    }
}
