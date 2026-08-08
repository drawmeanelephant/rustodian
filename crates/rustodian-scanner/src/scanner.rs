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

        let walker = builder.build_parallel();
        walker.run(|| {
            let projects = std::sync::Arc::clone(&projects);
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

                // Collect every candidate project root. We deliberately keep
                // descending into project directories: an independently
                // managed repository nested inside another project (its own
                // `.git`, or a `.rustodian.toml`) must still be discovered.
                // Suppression of ordinary nested workspace/package directories
                // is decided in a deterministic post-processing pass below,
                // never by traversal order.
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

        // A nested project is only reported independently when it carries a
        // strong independence signal: its own `.git` (directory or file, e.g.
        // a worktree) or a `.rustodian.toml` config. A candidate nested inside
        // another candidate without such a signal is an ordinary workspace or
        // package member and remains suppressed beneath the parent project.
        // The scan root itself and candidates with no ancestor project are
        // always reported.
        let roots: std::collections::HashSet<std::path::PathBuf> =
            projects.iter().map(|p| p.path.clone()).collect();
        projects.retain(|p| {
            let mut ancestor = p.path.parent();
            while let Some(dir) = ancestor {
                if roots.contains(dir) {
                    return has_independence_signal(&p.path);
                }
                ancestor = dir.parent();
            }
            true
        });

        Ok(projects)
    }
}

