//! # Rustodian Scanner
//!
//! Filesystem-based project discovery for Rustodian.
//!
//! Uses the `ignore` crate for `.gitignore`-aware directory traversal.
//! Detects projects by looking for language-specific marker files
//! (e.g., `Cargo.toml` for Rust, `package.json` for Node) and, independently
//! of language, project-root markers such as Cloudflare Wrangler config files
//! (`wrangler.jsonc`, `wrangler.json`, `wrangler.toml`).

pub mod commands;
pub mod detection;
pub mod error;
pub mod scanner;

pub use scanner::FsScanner;
