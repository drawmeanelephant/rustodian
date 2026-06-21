//! Git2-based implementation of [`GitInspector`].

use std::path::Path;

use tracing::{debug, instrument};

use rustodian_core::CoreError;
use rustodian_core::traits::GitInspector;
use rustodian_types::VcsInfo;

/// Git inspector using libgit2.
#[derive(Debug, Default)]
pub struct Git2Inspector;

impl GitInspector for Git2Inspector {
    #[instrument(skip(self), fields(path = %project_path.display()))]
    fn inspect(&self, project_path: &Path) -> Result<Option<VcsInfo>, CoreError> {
        debug!("Inspecting git repository");
        let _ = project_path;
        todo!("Open repo with git2, extract branch/remote/dirty/last commit")
    }
}

#[cfg(test)]
mod tests {
    // Future: tests with tempdir git repos
}
