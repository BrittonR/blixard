# Test Features Guide

This guide explains how to run tests that require the `test-helpers` feature.

## Methods to Run Tests with Features

### 1. Using Cargo Aliases (Recommended)

We've configured convenient aliases in `.cargo/config.toml`:

```bash
# Run all tests with test-helpers
cargo test-all

# Run Raft-specific tests
cargo test-raft

# Run unit tests only
cargo test-unit

# Run integration tests only
cargo test-integration
```

### 2. Manual Feature Flag

```bash
# Traditional method
cargo test --features test-helpers

# Specific test file
cargo test --test raft_state_machine_tests --features test-helpers
```

### 3. Required Features in Cargo.toml

Tests that need `test-helpers` are configured with `required-features`:

```toml
[[test]]
name = "raft_state_machine_tests"
required-features = ["test-helpers"]
```

This ensures:
- Tests won't compile without the required feature
- Clear error message if feature is missing
- No accidental test runs without proper setup

## Why test-helpers Feature?

The `test-helpers` feature enables:
- `TestNode` and `TestCluster` abstractions
- Deterministic timing utilities
- Mock implementations for testing
- Additional test-only APIs

## Adding New Tests

When creating a new test file that needs test-helpers:

1. Add the feature gate at the top of your test file:
   ```rust
   #![cfg(feature = "test-helpers")]
   ```

2. Add it to Cargo.toml:
   ```toml
   [[test]]
   name = "your_test_name"
   required-features = ["test-helpers"]
   ```

3. Run with: `cargo test-all` or `cargo test --test your_test_name --features test-helpers`