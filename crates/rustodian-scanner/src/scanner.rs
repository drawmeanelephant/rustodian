//! Filesystem scanner implementation.

use std::path::Path;

use tracing::{debug, instrument};

use rustodian_core::CoreError;
use rustodian_core::traits::{DiscoveredProject, ProjectScanner};
use rustodian_types::ScanConfig;

/// Filesystem-based project scanner.
///
/// Walks directory trees using the `ignore` crate (respects `.gitignore`)
/// and detects software projects by looking for marker files.
#[derive(Debug, Default)]
pub struct FsScanner;

impl ProjectScanner for FsScanner {
    #[instrument(skip(self), fields(root = %root.display()))]
    fn scan(&self, root: &Path, config: &ScanConfig) -> Result<Vec<DiscoveredProject>, CoreError> {
        debug!(max_depth = config.max_depth, "Starting filesystem scan");

        if config.max_depth == 0 {
            tracing::warn!(
                "ScanConfig::max_depth is 0. Returning empty results as this is treated as 'no traversal'."
            );
            return Ok(vec![]);
        }

        let mut builder = ignore::WalkBuilder::new(root);
        builder.max_depth(Some(config.max_depth));
        builder.follow_links(config.follow_symlinks);

        // Apply user-specified exclude patterns using globset.
        if !config.exclude_patterns.is_empty() {
            let mut gsb = globset::GlobSetBuilder::new();
            for pat in &config.exclude_patterns {
                if let Ok(glob) = globset::Glob::new(pat) {
                    gsb.add(glob);
                } else {
                    tracing::warn!("Invalid exclude pattern '{pat}'");
                }
            }
            if let Ok(excl) = gsb.build() {
                builder.filter_entry(move |e| !excl.is_match(e.path()));
            } else {
                tracing::warn!("Failed to build exclude globset");
            }
        }

        // Use parallel walking for better performance on large trees.
        builder.threads(0); // auto-detect CPU count

        let projects: std::sync::Arc<std::sync::Mutex<Vec<DiscoveredProject>>> =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let project_roots: std::sync::Arc<
            std::sync::Mutex<std::collections::HashSet<std::path::PathBuf>>,
        > = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));

        let walker = builder.build_parallel();
        walker.run(|| {
            let projects = std::sync::Arc::clone(&projects);
            let project_roots = std::sync::Arc::clone(&project_roots);
            Box::new(move |result| {
                let entry = match result {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::warn!("Error reading directory entry: {e}");
                        return ignore::WalkState::Continue;
                    }
                };

                let path = entry.path();
                if !path.is_dir() {
                    return ignore::WalkState::Continue;
                }

                // Skip if this directory is a child of an already-discovered
                // project root. This prevents detecting nested sub-projects
                // (e.g. a workspace member inside a Cargo workspace root).
                {
                    let roots = project_roots
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    for root in roots.iter() {
                        if path.starts_with(root) && path != root {
                            return ignore::WalkState::Skip;
                        }
                    }
                }

                let languages = crate::detection::detect_languages(path);
                if !languages.is_empty() {
                    let name = path
                        .file_name()
                        .unwrap_or_else(|| std::ffi::OsStr::new("unknown"))
                        .to_string_lossy()
                        .to_string();

                    let commands = crate::commands::CommandDiscoverer::discover(path);

                    if let Ok(mut projs) = projects.lock() {
                        projs.push(DiscoveredProject {
                            name,
                            path: path.to_path_buf(),
                            languages,
                            commands,
                        });
                    }

                    // Record this as a project root so children are skipped.
                    if let Ok(mut roots) = project_roots.lock() {
                        roots.insert(path.to_path_buf());
                    }

                    // Skip descending into this directory's children.
                    return ignore::WalkState::Skip;
                }

                ignore::WalkState::Continue
            })
        });

        let mut projects = match std::sync::Arc::try_unwrap(projects) {
            Ok(mutex) => mutex
                .into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            Err(arc) => arc
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone(),
        };

        // Sort by path for deterministic output regardless of walk order.
        projects.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(projects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use tempfile::tempdir;

    #[test]
    fn test_scanner_basic_and_exclusions() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create project A (Rust project)
        let proj_a = root.join("project_a");
        fs::create_dir_all(&proj_a).unwrap();
        File::create(proj_a.join("Cargo.toml")).unwrap();

        // Create project B (Python project)
        let proj_b = root.join("project_b");
        fs::create_dir_all(&proj_b).unwrap();
        File::create(proj_b.join("requirements.txt")).unwrap();

        // Create excluded folder
        let excl_dir = root.join("excluded_folder");
        fs::create_dir_all(&excl_dir).unwrap();
        File::create(excl_dir.join("Cargo.toml")).unwrap();

        let scanner = FsScanner;

        // Scan without exclusions
        let config_no_excl = ScanConfig {
            max_depth: 3,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };
        let projs = scanner.scan(root, &config_no_excl).unwrap();
        assert_eq!(projs.len(), 3);

        // Scan with exclusions
        let config_excl = ScanConfig {
            max_depth: 3,
            follow_symlinks: false,
            exclude_patterns: vec!["**/excluded_folder".to_string()],
        };
        let projs_excl = scanner.scan(root, &config_excl).unwrap();
        assert_eq!(projs_excl.len(), 2);
        assert_eq!(projs_excl[0].name, "project_a");
        assert_eq!(projs_excl[1].name, "project_b");
    }

    #[test]
    fn test_scanner_nested_skipping() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create parent project (Rust project)
        let parent_proj = root.join("parent_proj");
        fs::create_dir_all(&parent_proj).unwrap();
        File::create(parent_proj.join("Cargo.toml")).unwrap();

        // Create nested project inside parent (Node project)
        let nested_proj = parent_proj.join("nested_node_proj");
        fs::create_dir_all(&nested_proj).unwrap();
        File::create(nested_proj.join("package.json")).unwrap();

        let scanner = FsScanner;
        let config = ScanConfig {
            max_depth: 5,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };
        let projs = scanner.scan(root, &config).unwrap();
        
        // It should only find "parent_proj" and skip descending into "nested_node_proj"
        assert_eq!(projs.len(), 1);
        assert_eq!(projs[0].name, "parent_proj");
    }
}
