//! Storage-specific error types.

use rustodian_core::CoreError;

/// Errors specific to the SQLite storage implementation.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// SQLite error.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Data serialization/deserialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Migration error.
    #[error("migration error: {0}")]
    Migration(String),
}

impl From<StorageError> for CoreError {
    fn from(err: StorageError) -> Self {
        CoreError::Storage(err.to_string())
    }
}
