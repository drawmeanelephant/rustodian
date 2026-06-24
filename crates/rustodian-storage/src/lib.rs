//! # Rustodian Storage
//!
//! SQLite-backed storage for Rustodian project data.
//!
//! This crate implements [`rustodian_core::ProjectStore`] using `rusqlite`.
//! It handles database initialization, migrations, and all persistence operations.

pub mod error;
pub mod migrations;
pub mod store;

pub use store::SqliteStore;
pub mod remote_store;
