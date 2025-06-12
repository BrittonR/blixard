# CLAUDE.md - MadSim Testing Guide

This guide provides comprehensive documentation for using MadSim in the Blixard project.

## Overview

MadSim is a deterministic simulator for distributed systems that replaces async runtime and system APIs with simulated versions. This allows us to:

- **Control Time**: Advance time deterministically without real delays
- **Inject Faults**: Create network partitions, latency, and packet loss
- **Ensure Reproducibility**: Same seed = same execution every time
- **Test at Scale**: Run thousands of iterations quickly

## Architecture

The simulation testing is organized as a separate workspace member to avoid dependency conflicts:

```
blixard/
├── Cargo.toml          # Main project
├── src/                # Production code
├── simulation/         # MadSim test crate
│   ├── Cargo.toml     # Conditional deps
│   ├── tests/         # Test suites
│   └── examples/      # Usage examples
└── scripts/
    └── sim-test.sh    # Test runner
```

## Key Concepts

### 1. Conditional Compilation

We use `cfg(madsim)` to switch between real and simulated implementations:

```toml
# simulation/Cargo.toml
[target.'cfg(madsim)'.dependencies]
tokio = { version = "0.2", package = "madsim-tokio" }
tonic = { version = "0.5", package = "madsim-tonic" }

[target.'cfg(not(madsim))'.dependencies]
tokio = { version = "1.35", features = ["full"] }
tonic = "0.11"
```

### 2. Test Organization

- **grpc_integration_tests.rs**: gRPC service integration tests
- **integration_test.rs**: Basic madsim functionality tests  
- **raft_tests.rs**: Consensus correctness tests (placeholder - TODOs)

### 3. Running Tests

```bash
# Quick start
./scripts/sim-test.sh

# With specific seed
MADSIM_TEST_SEED=12345 ./scripts/sim-test.sh

# With debug logging
RUST_LOG=debug ./scripts/sim-test.sh

# Run specific test (madsim flag auto-applied via .cargo/config.toml)
cd simulation
cargo test test_name
```

## Best Practices

### 1. Test Structure

```rust
#[madsim::test]
async fn test_scenario() {
    run_test("scenario_name", || async {
        // Setup
        let nodes = setup_cluster().await;
        
        // Action
        perform_operations(&nodes).await?;
        
        // Inject faults (optional)
        inject_network_fault().await;
        
        // Verify
        assert_safety_properties(&nodes)?;
        assert_liveness_properties(&nodes)?;
        
        // Cleanup
        cleanup(&nodes).await;
        
        Ok(())
    }).await;
}
```

### 2. Time Control

```rust
// Time advances only when you sleep/await
let start = Instant::now();
sleep(Duration::from_secs(3600)).await; // Instant!
assert_eq!(start.elapsed(), Duration::from_secs(3600));
```

### 3. Network Simulation

```rust
let net = NetSim::current();

// Create partition
net.disconnect(endpoint1, endpoint2);

// Add latency
net.add_latency(src, dst, Duration::from_millis(50));

// Packet loss
net.set_packet_loss(src, dst, 0.1); // 10% loss

// Heal partition
net.connect(endpoint1, endpoint2);
```

### 4. Deterministic Randomness

```rust
use madsim::rand::{thread_rng, Rng};

let mut rng = thread_rng();
let value: u32 = rng.gen(); // Deterministic based on seed
```

## Common Patterns

### Pattern 1: Cluster Testing

```rust
async fn test_cluster_formation() {
    let nodes = spawn_cluster(5).await;
    
    // Wait for stabilization
    sleep(Duration::from_secs(2)).await;
    
    // Verify all nodes joined
    for node in &nodes {
        assert!(node.is_connected());
    }
}
```

### Pattern 2: Partition Testing

```rust
async fn test_split_brain() {
    let nodes = spawn_cluster(5).await;
    
    // Create partition: [1,2] | [3,4,5]
    create_partition(&[1,2], &[3,4,5]);
    
    // Both partitions should make progress
    verify_partition_behavior(&nodes).await;
    
    // Heal and verify reconciliation
    heal_partition();
    verify_consistency(&nodes).await;
}
```

