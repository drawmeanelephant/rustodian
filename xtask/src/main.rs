//! # xtask
//!
//! Workspace-level automation tasks for Rustodian.
//!
//! Run with: `cargo xtask <command>`
//! Or via justfile: `just xtask <command>`

use std::process::Command;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("coverage") => coverage(),
        Some("lint") => lint(),
        Some("dist") => dist(),
        Some("help") | None => help(),
        Some(unknown) => {
            eprintln!("Unknown command: {unknown}");
            eprintln!();
            help();
            std::process::exit(1);
        }
    }
}

fn help() {
    println!("Rustodian xtask - workspace automation");
    println!();
    println!("USAGE: cargo xtask <COMMAND>");
    println!();
    println!("COMMANDS:");
    println!("  coverage    Run tests with coverage reporting");
    println!("  lint        Run all lints (fmt + clippy + doc)");
    println!("  dist        Build release binaries");
    println!("  help        Show this help");
}

fn coverage() {
    println!("Running tests with coverage...");
    let status = Command::new("cargo")
        .args(["test", "--workspace"])
        .status()
        .expect("failed to run cargo test");

    if !status.success() {
        std::process::exit(1);
    }
    println!(
        "Coverage reporting not yet configured. \
         Run `cargo install cargo-tarpaulin` to set up."
    );
}

fn lint() {
    println!("Running all lints...");

    let checks = [
        ("cargo", vec!["fmt", "--all", "--", "--check"]),
        (
            "cargo",
            vec![
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        ),
        ("cargo", vec!["doc", "--workspace", "--no-deps"]),
    ];

    for (cmd, args) in &checks {
        println!("\n→ {} {}", cmd, args.join(" "));
        let status = Command::new(cmd)
            .args(args)
            .status()
            .unwrap_or_else(|e| panic!("failed to run {cmd}: {e}"));

        if !status.success() {
            eprintln!("\nLint failed!");
            std::process::exit(1);
        }
    }

    println!("\n✅ All lints passed!");
}

fn dist() {
    println!("Building release binary...");
    let status = Command::new("cargo")
        .args(["build", "--release", "-p", "rustodian-cli"])
        .status()
        .expect("failed to run cargo build");

    if !status.success() {
        std::process::exit(1);
    }
    println!("Binary at: target/release/rustodian");
}
