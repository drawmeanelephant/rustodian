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

        let mut builder = ignore::WalkBuilder::new(root);
        builder.max_depth(Some(config.max_depth));
        builder.follow_links(config.follow_symlinks);

        // Apply user-specified exclude patterns as override rules.
        if !config.exclude_patterns.is_empty() {
            let mut overrides = ignore::overrides::OverrideBuilder::new(root);
            for pattern in &config.exclude_patterns {
                let negated = format!("!{pattern}");
                if let Err(e) = overrides.add(&negated) {
                    tracing::warn!("Invalid exclude pattern '{pattern}': {e}");
                }
            }
            match overrides.build() {
                Ok(built) => {
                    builder.overrides(built);
                }
                Err(e) => {
                    tracing::warn!("Failed to build override rules: {e}");
                }
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
                            return ignore::WalkState::Continue;
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
    // Future: tests with tempdir fixtures containing marker files
}
