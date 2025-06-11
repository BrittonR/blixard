# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Complete Implementation Guide: Madsim + Proptest for Deterministic Distributed System Testing

### Overview

**Madsim** is a deterministic simulator for distributed systems in Rust that replaces async runtime and system APIs with simulated versions, allowing you to control time, network behavior, and randomness.

**Proptest** is a property-based testing framework that automatically generates test inputs and shrinks failing cases to minimal examples.

Together, they enable comprehensive testing where proptest explores the test space while madsim ensures each test is deterministic and reproducible.

### Setup

#### Cargo.toml Configuration

```toml
[dependencies]
madsim = "0.2"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
bytes = "1"
tracing = "0.1"
async-trait = "0.1"

[dev-dependencies]
madsim = { version = "0.2", features = ["macros"] }
proptest = "1.0"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

tonic = "0.11"  # For gRPC support
```

Note: For MadSim integration with tonic, see `docs/grpc_with_madsim.md` for the recommended approach using separate test crates.

### Testing Patterns

#### Running Madsim Tests

Tests using madsim MUST be run with the `--cfg madsim` flag:

```bash
# Run madsim simulation tests
RUSTFLAGS="--cfg madsim" cargo test --features simulation

# Run specific test
RUSTFLAGS="--cfg madsim" cargo test test_name --features simulation

# Run with specific seed for deterministic reproduction
MADSIM_TEST_SEED=1234567890 RUSTFLAGS="--cfg madsim" cargo test
```

#### Best Practices

1. **Use madsim runtime for tests**
```rust
// ❌ WRONG: Don't use tokio runtime directly
#[test]
fn incorrect_test() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // This uses real time, not simulated!
    });
}

// ✅ CORRECT: Use madsim runtime
#[test]
fn correct_test() {
    let rt = madsim::runtime::Runtime::new();
    rt.block_on(async {
        // Time is simulated
    });
}

// ✅ ALSO CORRECT: Use madsim::test attribute
#[madsim::test]
async fn also_correct_test() {
    // Automatically runs in madsim runtime
}
```

2. **Avoid Non-Deterministic Operations**
```rust
// ❌ WRONG: Real system time
let now = std::time::Instant::now();

// ✅ CORRECT: Simulated time (std types work in madsim context)
let now = std::time::Instant::now(); // This is intercepted by madsim when --cfg madsim

// ❌ WRONG: Real randomness
let value: u32 = rand::random();

// ✅ CORRECT: Use deterministic random in tests
let mut rng = madsim::rand::thread_rng();
let value: u32 = rng.gen();
```

3. **Network Testing**
```rust
#[madsim::test]
async fn test_network_simulation() {
    use tokio::net::{TcpListener, TcpStream};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    
    // Bind to port 0 for automatic assignment
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    tokio::task::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = [0; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        stream.write_all(&buf[..n]).await.unwrap();
    });
    
    tokio::time::sleep(Duration::from_millis(10)).await;
    
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(b"Hello").await.unwrap();
    
    let mut buf = [0; 5];
    stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"Hello");
}
```

## Quick Start Commands

### Development
```bash
# Build and test
cargo build                # Build the project
cargo test                 # Run tests
cargo test --features simulation  # Run deterministic simulation tests

# Testing scripts
./test_bootstrap.sh        # Bootstrap test (3 nodes)
./quick_test.sh           # Quick integration test
./test_cluster.sh         # Full cluster test

# Verification
cargo test --test proof_of_determinism  # Verify deterministic execution
```

### CLI Usage
```bash
# Cluster node management
cargo run -- node --id 1 --bind 127.0.0.1:7001  # Start node 1
cargo run -- node --id 2 --bind 127.0.0.1:7002 --peer 1:127.0.0.1:7001  # Join cluster

# VM management
cargo run -- vm create --name my-vm --config /path/to/microvm.nix
cargo run -- vm start my-vm
cargo run -- vm stop my-vm
cargo run -- vm list
cargo run -- vm status my-vm
```

## Commands and Usage

For current commands and usage:
- Check `src/main.rs` for CLI argument parsing
- Check `README.md` for usage examples and quickstart guide
- Check test scripts (`./test_*.sh`) for integration testing examples

## gRPC Integration

Blixard uses Tonic for gRPC communication between nodes and clients. The implementation follows these principles:

