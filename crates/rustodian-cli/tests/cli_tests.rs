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
    fs::write(
        js_dir.join(".rustodian.toml"),
        r#"[commands]
custom-cmd = "echo hello world"
"#,
    )
    .unwrap();

    // 1. Scan
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("scan")
        .arg("--path")
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
        .stdout(predicate::str::contains("test"))
        .stdout(predicate::str::contains("build"))
        .stdout(predicate::str::contains("custom-cmd"));

    // 4. Run custom command
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("run")
        .arg("my-js-proj")
        .arg("custom-cmd");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("hello world"));
}

#[test]
fn test_janitor() {
    let dir = TempDir::new().unwrap();
    let proj_dir = dir.path().join("my-rust-proj");
    fs::create_dir(&proj_dir).unwrap();
    fs::write(proj_dir.join("Cargo.toml"), "[package]").unwrap();

    let target_dir = proj_dir.join("target");
    fs::create_dir(&target_dir).unwrap();
    fs::write(target_dir.join("dummy.txt"), "dummy").unwrap();

    // 1. Scan
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("scan")
        .arg("--path")
        .arg(dir.path());
    cmd.assert().success();

    // 2. Janitor dry-run
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("janitor")
        .arg("my-rust-proj")
        .arg("--dry-run");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target"))
        .stdout(predicate::str::contains("Reclaimable (Dry Run)"));

    // verify file still exists
    assert!(target_dir.join("dummy.txt").exists());

    // 3. Janitor purge
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("janitor")
        .arg("my-rust-proj")
        .arg("--purge");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target"))
        .stdout(predicate::str::contains("Reclaimed"));

    // verify file is deleted
    assert!(!target_dir.exists());
}
