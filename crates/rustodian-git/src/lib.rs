//! # Rustodian Git
//!
//! Git repository inspection for Rustodian.
//!
//! Uses `git2` (libgit2 bindings) to extract repository information
//! without requiring a `git` binary on the system.

pub mod error;
pub mod inspector;

pub use inspector::Git2Inspector;