### Architecture
- **Runtime Abstraction**: Both `GrpcServer<R>` and `GrpcClient<R>` are generic over the runtime
- **Protocol Buffers**: Defined in `proto/blixard.proto` with cluster and VM management services
- **Deterministic Testing**: Can be tested with MadSim for reproducible network behavior

### Key Files
- `proto/blixard.proto` - Service definitions
- `src/grpc_server.rs` - Server implementation with runtime abstraction
- `src/grpc_client.rs` - Client implementation with runtime abstraction
- `tests/grpc_basic_test.rs` - Basic functionality tests
- `tests/grpc_madsim_test.rs` - Deterministic simulation tests
- `docs/grpc_with_madsim.md` - Detailed integration guide

### Usage
```rust
// Server
let server = GrpcServer::new(node, runtime);
server.serve(addr).await?;

// Client
let mut client = GrpcClient::connect(addr, runtime).await?;
let healthy = client.health_check().await?;
```

## Core Architecture

Blixard is a distributed microVM orchestration platform built in Rust with Raft consensus and deterministic testing.

### Migration Complete ✅

**The migration from Gleam to Rust is complete!** The project now features:
- **Raft Consensus**: Production-grade distributed consensus using tikv/raft-rs
- **Deterministic Testing**: TigerBeetle/FoundationDB-style simulation testing fully operational
- **Runtime Abstraction**: Complete separation between real and simulated runtime for testing
- **Consensus Safety**: Zero tolerance for Raft consensus violations maintained

**Migration Documentation**:
- `docs/GLEAM_TO_RUST_MIGRATION.md` - Migration history and approach
- `docs/RUST_GUIDELINES.md` - Rust coding standards and patterns
- `docs/advanced_testing_methodologies.md` - Testing methodologies implemented
- `FINAL_SIMULATION_STATUS.md` - Complete migration verification
- `HOW_TO_VERIFY_SIMULATION.md` - Instructions for verifying deterministic testing

**Key Achievements**:
- **✅ Consensus Safety**: All Raft consensus operations maintain safety guarantees
- **✅ Deterministic Testing**: 100% reproducible test execution with controlled time
- **✅ Runtime Abstraction**: All code uses RaftNode<R: Runtime> for testability

### Key Components

1. **src/main.rs** - CLI entry point and argument parsing
2. **src/node.rs** - Node management and lifecycle with RealRuntime
3. **src/raft_node_v2.rs** - Raft consensus implementation with runtime abstraction
4. **src/state_machine.rs** - Replicated state machine for VM orchestration
5. **src/storage.rs** - Persistent storage layer using redb
6. **src/microvm.rs** - MicroVM lifecycle management via microvm.nix
7. **src/network_v2.rs** - Network communication with runtime abstraction
8. **src/runtime/simulation.rs** - Deterministic simulation testing framework
9. **src/runtime_traits.rs** - Runtime abstraction layer (Real vs Simulated)

### Distributed System Design

- **Consensus**: Uses tikv/raft-rs for production-grade distributed consensus
- **Node Discovery**: Tailscale provides secure networking between cluster nodes
- **VM Management**: Direct microvm.nix integration for lightweight virtualization
- **Storage**: Persistent state using redb with Raft log replication
- **Testing**: Deterministic simulation with controlled time and network conditions

### Data Flow

1. CLI command → main.rs argument parsing
2. VM operation → state_machine.rs command processing
3. Raft consensus → raft_node_v2.rs proposal and replication
4. State persistence → storage.rs with redb backend
5. VM lifecycle → microvm.rs integration with microvm.nix
6. State replication → automatic Raft log distribution across cluster

### Configuration

Environment variables (set in flake.nix):
- `BLIXARD_STORAGE_MODE` - Storage backend selection
- `BLIXARD_KHEPRI_OPS` - Which operations use distributed storage

### Node Types

- **Cluster Nodes**: Persistent nodes running Khepri cluster (--join-cluster, --init-primary/secondary)
- **CLI Nodes**: Ephemeral nodes for individual commands that connect to cluster
- **User vs System**: Services can be managed at user level (--user flag) or system level

### Development Notes

- Uses `khepri_gleam` binding to Erlang Khepri library
- Erlang distribution handled by `distribution_helper.erl` 
- Tests use gleeunit framework
- Deployment creates symlinks in /usr/local/bin for system-wide access

