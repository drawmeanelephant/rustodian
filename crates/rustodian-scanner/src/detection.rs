//! Language detection from filesystem markers.
//!
//! Each language detector is a pure function that examines a project directory
//! and returns detection evidence. Adding a new language is as simple as
//! adding a new function and registering it in [`detect_languages`].

use std::path::Path;

use rustodian_types::{DetectionConfidence, Language, LanguageDetection, LanguageMarker};

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

    detections
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
        markers.push(LanguageMarker::ConfigFile(
            "requirements.txt".to_string(),
        ));
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
        markers.push(LanguageMarker::ManifestFile(
            "package.json".to_string(),
        ));
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
    fn test_detect_multi_language() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();

        let detections = detect_languages(dir.path());
        assert_eq!(detections.len(), 2);
    }

    #[test]
    fn test_detect_empty_directory() {
        let dir = TempDir::new().unwrap();
        let detections = detect_languages(dir.path());
        assert!(detections.is_empty());
    }
}