/// Whether a directory carries a strong signal that it is independently
/// managed: its own `.git` (directory or file, e.g. a worktree) or a
/// `.rustodian.toml` config. Cheap filesystem checks only — no git commands
/// are invoked.
fn has_independence_signal(dir: &Path) -> bool {
    dir.join(".git").exists() || dir.join(".rustodian.toml").exists()
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_scanner_symlink_loop() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let a = root.join("a");
        let b = root.join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&b, a.join("link_to_b")).unwrap();
            std::os::unix::fs::symlink(&a, b.join("link_to_a")).unwrap();
        }

        File::create(a.join("Cargo.toml")).unwrap();

        let scanner = FsScanner;
        let config = ScanConfig {
            max_depth: 5,
            follow_symlinks: true,
            exclude_patterns: vec![],
        };

        let projs = scanner.scan(root, &config).unwrap();
        assert!(!projs.is_empty());
    }

    #[test]
    fn test_scanner_no_read_permissions() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let proj = root.join("my_proj");
        fs::create_dir_all(&proj).unwrap();
        File::create(proj.join("Cargo.toml")).unwrap();

        let unreadable = root.join("unreadable");
        fs::create_dir_all(&unreadable).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();
        }

        let scanner = FsScanner;
        let config = ScanConfig {
            max_depth: 3,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };
        let projs = scanner.scan(root, &config).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o755)).unwrap();
        }

        assert_eq!(projs.len(), 1);
        assert_eq!(projs[0].name, "my_proj");
    }

    #[test]
    fn test_scanner_malformed_manifest() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let proj = root.join("multi_proj");
        fs::create_dir_all(&proj).unwrap();
        File::create(proj.join("Cargo.toml")).unwrap();
        File::create(proj.join("package.json")).unwrap();

        let scanner = FsScanner;
        let config = ScanConfig {
            max_depth: 3,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };
        let projs = scanner.scan(root, &config).unwrap();

        assert_eq!(projs.len(), 1);
        assert_eq!(projs[0].name, "multi_proj");
        assert_eq!(projs[0].languages.len(), 2);
    }
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
    fn test_scanner_workspace_member_remains_suppressed() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Parent project (Rust workspace root)
        let app = root.join("app");
        fs::create_dir_all(&app).unwrap();
        File::create(app.join("Cargo.toml")).unwrap();

        // Ordinary workspace member: no independence signal, so it stays
        // suppressed beneath the already-discovered parent project.
        let member = app.join("crates/foo");
        fs::create_dir_all(&member).unwrap();
        File::create(member.join("Cargo.toml")).unwrap();

        let scanner = FsScanner;
        let config = ScanConfig {
            max_depth: 6,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };
        let projs = scanner.scan(root, &config).unwrap();

        assert_eq!(projs.len(), 1);
        assert_eq!(projs[0].name, "app");
    }

    #[test]
    fn test_scanner_nested_git_repo_survives() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Parent project
        let corpus = root.join("corpus");
        fs::create_dir_all(&corpus).unwrap();
        File::create(corpus.join("Cargo.toml")).unwrap();

        // Independently managed nested repo: has its own `.git` directory.
        let nested = corpus.join("filed.fyi");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir_all(nested.join(".git")).unwrap();
        File::create(nested.join("package.json")).unwrap();

        let scanner = FsScanner;
        let config = ScanConfig {
            max_depth: 6,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };
        let projs = scanner.scan(root, &config).unwrap();

        assert_eq!(projs.len(), 2);
        assert_eq!(projs[0].name, "corpus");
        assert_eq!(projs[1].name, "filed.fyi");
    }

    #[test]
    fn test_scanner_nested_git_file_survives() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Parent project
        let corpus = root.join("corpus");
        fs::create_dir_all(&corpus).unwrap();
        File::create(corpus.join("Cargo.toml")).unwrap();

        // Nested repo using a `.git` file (e.g. a worktree or submodule).
        let nested = corpus.join("worktree");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join(".git"), "gitdir: /some/where").unwrap();
        File::create(nested.join("package.json")).unwrap();

        let scanner = FsScanner;
        let config = ScanConfig {
            max_depth: 6,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };
        let projs = scanner.scan(root, &config).unwrap();

        assert_eq!(projs.len(), 2);
        assert_eq!(projs[0].name, "corpus");
        assert_eq!(projs[1].name, "worktree");
    }

    #[test]
    fn test_scanner_nested_rustodian_managed_survives() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Parent project
        let app = root.join("app");
        fs::create_dir_all(&app).unwrap();
        File::create(app.join("Cargo.toml")).unwrap();

        // Explicitly Rustodian-managed nested project: `.rustodian.toml`.
        let widget = app.join("tools/widget");
        fs::create_dir_all(&widget).unwrap();
        fs::write(widget.join(".rustodian.toml"), "[commands]\n").unwrap();
        File::create(widget.join("package.json")).unwrap();

        let scanner = FsScanner;
        let config = ScanConfig {
            max_depth: 6,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };
        let projs = scanner.scan(root, &config).unwrap();

        assert_eq!(projs.len(), 2);
        assert_eq!(projs[0].name, "app");
        assert_eq!(projs[1].name, "widget");
    }

    #[test]
    fn test_scanner_sibling_projects_both_survive() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        for name in ["alpha", "beta"] {
            let proj = root.join(name);
            fs::create_dir_all(&proj).unwrap();
            File::create(proj.join("Cargo.toml")).unwrap();
        }

        let scanner = FsScanner;
        let config = ScanConfig {
            max_depth: 3,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };
        let projs = scanner.scan(root, &config).unwrap();

        assert_eq!(projs.len(), 2);
        assert_eq!(projs[0].name, "alpha");
        assert_eq!(projs[1].name, "beta");
    }

    #[test]
    fn test_scanner_deeply_nested_independent_repo_survives() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Parent project
        let corpus = root.join("corpus");
        fs::create_dir_all(&corpus).unwrap();
        File::create(corpus.join("Cargo.toml")).unwrap();

        // Deeply nested independently managed repo.
        let nested = corpus.join("a/b/c/d/repo");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir_all(nested.join(".git")).unwrap();
        File::create(nested.join("go.mod")).unwrap();

        let scanner = FsScanner;
        let config = ScanConfig {
            max_depth: 8,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };
        let projs = scanner.scan(root, &config).unwrap();

        assert_eq!(projs.len(), 2);
        assert_eq!(projs[0].name, "corpus");
        assert_eq!(projs[1].name, "repo");
    }

    #[test]
    fn test_scanner_output_deterministic_across_runs() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let corpus = root.join("corpus");
        fs::create_dir_all(&corpus).unwrap();
        File::create(corpus.join("Cargo.toml")).unwrap();

        // Independently managed nested repo.
        let nested = corpus.join("filed.fyi");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir_all(nested.join(".git")).unwrap();
        File::create(nested.join("package.json")).unwrap();

        // Ordinary workspace member, exercising the suppression path.
        let member = corpus.join("crates/foo");
        fs::create_dir_all(&member).unwrap();
        File::create(member.join("Cargo.toml")).unwrap();

        let scanner = FsScanner;
        let config = ScanConfig {
            max_depth: 6,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };

        let first = scanner.scan(root, &config).unwrap();
        let second = scanner.scan(root, &config).unwrap();

        // `DiscoveredProject` has no `PartialEq`; key fields determine each
        // entry, so compare on (name, path).
        let keyed = |projs: &[DiscoveredProject]| {
            projs
                .iter()
                .map(|p| (p.name.clone(), p.path.clone()))
                .collect::<Vec<_>>()
        };

        // Identical output across runs, sorted by path.
        let mut sorted = keyed(&first);
        sorted.sort();
        assert_eq!(keyed(&first), sorted);
        assert_eq!(keyed(&first), keyed(&second));
    }

    #[test]
    fn test_scanner_nested_repo_direct_scan_equivalent() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let corpus = root.join("corpus");
        fs::create_dir_all(&corpus).unwrap();
        File::create(corpus.join("Cargo.toml")).unwrap();

        let nested = corpus.join("filed.fyi");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir_all(nested.join(".git")).unwrap();
        fs::write(
            nested.join("package.json"),
            r#"{"scripts": {"dev": "vite"}}"#,
        )
        .unwrap();

        let scanner = FsScanner;
        let config = ScanConfig {
            max_depth: 6,
            follow_symlinks: false,
            exclude_patterns: vec![],
        };

        let from_parent = scanner.scan(root, &config).unwrap();
        let nested_from_parent = from_parent
            .iter()
            .find(|p| p.name == "filed.fyi")
            .expect("nested independent repo should be discovered from the parent");

        let direct = scanner.scan(&nested, &config).unwrap();
        assert_eq!(direct.len(), 1);
        let direct_proj = &direct[0];

        assert_eq!(nested_from_parent.name, direct_proj.name);
        assert_eq!(nested_from_parent.path, direct_proj.path);

        // Equivalent language metadata.
        let lang_keyed = |p: &DiscoveredProject| {
            p.languages
                .iter()
                .map(|l| (l.language.clone(), format!("{:?}", l.markers)))
                .collect::<Vec<_>>()
        };
        assert_eq!(lang_keyed(nested_from_parent), lang_keyed(direct_proj));

        // Equivalent command metadata.
        let cmd_keyed = |p: &DiscoveredProject| {
            p.commands
                .iter()
                .map(|c| (c.name.clone(), c.command.clone(), c.source.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(cmd_keyed(nested_from_parent), cmd_keyed(direct_proj));
    }
}
