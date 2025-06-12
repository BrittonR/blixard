# Blixard Simulation Tests

This crate contains deterministic simulation tests for Blixard using [madsim](https://github.com/madsim-rs/madsim).

## Overview

MadSim allows us to:
- Control time deterministically
- Simulate network partitions, latency, and packet loss
- Test distributed algorithms with reproducible failures
- Run thousands of test iterations quickly

## Current Status

This simulation workspace is currently a separate crate from the main Blixard project due to dependency conflicts between standard tokio/tonic and their madsim counterparts. Tests that require the main Blixard types are temporarily disabled.

### Working Tests
- ✅ `grpc_integration_tests` - Basic gRPC service testing with madsim
- ✅ `grpc_mock_consensus_tests` - Mock consensus testing for gRPC integration
- ✅ `raft_simple_demo` - Demonstrates real Raft crate usage in MadSim
- ✅ `raft_comprehensive_tests` - Comprehensive Raft consensus verification
- ✅ `raft_property_tests` - Property-based testing for Raft invariants
- ✅ `raft_leader_election_test` - Focused leader election testing
- ✅ `test_util` - Shared utilities for Raft testing

## Running Tests

### Quick Start

```bash
# Run all simulation tests from the project root
./scripts/sim-test.sh

# Or run directly from the simulation directory
# (madsim flag is automatically applied via .cargo/config.toml)
cd simulation
cargo test

# Run with specific seed for reproduction
MADSIM_TEST_SEED=12345 ./scripts/sim-test.sh

# Run with debug logging
RUST_LOG=debug ./scripts/sim-test.sh
```

### Manual Commands

```bash
cd simulation

# Build (madsim cfg is automatically applied)
cargo build --tests

# Run tests
cargo test -- --nocapture

# Run specific test suite
cargo test raft -- --nocapture
```

## Test Organization

- `grpc_integration_tests.rs` - Basic gRPC service integration tests
- `grpc_mock_consensus_tests.rs` - Mock consensus behavior for testing gRPC flows
- `raft_consensus_tests.rs` - Real Raft consensus implementation tests
- `raft_simple_demo.rs` - Simple demonstrations of real Raft crate usage
- `integration_test.rs` - Basic MadSim functionality tests

## How It Works

1. **Conditional Compilation**: When `--cfg madsim` is set, tokio and other async libraries are replaced with madsim versions that intercept system calls.

2. **Deterministic Execution**: All sources of non-determinism (time, randomness, network) are controlled by the simulator.

3. **Fault Injection**: Network problems can be injected precisely:
   ```rust
   // Create network partition
   net.disconnect("10.0.0.1:8080", "10.0.0.2:8080");
   
   // Add latency
   net.add_latency("10.0.0.1:8080", "10.0.0.2:8080", Duration::from_millis(50));
   
   // Packet loss
   net.set_packet_loss("10.0.0.1:8080", "10.0.0.2:8080", 0.1); // 10% loss
   ```

4. **Time Control**: Time advances only when explicitly waited on:
   ```rust
   let start = Instant::now();
   sleep(Duration::from_secs(3600)).await; // Instant in simulation!
   assert_eq!(start.elapsed(), Duration::from_secs(3600));
   ```

## Adding New Tests

1. Add test function to appropriate module
2. Use `run_test` wrapper for consistent output
3. Follow pattern:
   - Setup phase (spawn nodes/services)
   - Fault injection (if applicable)
   - Operations
   - Assertions
   - Cleanup

Example:
```rust
async fn test_my_scenario() -> Result<(), String> {
    // Setup
    let nodes = spawn_cluster(3).await;
    
    // Inject fault
    partition_network(&["10.0.0.1:8080"], &["10.0.0.2:8080", "10.0.0.3:8080"]);
    
    // Test operations
    perform_operations(&nodes).await?;
    
    // Verify properties
    assert_safety_property(&nodes)?;
    assert_liveness_property(&nodes)?;
    
    Ok(())
}
```

## Best Practices

1. **Use Descriptive Names**: Test names should clearly indicate the scenario
2. **Log Important Events**: Use `tracing::info!` for key state changes
3. **Test Both Safety and Liveness**: Verify nothing bad happens AND progress is made
4. **Clean Up Resources**: Ensure nodes/tasks are properly stopped
5. **Document Expectations**: Comment what properties you're testing

## Debugging Failed Tests

1. **Reproduce with Seed**: Failed tests print their seed - use it to reproduce:
   ```bash
   MADSIM_TEST_SEED=<failed-seed> ./scripts/sim-test.sh
   ```

2. **Enable Debug Logging**:
   ```bash
   RUST_LOG=blixard=debug,madsim=debug ./scripts/sim-test.sh
   ```

3. **Add Assertions**: Break down complex assertions into steps

4. **Visualize Timeline**: MadSim can output event timelines for analysis