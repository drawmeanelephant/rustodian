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

pub mod custodian;
pub mod error;
pub mod traits;

pub use custodian::Custodian;
pub use error::CoreError;
pub use traits::{GitInspector, ProjectScanner, ProjectStore};
