# gRPC with MadSim Integration

This document describes how to use Tonic (gRPC) with MadSim for deterministic testing in Blixard.

## Overview

MadSim provides deterministic replacements for tokio and tonic, allowing you to test gRPC-based distributed systems with perfect reproducibility. This integration enables:

- **Deterministic network behavior**: All gRPC calls are simulated
- **Time control**: Advance time programmatically
- **Network fault injection**: Simulate partitions, latency, and failures
- **Perfect reproducibility**: Same seed = same execution

## Setup

To use MadSim with tonic, you need to configure your test environment properly. MadSim provides its own implementations of tokio and tonic that intercept network calls.

### Creating a Test-Specific Cargo.toml

For tests that use MadSim, create a separate workspace member:

```toml
# tests/madsim-tests/Cargo.toml
[package]
name = "blixard-madsim-tests"
version = "0.1.0"
edition = "2021"

[dependencies]
blixard = { path = "../.." }
madsim = { version = "0.2", features = ["macros", "rpc"] }
tokio = { package = "madsim-tokio", version = "0.2" }
tonic = { package = "madsim-tonic", version = "0.2" }
tempfile = "3.8"

[[test]]
name = "grpc_simulation"
path = "grpc_simulation.rs"
```

This approach avoids conflicts with the main project's dependencies.

## Architecture

### Server Implementation

The `GrpcServer<R: Runtime>` is generic over the runtime, allowing it to work with both real and simulated runtimes:

```rust
pub struct GrpcServer<R: Runtime> {
    node: Arc<RwLock<Node>>,
    runtime: Arc<R>,
}
```

### Client Implementation

Similarly, `GrpcClient<R: Runtime>` abstracts over the runtime:

```rust
pub struct GrpcClient<R: Runtime> {
    client: ClusterServiceClient<Channel>,
    runtime: Arc<R>,
}
```

## Usage

### Running Tests

To run gRPC tests with MadSim:

```bash
# Run specific test
RUSTFLAGS="--cfg madsim" cargo test --features simulation test_grpc_client_server_with_madsim

# Run all gRPC MadSim tests
RUSTFLAGS="--cfg madsim" cargo test --features simulation grpc_madsim

# Run the demo
RUSTFLAGS="--cfg madsim" cargo run --example grpc_madsim_demo --features simulation
```

### Writing Tests

Here's a template for writing gRPC tests with MadSim:

```rust
#[madsim::test]
async fn test_grpc_operation() {
    // Create simulated runtime
    let runtime = Arc::new(SimulatedRuntime::new(42));
    
    // Setup server
    let server = create_grpc_server(runtime.clone()).await;
    madsim::task::spawn(async move {
        server.serve("127.0.0.1:50051".parse().unwrap()).await.unwrap();
    });
    
    // Wait for server (instant in simulation)
    madsim::time::sleep(Duration::from_millis(100)).await;
    
    // Create client
    let mut client = GrpcClient::connect(
        "http://127.0.0.1:50051".to_string(),
        runtime.clone()
    ).await.unwrap();
    
    // Test operations
    assert!(client.health_check().await.unwrap());
}
```

## Advanced Features

### Network Partitioning

Once fully integrated with MadSim's network simulation:

```rust
// Partition two nodes
runtime.partition_network(addr1, addr2);

// Heal partition
runtime.heal_network(addr1, addr2);
```

### Latency Injection

```rust
// Add 100ms latency between nodes
runtime.set_network_latency(from_addr, to_addr, Duration::from_millis(100));
```

### Deterministic Chaos Testing

```rust
#[madsim::test]
async fn test_grpc_under_chaos() {
    let runtime = Arc::new(SimulatedRuntime::new(42));
    
    // Setup cluster
    let nodes = setup_cluster(runtime.clone()).await;
    
    // Inject failures
    for _ in 0..10 {
        // Random partition
        let (node1, node2) = pick_random_nodes(&nodes);
        runtime.partition_network(node1.addr, node2.addr);
        
        // Advance time
        madsim::time::sleep(Duration::from_secs(5)).await;
        
        // Heal and verify consistency
        runtime.heal_network(node1.addr, node2.addr);
        verify_consistency(&nodes).await;
    }
}
```

## Best Practices

1. **Always use runtime abstraction**: Never use tokio directly in testable code
2. **Set deterministic seeds**: Use fixed seeds for reproducible tests
3. **Test edge cases**: Network partitions, latency, message reordering
4. **Verify invariants**: Check system properties hold under all conditions
5. **Use property-based testing**: Combine with proptest for comprehensive coverage

## Debugging

When tests fail:

1. **Check the seed**: Failed tests print the seed - use it to reproduce
2. **Add logging**: Use `tracing` to understand execution order
3. **Simplify**: Reduce the test to minimal failing case
4. **Time advancement**: Ensure you're advancing time when needed

## Limitations

- Real TLS/mTLS not supported in simulation (use test certificates)
- Some advanced tonic features may not be fully simulated
- Performance characteristics differ from real execution

## Examples

See these files for complete examples:
- `examples/grpc_madsim_demo.rs` - Basic demonstration
- `tests/grpc_madsim_test.rs` - Comprehensive test suite
- `src/grpc_server.rs` - Server implementation
- `src/grpc_client.rs` - Client implementation