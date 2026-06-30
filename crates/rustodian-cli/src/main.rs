//! # Rustodian CLI

//!
//! Department of Project Custodianship 🏛️
//!
//! Command-line entry point for the Rustodian project observatory.
//! This is the composition root — it wires infrastructure implementations
//! to the core orchestrator and dispatches CLI commands.

mod commands;
mod output;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{ArgAction, Parser, Subcommand, ValueEnum};
use tracing::info;

use rustodian_core::Custodian;
use rustodian_git::Git2Inspector;
use rustodian_scanner::FsScanner;
use rustodian_storage::SqliteStore;

/// Rustodian: Department of Project Custodianship 🏛️
///
/// A personal project observatory that discovers, indexes,
/// and monitors your software projects.
#[derive(Parser)]
#[command(name = "rustodian", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output format
    #[arg(long, alias = "output", global = true, default_value = "table")]
    format: OutputFormat,

    /// Verbosity level (-v, -vv, -vvv)
    #[arg(short, long, global = true, action = ArgAction::Count)]
    verbose: u8,

    /// Path to database file
    #[arg(long, global = true, env = "RUSTODIAN_DB")]
    db: Option<PathBuf>,
}

/// Available output formats.
#[derive(Debug, Clone, ValueEnum)]
enum OutputFormat {
    Table,
    Json,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan a directory tree for software projects
    Scan {
        /// Root directory to scan
        #[arg(short, long, env = "RUSTODIAN_SCAN_ROOT", default_value = ".")]
        path: PathBuf,

        /// Maximum directory depth
        #[arg(long, default_value_t = rustodian_types::scan::DEFAULT_MAX_DEPTH)]
        max_depth: usize,
    },

    /// List all tracked projects
    List {
        /// Filter by language
        #[arg(long)]
        language: Option<String>,
    },

    /// Show observatory status summary
    Status,

    /// Manage remote GitHub projects
    Remote {
        #[command(subcommand)]
        command: RemoteCommands,
    },

    /// Show detailed info about a specific project
    Info {
        /// Project name or ID
        project: String,
    },

    /// Run a discovered command for a project
    Run {
        /// Project name or ID
        project: String,
        /// Command name to run
        command: String,
    },

    /// View logs for a project
    Logs {
        /// Project name or ID
        project: String,

        /// Limit number of logs shown
        #[arg(long, default_value = "10")]
        limit: usize,
    },

    /// Print active configuration
    Config,
}

#[derive(Subcommand)]
enum RemoteCommands {
    /// Add a remote project to track
    Add {
        /// GitHub repository slug (e.g., username/repo)
        repo_slug: String,

        /// Glob patterns of files to preserve during refresh
        #[arg(long, value_delimiter = ',')]
        preserve: Vec<String>,
    },

    /// List tracked remote projects
    List,

    /// Refresh (download) all tracked remote projects
    Refresh {
        /// Destination directory
        #[arg(long, default_value = ".")]
        dest: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing based on verbosity
    output::init_tracing(cli.verbose);

    info!("Rustodian starting");

    // Wire up infrastructure
    let db_path = match cli.db {
        Some(path) => path,
        None => SqliteStore::default_path().context("failed to determine database path")?,
    };

    let store = SqliteStore::open(&db_path).context("failed to open database")?;
    store.migrate().context("failed to run migrations")?;

    let scanner = FsScanner;
    let git = Git2Inspector;

    let runner = rustodian_core::runner::DefaultCommandRunner;
    let custodian = Custodian::new(
        Box::new(store.clone()),
        Box::new(scanner),
        Box::new(git),
        Box::new(runner),
    );

    // Dispatch command
    match cli.command {
        Commands::Scan { path, max_depth } => {
            commands::scan::execute(&custodian, &path, max_depth, &cli.format)
        }
        Commands::List { language } => {
            commands::list::execute(&custodian, language.as_deref(), &cli.format)
        }
        Commands::Status => commands::status::execute(&custodian, &cli.format),
        Commands::Remote { command } => match command {
            RemoteCommands::Add {
                repo_slug,
                preserve,
            } => commands::remote::execute_add(&store, &repo_slug, &preserve),
            RemoteCommands::List => commands::remote::execute_list(&store, &cli.format),
            RemoteCommands::Refresh { dest } => {
                commands::remote::execute_refresh(&custodian, &store, &dest)
            }
        },
        Commands::Info { project } => commands::info::execute(&custodian, &project, &cli.format),
        Commands::Run { project, command } => {
            commands::run::execute(&custodian, &project, &command)
        }
        Commands::Logs { project, limit } => {
            commands::logs::execute(&custodian, &store, &project, limit, &cli.format)
        }
        Commands::Config => commands::config::execute(&db_path, &cli.format),
    }
}
