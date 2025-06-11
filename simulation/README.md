# Blixard Simulation Tests

This crate contains deterministic simulation tests for Blixard using [madsim](https://github.com/madsim-rs/madsim).

## Overview

MadSim allows us to:
- Control time deterministically
- Simulate network partitions, latency, and packet loss
- Test distributed algorithms with reproducible failures
- Run thousands of test iterations quickly

## Running Tests

### Quick Start

```bash
# Run all simulation tests
./scripts/sim-test.sh

# Run with specific seed for reproduction
MADSIM_TEST_SEED=12345 ./scripts/sim-test.sh

# Run with debug logging
RUST_LOG=debug ./scripts/sim-test.sh
```

### Manual Commands

```bash
cd simulation

# Build with madsim cfg
RUSTFLAGS="--cfg madsim" cargo build --tests

# Run tests
RUSTFLAGS="--cfg madsim" cargo test -- --nocapture

# Run specific test suite
RUSTFLAGS="--cfg madsim" cargo test raft -- --nocapture
```

## Test Organization

- `cluster_tests.rs` - Node lifecycle and cluster formation
- `network_tests.rs` - Network fault injection scenarios  
- `raft_tests.rs` - Consensus algorithm correctness

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