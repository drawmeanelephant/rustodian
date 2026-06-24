use thiserror::Error;
#[derive(Error, Debug)]
pub enum RemoteError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Archive extraction error: {0}")]
    Extraction(String),
    #[error("Not found: {0}")]
    NotFound(String),
}