### Khepri Storage Patterns

#### Path Format
- Khepri paths are lists of binaries without colons: `["services", "service-name"]`
- In Gleam code, paths are written with colons: `"/:services/service-name"` 
- The `khepri_gleam:to_khepri_path()` function strips the colons and splits the path

#### RPC Calls to Khepri
When making RPC calls to remote Khepri nodes:
1. **Path encoding**: Pass paths as lists of binaries: `[<<"services">>]` not `[":services"]`
2. **Result decoding**: Erlang `{ok, Value}` tuples appear as Gleam Result types in dynamic values
3. **Tuple access**: Use `decode.at([1], decoder)` to extract the value from an `{ok, Value}` tuple

#### Common Issues and Solutions
1. **"Failed to decode RPC result"**: Check if the path format is correct (no colons in binaries)
2. **Empty results when services exist**: Verify the RPC result structure - it may be wrapped differently than expected
3. **Service state shows "Unknown"**: Ensure the service info decoder handles the 3-tuple format: `(state, node, timestamp)`

#### Testing RPC Decoding
When debugging RPC issues:
1. Print the raw result: `io.println("Raw RPC result: " <> string.inspect(result))`
2. Check the dynamic type: `io.println("RPC result type: " <> string.inspect(dynamic.classify(result)))`
3. Try multiple decode strategies if the first fails

#### Service Management Gotchas

1. **Stop vs Remove**: 
   - `stop` marks a service as "Stopped" but keeps it in Blixard's management
   - `remove` deletes the service from Khepri entirely
   - Services marked as "Stopped" will still appear in `list` output

2. **Node Name Recording**:
   - CLI nodes have ephemeral names like `service_cli_<timestamp>@localhost`
   - When storing service info, record the cluster node's name, not the CLI node
   - This ensures services can be found by subsequent CLI commands

3. **Service Not Found Issues**:
   - If stop shows "not managed by Blixard", check:
     - RPC error handling in `get_service_info` 
     - Node name consistency in stored data
     - Proper decoding of Erlang error tuples like `{error, not_found}`

4. **RPC Timeouts**:
   - Add timeout protection to RPC calls (5 seconds default)
   - Use `rpc_helper.erl` for consistent timeout handling
   - Handle `{badrpc, timeout}` responses gracefully

## AI Development Guidelines

### Core Rules
1. Keep this file and referenced docs updated
2. Use AIDEV-* comment prefixes for context:
   - `AIDEV-NOTE`: Important implementation details
   - `AIDEV-TODO`: Future work items
   - `AIDEV-QUESTION`: Clarification needed
3. Never change API contracts without approval
4. Follow existing patterns over cleverness
5. Create structured, actionable errors
6. Profile before optimizing

### What AI Must NEVER Do
- Change API contracts without explicit approval
- Alter database migrations or Khepri schema
- Commit secrets or API keys
- Remove AIDEV-* comments
- Assume business logic - always ask for clarification

### Error Handling Example
```gleam
ServiceError {
  code: "SVC_001",
  message: "Service not found in cluster",
  service: "nginx",
  node: "cluster-node-1",
  timestamp: "2025-01-01T12:00:00Z",
}
```

## Testing

### Framework
- **gleeunit**: Gleam's unit testing framework
- Run tests with: `gleam test`
- Test files: `test/*.gleam`

### Testing Patterns
- Unit tests covering all major systems
- Cluster test harness for distributed operations
- Manual testing scripts for integration scenarios
- Test both user and system service management

### Resources
- `test/blixard_test.gleam`: Core functionality tests
- `test/cluster_test_harness.gleam`: Distributed testing utilities
- `scripts/test_*.sh`: Integration test scripts

## Performance

### Optimization Tips
- Enable Khepri caching for repeated operations
- Use `--verbose` flag to identify bottlenecks
- Monitor RPC call latency in distributed setups
- Profile Erlang distribution overhead

### Monitoring
- Use `replication_monitor.gleam` for cluster health
- Track service state consistency across nodes
- Monitor Tailscale network latency between nodes

## Technology Stack

- **Gleam + Erlang/OTP**: Core framework and runtime
- **Khepri**: Raft-based distributed storage
- **Tailscale**: Secure networking for cluster discovery
- **systemd**: Service lifecycle management
- **gleeunit**: Testing framework
- **Nix**: Reproducible builds and development environment