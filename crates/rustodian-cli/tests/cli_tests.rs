use std::fs;
use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn test_scan_and_list() {
    let dir = TempDir::new().unwrap();
    let proj_dir = dir.path().join("my-rust-proj");
    fs::create_dir(&proj_dir).unwrap();
    fs::write(proj_dir.join("Cargo.toml"), "[package]").unwrap();

    let js_dir = dir.path().join("my-js-proj");
    fs::create_dir(&js_dir).unwrap();
    fs::write(
        js_dir.join("package.json"),
        r#"{"scripts": {"build": "webpack"}}"#,
    )
    .unwrap();
    fs::write(
        js_dir.join("justfile"),
        "test:\n  echo test\n\nfmt:\n  prettier --write",
    )
    .unwrap();

    // 1. Scan
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("scan")
        .arg(dir.path());
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Projects Found:   2"));

    // 2. List
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("list");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("my-rust-proj"))
        .stdout(predicate::str::contains("my-js-proj"));

    // 3. Info for JS proj
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("info")
        .arg("my-js-proj");
    cmd.assert()
       .success()
       .stdout(predicate::str::contains("Discovered Commands:"))
       .stdout(predicate::str::contains("npm run build")) // from package.json
       .stdout(predicate::str::contains("just test")) // from justfile
       .stdout(predicate::str::contains("just fmt")); // from justfile
}
