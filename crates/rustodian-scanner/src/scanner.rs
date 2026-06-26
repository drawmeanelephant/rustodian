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
        let mut projects = Vec::new();

        let mut builder = ignore::WalkBuilder::new(root);
        builder.max_depth(Some(config.max_depth));
        builder.follow_links(config.follow_symlinks);
        
        // Exclude directories that shouldn't be searched (e.g. .git is already ignored by default)
        
        let walker = builder.build();
        
        for result in walker {
            let entry = match result {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("Error reading directory entry: {}", e);
                    continue;
                }
            };
            
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            
            let languages = crate::detection::detect_languages(path);
            if !languages.is_empty() {
                let name = path
                    .file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new("unknown"))
                    .to_string_lossy()
                    .to_string();
                    
                let commands = crate::commands::CommandDiscoverer::discover(path);
                    
                projects.push(DiscoveredProject {
                    name,
                    path: path.to_path_buf(),
                    languages,
                    commands,
                });
            }
        }
        
        Ok(projects)
    }
}

#[cfg(test)]
mod tests {
    // Future: tests with tempdir fixtures containing marker files
}
