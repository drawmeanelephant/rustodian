use crate::OutputFormat;
use anyhow::{Context, Result};
use rustodian_core::traits::{RemoteDownloader, RemoteProjectStore};
use rustodian_remote::GithubDownloader;
use rustodian_storage::SqliteStore;
use rustodian_types::RemoteProject;
use tokio::runtime::Runtime;
use tracing::info;

pub fn execute_add(store: &SqliteStore, repo_slug: &str, preserve: &[String]) -> Result<()> {
    let project = RemoteProject {
        repo_slug: repo_slug.to_string(),
        preserve_patterns: preserve.to_vec(),
    };
    store
        .save_remote_project(&project)
        .context("failed to save remote project")?;
    info!("Added remote project {}", repo_slug);
    println!("Added remote project: {repo_slug}");
    Ok(())
}

pub fn execute_list(store: &SqliteStore, format: &OutputFormat) -> Result<()> {
    let projects = store
        .list_remote_projects()
        .context("failed to list remote projects")?;
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&projects)?);
        }
        OutputFormat::Table => {
            if projects.is_empty() {
                println!("No remote projects tracked.");
                return Ok(());
            }
            let mut table = comfy_table::Table::new();
            table.set_header(vec!["Repo Slug", "Preserve Patterns"]);
            for p in projects {
                let patterns = if p.preserve_patterns.is_empty() {
                    "(none)".to_string()
                } else {
                    p.preserve_patterns.join(", ")
                };
                table.add_row(vec![p.repo_slug, patterns]);
            }
            println!("{table}");
        }
    }
    Ok(())
}

pub fn execute_refresh(store: &SqliteStore, dest_dir: &std::path::Path) -> Result<()> {
    let projects = store
        .list_remote_projects()
        .context("failed to list remote projects")?;
    if projects.is_empty() {
        println!("No remote projects to refresh.");
        return Ok(());
    }
    let downloader = GithubDownloader::new();
    let rt = Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        for project in projects {
            println!("Refreshing {}...", project.repo_slug);
            match downloader
                .download_and_extract(&project, dest_dir, &project.preserve_patterns)
                .await
            {
                Ok(()) => println!("Successfully refreshed {}", project.repo_slug),
                Err(e) => println!("Failed to refresh {}: {}", project.repo_slug, e),
            }
        }
    });
    Ok(())
}
