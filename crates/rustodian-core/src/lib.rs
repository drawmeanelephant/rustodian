//! # Rustodian Core
//!
//! Domain logic, trait definitions, and orchestration for Rustodian.
//!
//! This crate defines the contracts that infrastructure crates must implement.
//! It has **zero knowledge** of `SQLite`, filesystems, or git — those are
//! implementation details provided by other crates.
//!
//! ## Architecture
//!
//! - [`traits`] — The contracts: `ProjectStore`, `ProjectScanner`, `GitInspector`
//! - [`custodian`] — The orchestrator that wires everything together
//! - [`error`] — Domain error types

pub mod bootstrapper;
pub mod brief;
pub mod custodian;
pub mod error;
pub mod janitor;
pub mod log_buffer;
pub mod runner;
pub mod traits;

pub use bootstrapper::ProjectBootstrapper;
pub use brief::{
    AttentionReason, BriefCategory, BriefCounts, BriefReport, ProjectBrief, SuggestedAction,
};
pub use custodian::{Custodian, PruneOutcome, PruneProjectResult, PruneReport};
pub use error::CoreError;
pub use janitor::DigitalJanitor;
pub use log_buffer::LogBuffer;
pub use traits::{GitInspector, ProjectScanner, ProjectStore};
