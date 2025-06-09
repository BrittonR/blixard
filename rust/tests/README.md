# Blixard Testing Strategy

Following the TigerBeetle/FoundationDB approach, we use multiple layers of testing to ensure correctness:

## Test Types

### 1. Deterministic Simulation Tests (`simulation_test.rs`)
- Uses `madsim` for deterministic execution
- Tests distributed scenarios with controlled time and I/O
- Same seed = same results, making bugs reproducible
- Run with: `cargo nextest run --features simulation`

### 2. Property-Based Tests (`property_test.rs`)
- Uses `proptest` to generate random test cases
- Verifies invariants hold across all possible inputs
- Automatically shrinks failing cases to minimal examples
- Run with: `cargo nextest run property_test`

### 3. Fault Injection Tests (`fault_injection_test.rs`)
- Uses `fail-rs` to inject failures at specific points
- Tests error handling and recovery mechanisms
- Simulates network failures, disk errors, etc.
- Run with: `cargo nextest run --features failpoints`

### 4. Model Checking Tests (`model_checking_test.rs`)
- Uses `stateright` to exhaustively check state spaces
- Verifies safety and liveness properties
- Finds rare race conditions and edge cases
- Run with: `cargo nextest run model_checking`

## Running Tests

```bash
# Quick test run
cargo nextest run

# Run all test types
test_all  # Function available in nix develop shell

# Run specific test types
cargo nextest run --features simulation
cargo nextest run --features failpoints
cargo nextest run -p blixard property_test

# Watch mode for development
cargo watch -x "nextest run"

# Run with specific nextest profile
cargo nextest run --profile ci
```

## Key Testing Principles

1. **Determinism First**: All tests should be deterministic when possible
2. **Property Over Example**: Test properties that must hold, not specific examples
3. **Inject Failures**: Actively test failure scenarios, don't wait for them
4. **Model Critical Paths**: Use formal methods for consensus and state management
5. **Continuous Testing**: Run long-running tests to find deep bugs

## Writing New Tests

When adding new functionality:
1. Write property tests for the invariants
2. Add simulation tests for distributed behavior
3. Add fault injection points in the implementation
4. Model check critical state machines
5. Run continuous tests overnight