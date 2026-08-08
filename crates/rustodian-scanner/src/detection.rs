//! Detection of languages and project roots from filesystem markers.
//!
//! Each detector is a pure function that examines a project directory and
//! returns detection evidence. Adding a new language is as simple as adding a
//! new function and registering it in [`detect_languages`]. Project-root
//! markers are tracked separately from languages: a marker like
//! `wrangler.jsonc` identifies a project/deployment root without establishing
//! the implementation language.

use std::path::Path;

use rustodian_types::{
    DetectionConfidence, Language, LanguageDetection, LanguageMarker, ProjectRootMarker,
};

/// Detect all languages present in a project directory.
///
/// Runs all registered language detectors and collects results.
pub fn detect_languages(project_path: &Path) -> Vec<LanguageDetection> {
    let mut detections = Vec::new();

    // Run each detector — order doesn't matter, they're independent
    if let Some(d) = detect_rust(project_path) {
        detections.push(d);
    }
    if let Some(d) = detect_python(project_path) {
        detections.push(d);
    }
    if let Some(d) = detect_node(project_path) {
        detections.push(d);
    }
    if let Some(d) = detect_go(project_path) {
        detections.push(d);
    }
    if let Some(d) = detect_ruby(project_path) {
        detections.push(d);
    }
    if let Some(d) = detect_zig(project_path) {
        detections.push(d);
    }

    detections
}

/// Detect project-root markers that establish a directory as a project or
/// deployment root without making any language claim.
///
/// Currently recognizes Cloudflare Wrangler configuration files. Presence of
/// the file is sufficient — contents are never parsed, so a malformed
/// `wrangler.jsonc` still counts as project-root evidence.
pub fn detect_project_roots(project_path: &Path) -> Vec<ProjectRootMarker> {
    let mut markers = Vec::new();

    for file in ["wrangler.jsonc", "wrangler.json", "wrangler.toml"] {
        if project_path.join(file).exists() {
            markers.push(ProjectRootMarker::CloudflareWrangler(file.to_string()));
        }
    }

    markers
}

/// Detect Rust projects by looking for Cargo.toml.
fn detect_rust(path: &Path) -> Option<LanguageDetection> {
    let mut markers = Vec::new();

    if path.join("Cargo.toml").exists() {
        markers.push(LanguageMarker::ManifestFile("Cargo.toml".to_string()));
    }
    if path.join("Cargo.lock").exists() {
        markers.push(LanguageMarker::LockFile("Cargo.lock".to_string()));
    }

    if markers.is_empty() {
        return None;
    }

    let confidence = if markers
        .iter()
        .any(|m| matches!(m, LanguageMarker::ManifestFile(_)))
    {
        DetectionConfidence::High
    } else {
        DetectionConfidence::Medium
    };

    Some(LanguageDetection {
        language: Language::Rust,
        confidence,
        markers,
    })
}

/// Detect Python projects.
fn detect_python(path: &Path) -> Option<LanguageDetection> {
    let mut markers = Vec::new();

    for manifest in &["pyproject.toml", "setup.py", "setup.cfg"] {
        if path.join(manifest).exists() {
            markers.push(LanguageMarker::ManifestFile((*manifest).to_string()));
        }
    }
    for lock in &["poetry.lock", "Pipfile.lock", "uv.lock"] {
        if path.join(lock).exists() {
            markers.push(LanguageMarker::LockFile((*lock).to_string()));
        }
    }
    if path.join("requirements.txt").exists() {
        markers.push(LanguageMarker::ConfigFile("requirements.txt".to_string()));
    }

    if markers.is_empty() {
        return None;
    }

    let confidence = if markers
        .iter()
        .any(|m| matches!(m, LanguageMarker::ManifestFile(_)))
    {
        DetectionConfidence::High
    } else {
        DetectionConfidence::Medium
    };

    Some(LanguageDetection {
        language: Language::Python,
        confidence,
        markers,
    })
}

/// Detect Node.js projects.
fn detect_node(path: &Path) -> Option<LanguageDetection> {
    let mut markers = Vec::new();

    if path.join("package.json").exists() {
        markers.push(LanguageMarker::ManifestFile("package.json".to_string()));
    }
    for lock in &[
        "package-lock.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "bun.lockb",
    ] {
        if path.join(lock).exists() {
            markers.push(LanguageMarker::LockFile((*lock).to_string()));
        }
    }

    if markers.is_empty() {
        return None;
    }

    Some(LanguageDetection {
        language: Language::Node,
        confidence: DetectionConfidence::High,
        markers,
    })
}

/// Detect Go projects.
fn detect_go(path: &Path) -> Option<LanguageDetection> {
    let mut markers = Vec::new();

    if path.join("go.mod").exists() {
        markers.push(LanguageMarker::ManifestFile("go.mod".to_string()));
    }
    if path.join("go.sum").exists() {
        markers.push(LanguageMarker::LockFile("go.sum".to_string()));
    }

    if markers.is_empty() {
        return None;
    }

    Some(LanguageDetection {
        language: Language::Go,
        confidence: DetectionConfidence::High,
        markers,
    })
}