### Pattern 3: Chaos Testing

```rust
async fn test_chaos() {
    let nodes = spawn_cluster(10).await;
    let client = spawn_client().await;
    
    // Run workload
    let workload = tokio::spawn(async move {
        client.run_operations().await
    });
    
    // Inject random failures
    for _ in 0..20 {
        let fault = random_fault();
        apply_fault(fault).await;
        sleep(Duration::from_secs(1)).await;
    }
    
    // Verify no data loss
    workload.await.unwrap();
    verify_consistency(&nodes).await;
}
```

## Debugging Failed Tests

### 1. Reproduce with Seed

```bash
# Test output shows seed
test failed with seed: 1234567890

# Reproduce exact execution
MADSIM_TEST_SEED=1234567890 cargo test test_name
```

### 2. Enable Tracing

```rust
// In test setup
tracing_subscriber::fmt::init();

// In test code
tracing::info!("Important event: {:?}", data);
tracing::debug!("Detailed state: {:?}", state);
```

### 3. Step Through Execution

```rust
// Add checkpoints
println!("State before partition: {:?}", get_state());
apply_partition();
println!("State after partition: {:?}", get_state());
```

## Advanced Topics

### 1. Custom Runtimes

```rust
// Create runtime with specific config
let mut config = madsim::Config::new();
config.addr_num = 10; // Support 10 addresses
config.init_seed = 42;

let runtime = madsim::Runtime::with_config(config);
runtime.block_on(async {
    // Your test
});
```

### 2. Multi-Region Simulation

```rust
// Simulate WAN latencies
for region1 in regions {
    for region2 in regions {
        if region1 != region2 {
            net.add_latency(
                region1.endpoints(), 
                region2.endpoints(),
                Duration::from_millis(100) // Cross-region
            );
        }
    }
}
```

### 3. State Machine Testing

```rust
// Property: all replicas converge
async fn check_convergence(replicas: &[Replica]) {
    let states: Vec<_> = replicas.iter()
        .map(|r| r.get_state())
        .collect();
    
    assert!(states.windows(2).all(|w| w[0] == w[1]),
            "Replicas diverged: {:?}", states);
}
```

## Troubleshooting

### "cfg(madsim) not recognized"

Ensure you're using RUSTFLAGS:
```bash
RUSTFLAGS="--cfg madsim" cargo test
```

### "Type mismatch" errors

Check that production code doesn't hard-code tokio types:
```rust
// Bad: Hard-coded type
fn process(handle: tokio::task::JoinHandle<()>) { }

// Good: Generic or trait-based
fn process(handle: impl Future<Output = ()>) { }
```

### "Test hangs forever"

MadSim time only advances on await points:
```rust
// This will hang
loop {
    if condition { break; }
    // Missing: sleep(Duration::from_millis(1)).await;
}
```

## References

- [MadSim Documentation](https://docs.rs/madsim/latest/madsim/) - **IMPORTANT: Always check this for the latest API**
- [MadSim GitHub](https://github.com/madsim-rs/madsim)
- [MadSim Examples](https://github.com/madsim-rs/madsim/tree/main/examples)
- [RisingWave Usage](https://github.com/risingwavelabs/risingwave/tree/main/src/tests/simulation)

### API Notes

The MadSim API evolves over time. Key changes in recent versions:
- `disconnect()` → `clog_node()` / `clog_link()`
- `connect()` → `unclog_node()` / `unclog_link()`
- Latency and packet loss are configured via `Config` at runtime creation
- Use `NodeId` (numeric) instead of string endpoints for network control

## Future Enhancements

Once Raft is implemented, we'll add:

1. **Linearizability Testing**: Verify operations appear atomic
2. **Model Checking**: Exhaustively explore state space
3. **Failure Injection**: Node crashes, disk failures
4. **Performance Testing**: Measure throughput under faults
5. **Jepsen-style Tests**: Complex failure scenarios