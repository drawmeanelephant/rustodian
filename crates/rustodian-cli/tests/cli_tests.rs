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
fn test_run_failing_command_exits_nonzero() {
    let dir = TempDir::new().unwrap();
    let proj_dir = dir.path().join("my-failing-proj");
    fs::create_dir(&proj_dir).unwrap();
    fs::write(proj_dir.join("package.json"), "{}").unwrap();
    fs::write(
        proj_dir.join(".rustodian.toml"),
        r#"[commands]
fail = "echo 'boom' && exit 42"
"#,
    )
    .unwrap();

    // 1. Scan
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("scan")
        .arg("--path")
        .arg(dir.path());
    cmd.assert().success();

    // 2. Run the failing command: output is still captured, the CLI must exit
    //    nonzero, and no success message may be printed.
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("run")
        .arg("my-failing-proj")
        .arg("fail");
    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("boom"))
        .stdout(predicate::str::contains("Command executed successfully.").not())
        .stderr(predicate::str::contains("exit code 42"));
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
    let build_dir = proj_dir.join("build");
    fs::create_dir(&build_dir).unwrap();
    fs::write(build_dir.join("keep.txt"), "keep").unwrap();
    let dist_dir = proj_dir.join("dist");
    fs::create_dir(&dist_dir).unwrap();
    fs::write(dist_dir.join("keep.txt"), "keep").unwrap();
    let node_modules_dir = proj_dir.join("node_modules");
    fs::create_dir(&node_modules_dir).unwrap();
    fs::write(node_modules_dir.join("keep.txt"), "keep").unwrap();

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
        .stdout(predicate::str::contains("reclaimable"))
        .stdout(predicate::str::contains("5 B"));

    // verify file still exists
    assert!(target_dir.join("dummy.txt").exists());
    assert!(build_dir.exists());
    assert!(dist_dir.exists());
    assert!(node_modules_dir.exists());

    // 3. Structured JSON output exposes raw size values.
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    let output = cmd
        .env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("janitor")
        .arg("my-rust-proj")
        .arg("--format")
        .arg("json")
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["targets"][0]["target"], "target");
    assert_eq!(json["targets"][0]["outcome"], "reclaimable");
    assert_eq!(json["targets"][0]["size_bytes"], 5);

    // 4. Janitor purge
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("janitor")
        .arg("my-rust-proj")
        .arg("--purge");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target"))
        .stdout(predicate::str::contains("removed"));

    // Eligible cleanup is deleted; ambiguous and language-ineligible directories stay.
    assert!(!target_dir.exists());
    assert!(build_dir.exists());
    assert!(dist_dir.exists());
    assert!(node_modules_dir.exists());
}

#[cfg(unix)]
#[test]
fn test_janitor_refuses_symlink_target() {
    use std::os::unix::fs::symlink;

    let dir = TempDir::new().unwrap();
    let project = dir.path().join("my-rust-proj");
    fs::create_dir(&project).unwrap();
    fs::write(project.join("Cargo.toml"), "[package]").unwrap();
    let outside = TempDir::new().unwrap();
    let outside_file = outside.path().join("must-survive.txt");
    fs::write(&outside_file, "safe").unwrap();
    symlink(outside.path(), project.join("target")).unwrap();

    scan_project(dir.path(), "test.db");

    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("janitor")
        .arg("my-rust-proj")
        .arg("--purge");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skipped"))
        .stdout(predicate::str::contains("symbolic link"));

    assert!(
        project
            .join("target")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(outside_file.exists());
}

#[cfg(unix)]
#[test]
fn test_janitor_reports_partial_purge_failure() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let project = dir.path().join("mixed-project");
    fs::create_dir(&project).unwrap();
    fs::write(project.join("Cargo.toml"), "[package]").unwrap();
    fs::write(project.join("pyproject.toml"), "").unwrap();
    let target = project.join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("removed.txt"), "removed").unwrap();
    let venv = project.join(".venv");
    fs::create_dir(&venv).unwrap();
    fs::write(venv.join("locked.txt"), "not reclaimed").unwrap();

    scan_project(dir.path(), "test.db");
    fs::set_permissions(&venv, fs::Permissions::from_mode(0o555)).unwrap();

    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("janitor")
        .arg("mixed-project")
        .arg("--purge");
    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("target"))
        .stdout(predicate::str::contains(".venv"))
        .stdout(predicate::str::contains("removed"))
        .stdout(predicate::str::contains("failed"));

    assert!(!target.exists());
    assert!(venv.exists());
    fs::set_permissions(&venv, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
fn scan_project(root: &std::path::Path, db_name: &str) {
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", root.join(db_name))
        .arg("scan")
        .arg("--path")
        .arg(root);
    cmd.assert().success();
}

#[test]
fn test_brief_ignores_janitor_logs_for_health() {
    let dir = TempDir::new().unwrap();
    let proj_dir = dir.path().join("my-js-proj");
    fs::create_dir(&proj_dir).unwrap();
    fs::write(
        proj_dir.join("package.json"),
        r#"{"name": "my-js-proj", "scripts": {"start": "echo hi"}}"#,
    )
    .unwrap();
    // A shell-metacharacter command forces use_shell, so `echo boom && exit 1`
    // runs in sh/cmd and records exit code 1.
    fs::write(
        proj_dir.join(".rustodian.toml"),
        "[commands]\ntest = \"echo boom && exit 1\"\n",
    )
    .unwrap();

    // 1. Scan
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("scan")
        .arg("--path")
        .arg(dir.path());
    cmd.assert().success();

    // 2. Run the failing test -> logs "test" with exit code 1. The run
    //    command itself now exits nonzero when the child command fails.
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("run")
        .arg("my-js-proj")
        .arg("test");
    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("boom"));

    // 3. Janitor purge writes a `janitor:clean` log after the failed test
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("janitor")
        .arg("my-js-proj")
        .arg("--purge");
    cmd.assert().success();

    // 4. Brief must still classify by the failed test, not the janitor log
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    let output = cmd
        .env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("brief")
        .arg("--format")
        .arg("json")
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["projects"][0]["category"], "needs_attention");
    assert_eq!(
        json["projects"][0]["latest_command"]["command_name"],
        "test"
    );
    assert_eq!(json["projects"][0]["latest_command"]["exit_code"], 1);

    // 5. The logs command still surfaces janitor logs
    let mut cmd = Command::cargo_bin("rustodian").unwrap();
    cmd.env("RUSTODIAN_DB", dir.path().join("test.db"))
        .arg("logs")
        .arg("my-js-proj");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("janitor:clean"));
}
