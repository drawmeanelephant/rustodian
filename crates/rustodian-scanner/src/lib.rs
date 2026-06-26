//! # Rustodian Scanner
//!
//! Filesystem-based project discovery for Rustodian.
//!
//! Uses the `ignore` crate for `.gitignore`-aware directory traversal.
//! Detects projects by looking for language-specific marker files
//! (e.g., `Cargo.toml` for Rust, `package.json` for Node).

pub mod commands;
pub mod detection;
pub mod error;
pub mod scanner;

pub use scanner::FsScanner;
