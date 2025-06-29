# Iroh Transport Development Guide

## Overview

This guide helps developers work with the Iroh P2P transport in the Blixard codebase during development.

## Setting Up Your Development Environment

### Prerequisites

1. **Rust toolchain**
   ```bash
   rustup update
   cargo --version  # Should be 1.75+
   ```

2. **Network requirements**
   - UDP ports available for QUIC
   - Internet connection for STUN servers (or local STUN)
   - Firewall allowing local connections

### Building with Iroh

```bash
# Build with Iroh transport enabled
cargo build --features iroh-transport

# Run tests
cargo test --features iroh-transport

# Run with debug logging
RUST_LOG=blixard=debug,iroh=debug cargo run -- node
```

## Testing Locally

### 1. Single Node Test

```bash
# Start a single node with Iroh
cargo run -- node \
  --id 1 \
  --bind 127.0.0.1:7001 \
  --transport iroh \
  --data-dir /tmp/blixard-test
```

### 2. Multi-Node Cluster Test

Use the provided test script:
```bash
./scripts/test-iroh-locally.sh
```

Or manually:
```bash
# Terminal 1 - Bootstrap node
cargo run -- node --id 1 --bind 127.0.0.1:7001 --transport iroh

# Terminal 2 - Join cluster
cargo run -- node --id 2 --bind 127.0.0.1:7002 --transport iroh --join 127.0.0.1:7001

# Terminal 3 - Another node
cargo run -- node --id 3 --bind 127.0.0.1:7003 --transport iroh --join 127.0.0.1:7001
```

### 3. Test Examples

```bash
# Test serialization performance
cargo run --example iroh_raft_benchmark

# Test network operations
cargo run --example iroh_network_test

# Test cluster operations
cargo run --example iroh_cluster_demo
```

## Configuration Options

### Basic Iroh Configuration

```toml
[transport]
mode = "iroh"  # or "grpc" or "dual"

[transport.iroh]
# Connection settings
max_connections = 100
connection_pool_size = 10

# Timeouts
connect_timeout = "5s"
request_timeout = "30s"

# For development/testing
verbose_logging = true
enable_packet_capture = false
```

### Dual Mode (for A/B testing)

```toml
[transport]
mode = "dual"

[transport.migration]
prefer_iroh = ["health", "status"]  # Services to use Iroh
raft_transport = "grpc"             # Keep Raft on gRPC initially
fallback_to_grpc = true            # Fallback on errors
```

## Common Development Tasks

### Debugging Connection Issues

1. **Enable verbose logging**
   ```bash
   RUST_LOG=blixard=trace,iroh=trace cargo run -- node
   ```

2. **Check Iroh endpoint status**
   ```rust
   // In your code
   let endpoint = node.get_iroh_endpoint().await?;
   println!("Node ID: {}", endpoint.node_id());
   println!("Local addresses: {:?}", endpoint.local_addr());
   ```

3. **Test connectivity**
   ```bash
   # Use the network test
   cargo run --example iroh_network_test
   ```

### Adding New RPC Services

1. **Define service trait**
   ```rust
   #[async_trait]
   pub trait MyService: Send + Sync {
       async fn my_method(&self, request: MyRequest) -> BlixardResult<MyResponse>;
   }
   ```

2. **Create Iroh wrapper**
   ```rust
   pub struct IrohMyService {
       inner: Arc<dyn MyService>,
   }
   
   #[async_trait]
   impl IrohService for IrohMyService {
       // Implementation
   }
   ```

3. **Register with server**
   ```rust
   let service = IrohMyService::new(my_service_impl);
   iroh_server.register_service("my_service", Arc::new(service));
   ```

### Performance Testing

1. **Benchmark specific operations**
   ```rust
   use criterion::{black_box, criterion_group, criterion_main, Criterion};
   
   fn bench_iroh_message(c: &mut Criterion) {
       c.bench_function("iroh_serialize", |b| {
           b.iter(|| {
               serialize_message(black_box(&msg))
           })
       });
   }
   ```

2. **Compare with gRPC**
   ```bash
   # Run comparison benchmark
   cargo bench --features iroh-transport -- --baseline grpc
   ```

## Troubleshooting

### Issue: "No addressing information available"

**Solution**: Nodes can't find each other. Ensure:
- Nodes are sharing their Iroh NodeAddr
- Network allows UDP traffic
- No firewall blocking connections

### Issue: High CPU usage

**Solution**: Check for:
- Excessive logging (reduce log level)
- Connection churn (increase timeouts)
- Message batching disabled

### Issue: Tests failing with timeouts

**Solution**:
- Increase test timeouts for CI
- Use `test_helpers::wait_for_condition()`
- Check for proper async handling

## Testing Patterns

### Integration Test Template

```rust
#[tokio::test]
async fn test_iroh_functionality() {
    // Setup
    let secret = SecretKey::generate(rand::thread_rng());
    let endpoint = Endpoint::builder()
        .secret_key(secret)
        .bind()
        .await
        .unwrap();
    
    // Create services
    let server = IrohRpcServer::new(endpoint.clone());
    server.start().await.unwrap();
    
    // Test operations
    let client = IrohRpcClient::new(endpoint);
    // ... test code ...
    
    // Cleanup
    server.shutdown().await;
}
```

### Property Testing

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_message_roundtrip(msg in any::<RaftMessage>()) {
        let serialized = serialize_message(&msg).unwrap();
        let deserialized = deserialize_message(&serialized).unwrap();
        assert_eq!(msg, deserialized);
    }
}
```

## Development Tips

1. **Use type aliases for clarity**
   ```rust
   type IrohNodeId = iroh::NodeId;
   type BlixardNodeId = u64;
   ```

2. **Handle connection errors gracefully**
   ```rust
   match endpoint.connect(addr, alpn).await {
       Ok(conn) => { /* use connection */ },
       Err(e) if e.is_timeout() => { /* retry */ },
       Err(e) => { /* log and fail */ },
   }
   ```

3. **Monitor resource usage**
   ```rust
   // Track open connections
   let conn_count = endpoint.connection_count();
   metrics::gauge!("iroh_connections", conn_count as f64);
   ```

4. **Use structured logging**
   ```rust
   tracing::info!(
       node_id = %self.node_id,
       peer_id = %peer_id,
       transport = "iroh",
       "Connection established"
   );
   ```

## Next Steps

1. Run the test suite: `cargo test --features iroh-transport`
2. Try the examples in `examples/`
3. Set up a local cluster with `scripts/test-iroh-locally.sh`
4. Experiment with different configurations
5. Contribute improvements!

## Resources

- [Iroh Documentation](https://iroh.computer/docs)
- [QUIC Protocol](https://www.rfc-editor.org/rfc/rfc9000.html)
- [Our Integration Docs](../IROH_MIGRATION_PLAN.md)