/// Detect Ruby projects.
fn detect_ruby(path: &Path) -> Option<LanguageDetection> {
    let mut markers = Vec::new();

    if path.join("Gemfile").exists() {
        markers.push(LanguageMarker::ManifestFile("Gemfile".to_string()));
    }

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Some(name) = entry
                .file_name()
                .to_str()
                .filter(|n| n.ends_with(".gemspec"))
            {
                markers.push(LanguageMarker::ManifestFile(name.to_string()));
            }
        }
    }

    if path.join("Gemfile.lock").exists() {
        markers.push(LanguageMarker::LockFile("Gemfile.lock".to_string()));
    }

    if markers.is_empty() {
        return None;
    }

    let confidence = if markers
        .iter()
        .any(|m| matches!(m, LanguageMarker::ManifestFile(_)))
    {
        DetectionConfidence::High
    } else {
        DetectionConfidence::Medium
    };

    Some(LanguageDetection {
        language: Language::Ruby,
        confidence,
        markers,
    })
}

/// Detect Zig projects.
fn detect_zig(path: &Path) -> Option<LanguageDetection> {
    let mut markers = Vec::new();

    if path.join("build.zig").exists() {
        markers.push(LanguageMarker::ManifestFile("build.zig".to_string()));
    }

    if path.join("build.zig.zon").exists() {
        markers.push(LanguageMarker::LockFile("build.zig.zon".to_string()));
    }

    if markers.is_empty() {
        return None;
    }

    let confidence = if markers
        .iter()
        .any(|m| matches!(m, LanguageMarker::ManifestFile(_)))
    {
        DetectionConfidence::High
    } else {
        DetectionConfidence::Medium
    };

    Some(LanguageDetection {
        language: Language::Zig,
        confidence,
        markers,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_detect_rust_project() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        fs::write(dir.path().join("Cargo.lock"), "").unwrap();

        let detections = detect_languages(dir.path());
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].language, Language::Rust);
        assert_eq!(detections[0].confidence, DetectionConfidence::High);
        assert_eq!(detections[0].markers.len(), 2);
    }

    #[test]
    fn test_detect_python_project() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("pyproject.toml"), "").unwrap();

        let detections = detect_languages(dir.path());
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].language, Language::Python);
    }

    #[test]
    fn test_detect_node_project() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();

        let detections = detect_languages(dir.path());
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].language, Language::Node);
    }

    #[test]
    fn test_detect_go_project() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("go.mod"), "module example").unwrap();

        let detections = detect_languages(dir.path());
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].language, Language::Go);
    }

    #[test]
    fn test_detect_ruby_project() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Gemfile"), "source 'https://rubygems.org'").unwrap();

        let detections = detect_languages(dir.path());
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].language, Language::Ruby);
    }

    #[test]
    fn test_detect_multi_language() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();

        let detections = detect_languages(dir.path());
        assert_eq!(detections.len(), 2);
    }

    #[test]
    fn test_detect_zig_project() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("build.zig"), "").unwrap();

        let detections = detect_languages(dir.path());
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].language, Language::Zig);
    }

    #[test]
    fn test_detect_empty_directory() {
        let dir = TempDir::new().unwrap();
        let detections = detect_languages(dir.path());
        assert!(detections.is_empty());
        assert!(detect_project_roots(dir.path()).is_empty());
    }

    #[test]
    fn test_detect_wrangler_only_project_claims_no_language() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("wrangler.jsonc"), "{ not json }").unwrap();

        // No language evidence: the Wrangler config is not a language marker.
        assert!(detect_languages(dir.path()).is_empty());

        // ...but it is project-root evidence.
        let roots = detect_project_roots(dir.path());
        assert_eq!(roots.len(), 1);
        assert!(matches!(
            &roots[0],
            ProjectRootMarker::CloudflareWrangler(f) if f == "wrangler.jsonc"
        ));
        assert_eq!(roots[0].platform(), "cloudflare-wrangler");
    }

    #[test]
    fn test_detect_wrangler_all_config_filenames() {
        for file in ["wrangler.jsonc", "wrangler.json", "wrangler.toml"] {
            let dir = TempDir::new().unwrap();
            fs::write(dir.path().join(file), "").unwrap();

            let roots = detect_project_roots(dir.path());
            assert_eq!(roots.len(), 1, "expected {file} to be detected");
            assert!(matches!(
                &roots[0],
                ProjectRootMarker::CloudflareWrangler(f) if f == file
            ));

            // The config file alone must never claim a language.
            assert!(detect_languages(dir.path()).is_empty());
        }
    }

    #[test]
    fn test_detect_wrangler_with_node_keeps_node_detection() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("wrangler.jsonc"), "").unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();

        let detections = detect_languages(dir.path());
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].language, Language::Node);
        assert_eq!(detections[0].confidence, DetectionConfidence::High);
        assert_eq!(detect_project_roots(dir.path()).len(), 1);
    }

    #[test]
    fn test_detect_wrangler_with_python_keeps_python_detection() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("wrangler.toml"), "").unwrap();
        fs::write(dir.path().join("pyproject.toml"), "").unwrap();

        let detections = detect_languages(dir.path());
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].language, Language::Python);
        assert_eq!(detect_project_roots(dir.path()).len(), 1);
    }

    #[test]
    fn test_detect_malformed_wrangler_jsonc_is_still_a_root() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("wrangler.jsonc"),
            "{\n  \"name\": // trailing garbage\n",
        )
        .unwrap();

        // Presence of the config file is sufficient; contents are never parsed.
        assert_eq!(detect_project_roots(dir.path()).len(), 1);
    }

    #[test]
    fn test_detect_package_json_without_wrangler_has_no_project_roots() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();

        assert_eq!(detect_languages(dir.path()).len(), 1);
        assert!(detect_project_roots(dir.path()).is_empty());
    }
}
