# Testing Strategy

## Unit Tests

Each crate has inline `#[cfg(test)]` modules. Run with:

```bash
cargo test --workspace
```

## Integration Tests

CLI integration tests use `assert_cmd` and `predicates`:

```bash
cargo test -p rustodian-cli
```

## Test Fixtures

Tests that need project directories use `tempfile::TempDir` to create
isolated fixture directories with specific marker files.

## Snapshot Testing

`insta` is available for snapshot testing of complex outputs:

```bash
# Run tests and review snapshots
cargo insta test --workspace
cargo insta review
```

## Coverage

```bash
cargo xtask coverage
# Or directly:
cargo tarpaulin --workspace --out html
```

## What to Test

| Crate | Focus |
|-------|-------|
| types | Serialization roundtrips |
| core | Custodian orchestration with mocks |
| storage | Migration idempotency, CRUD operations |
| scanner | Language detection, directory walking |
| git | Git info extraction from fixture repos |
| cli | End-to-end command testing |
