use crate::OutputFormat;
use anyhow::{Context, Result};
use rustodian_core::Custodian;
use rustodian_core::traits::{RemoteDownloader, RemoteProjectStore};
use rustodian_remote::GithubDownloader;
use rustodian_storage::SqliteStore;
use rustodian_types::{RemoteProject, ScanConfig};
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

pub fn execute_refresh(
    custodian: &Custodian,
    store: &SqliteStore,
    dest_dir: &std::path::Path,
) -> Result<()> {
    let projects = store
        .list_remote_projects()
        .context("failed to list remote projects")?;
    if projects.is_empty() {
        println!("No remote projects to refresh.");
        return Ok(());
    }
    let downloader = GithubDownloader::new();
    let rt = Runtime::new().context("failed to create tokio runtime")?;
    
    for project in projects {
        println!("Refreshing {}...", project.repo_slug);
        let project_dest = dest_dir.join(&project.repo_slug);
        let download_res = rt.block_on(async {
            downloader
                .download_and_extract(&project, &project_dest, &project.preserve_patterns)
                .await
        });

        match download_res {
            Ok(()) => {
                println!("Successfully refreshed {}", project.repo_slug);
                println!("Scanning project {}...", project.repo_slug);
                let scan_config = ScanConfig {
                    max_depth: 3,
                    follow_symlinks: false,
                    exclude_patterns: vec![],
                };
                match custodian.scan(&project_dest, &scan_config) {
                    Ok(report) => {
                        println!("Scan completed. Found {} projects.", report.projects_found);
                        match custodian.find_by_path(&project_dest) {
                            Ok(Some(proj)) => {
                                println!("Bootstrapping and verifying project {}...", proj.name);
                                match custodian.bootstrap_and_verify(&proj.id) {
                                    Ok(()) => println!("Successfully bootstrapped and verified {}!", proj.name),
                                    Err(e) => println!("Failed to bootstrap and verify {}: {}", proj.name, e),
                                }
                            }
                            Ok(None) => {
                                println!("Could not find the project in database by path: {}", project_dest.display());
                            }
                            Err(e) => {
                                println!("Failed to query project by path: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        println!("Failed to scan project {}: {}", project.repo_slug, e);
                    }
                }
            }
            Err(e) => println!("Failed to refresh {}: {}", project.repo_slug, e),
        }
    }
    Ok(())
}
