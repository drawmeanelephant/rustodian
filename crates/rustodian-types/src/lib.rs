//! # Rustodian Types
//!
//! Shared data structures and enums for the Rustodian project observatory.
//! This crate contains pure data — no behavior, no traits, no I/O.

pub mod language;
pub mod project;
pub mod scan;
pub mod vcs;

// Re-export key types for convenience
pub use language::{DetectionConfidence, Language, LanguageDetection, LanguageMarker};
pub use project::RemoteProject;
pub use project::{Project, ProjectId, ProjectMetadata};
pub use scan::{ScanConfig, ScanId, ScanRecord, ScanStatus};
pub use vcs::{CommitInfo, VcsInfo, VcsType};
