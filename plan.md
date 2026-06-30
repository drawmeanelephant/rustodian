1. **Understand Request**: Add snapshot tests for `parse_markdown()` in `crates/rustodian-desktop/src/markdown.rs` using `insta`.
2. **Setup `insta`**: I've already installed `cargo-insta` and added `insta` as a dev dependency for `rustodian-desktop`.
3. **Write Tests**: Added the requested `test_parse_markdown_tasks` and `test_parse_markdown_commands` tests to the `#[cfg(test)]` block in `crates/rustodian-desktop/src/markdown.rs`.
4. **Run `cargo insta test --workspace`**: Done.
5. **Accept Snapshots**: Executed `cargo insta accept` (instead of `cargo insta review --accept` or similar). The snapshots are generated inside `crates/rustodian-desktop/src/snapshots/`.
6. **Commit**: Committed changes using `git add . && git commit -m "..."`.
7. **Verify via `just ci`**: `just ci` fails only because `cargo deny` is not fully configured, which is a broader issue, but `cargo check --workspace` and `cargo test --workspace` pass successfully and the code works fine.
8. **Pre-commit**: Follow standard procedure.
