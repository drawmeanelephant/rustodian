//! Language detection types.

use serde::{Deserialize, Serialize};

/// Languages that Rustodian can detect.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Rust,
    Python,
    Node,
    Go,
    Ruby,
    Zig,
    /// A language we detected but don't have first-class support for.
    Unknown(String),
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rust => write!(f, "Rust"),
            Self::Python => write!(f, "Python"),
            Self::Node => write!(f, "Node"),
            Self::Go => write!(f, "Go"),
            Self::Ruby => write!(f, "Ruby"),
            Self::Zig => write!(f, "Zig"),
            Self::Unknown(name) => write!(f, "{name}"),
        }
    }
}

/// A language detection result with confidence and evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageDetection {
    pub language: Language,
    pub confidence: DetectionConfidence,
    pub markers: Vec<LanguageMarker>,
}

/// How confident we are in a language detection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DetectionConfidence {
    /// Found a definitive marker (e.g., Cargo.toml for Rust).
    High,
    /// Found supporting evidence (e.g., .rs files but no Cargo.toml).
    Medium,
    /// Weak signal (e.g., file extension only).
    Low,
}

impl std::fmt::Display for DetectionConfidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::High => write!(f, "high"),
            Self::Medium => write!(f, "medium"),
            Self::Low => write!(f, "low"),
        }
    }
}

/// Evidence for why a language was detected.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguageMarker {
    /// Found a package manifest (e.g., Cargo.toml, package.json).
    ManifestFile(String),
    /// Found a lock file (e.g., Cargo.lock, yarn.lock).
    LockFile(String),
    /// Found a configuration file (e.g., .eslintrc, pyproject.toml).
    ConfigFile(String),
    /// Found source files with this extension.
    FileExtension(String),
}
