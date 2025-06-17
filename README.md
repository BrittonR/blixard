# Blixard - Distributed MicroVM Orchestration Platform

A distributed microVM orchestration platform built in Rust, providing enterprise-grade virtual machine management with Raft consensus.

## Features

- **Raft Consensus**: Production-grade distributed consensus using tikv/raft-rs (✅ Implemented)
- **Multi-Node Clustering**: Dynamic cluster formation with join/leave operations (✅ Implemented)
- **Raft Snapshots**: Full snapshot support for state transfer and log compaction (✅ Implemented)
- **MicroVM Integration**: Seamless integration with microvm.nix for lightweight virtualization
- **Tailscale Discovery**: Automatic node discovery via Tailscale networking
- **Persistent Storage**: Durable state management with redb (✅ Implemented)
- **Fault Tolerant**: Designed for high availability with automatic failover
- **Deterministic Testing**: TigerBeetle/FoundationDB-style simulation testing for reproducible verification
- **Task Scheduling**: Resource-aware task assignment with distributed state machine (✅ Implemented)
- **Worker Management**: Dynamic worker registration and health monitoring (✅ Implemented)
- **Peer Management**: Automatic peer connection management with reconnection logic (✅ Implemented)

## Architecture

Blixard uses a distributed architecture with:
- Raft consensus for state replication
- Direct microvm.nix integration for VM lifecycle management
- Tailscale for secure node communication
- Event-driven state machine for consistency

## Quick Start

### Prerequisites

- Nix with flakes enabled
- Tailscale (optional, for node discovery)
- microvm.nix

### Development Environment

```bash
# Enter development shell
nix develop

# Build the project
cargo build

# Run tests
cargo test

# Run CLI integration tests
cargo test --test cli_integration_test
cargo test --test vm_cli_test
```

### Running a Cluster

```bash
# Start a 3-node test cluster
./test_bootstrap.sh

# Or start nodes manually:
# Node 1 (bootstrap)
cargo run -- node --id 1 --bind 127.0.0.1:7001

# Node 2
cargo run -- node --id 2 --bind 127.0.0.1:7002 --peer 1:127.0.0.1:7001

# Node 3
cargo run -- node --id 3 --bind 127.0.0.1:7003 --peer 1:127.0.0.1:7001 --peer 2:127.0.0.1:7002
```

### VM Management

```bash
# Create a VM
cargo run -- vm create --name my-vm --config /path/to/microvm.nix

# Start a VM
cargo run -- vm start my-vm

# Stop a VM
cargo run -- vm stop my-vm

# List VMs
cargo run -- vm list

# Check VM status
cargo run -- vm status my-vm
```

## Testing

Blixard includes comprehensive testing with deterministic simulation capabilities:

```bash
# Unit tests
cargo test

# CLI integration tests
cargo test --test cli_integration_test
cargo test --test vm_cli_test

# Run all tests with cargo nextest (recommended for better isolation)
cargo nextest run --all-features

# Deterministic simulation tests (TigerBeetle/FoundationDB-style)
cargo test -p blixard-madsim-tests

# Run with fixed seed for reproducibility
MADSIM_TEST_SEED=12345 cargo test -p blixard-madsim-tests

# Quick integration test (2 nodes)
./quick_test.sh

# Bootstrap test (3 nodes with leader election)
./test_bootstrap.sh

# Full cluster test
./test_cluster.sh
```

### Test Status

- **Single-node clusters**: ✅ Fully working and reliable
- **Three-node clusters**: ✅ Core functionality working (basic cluster formation, leader election)
- **Multi-node operations**: 🔧 Partial - task/worker functionality not yet implemented

Recent fixes have resolved the three-node cluster formation issues. See [THREE_NODE_CLUSTER_DEBUG.md](THREE_NODE_CLUSTER_DEBUG.md) for details.

### Deterministic Testing

Blixard features a sophisticated deterministic testing framework inspired by TigerBeetle and FoundationDB:

- **Controlled Time**: Tests use simulated time that can be advanced precisely
- **Reproducible Results**: Same seed always produces identical test execution
- **Chaos Testing**: Network partitions, node failures, and timing edge cases
- **Raft Safety**: Comprehensive consensus safety verification

See [HOW_TO_VERIFY_SIMULATION.md](HOW_TO_VERIFY_SIMULATION.md) for verification instructions.

## Configuration

Nodes can be configured with:
- `--id`: Unique node identifier
- `--bind`: Address to bind to (e.g., 127.0.0.1:7001)
- `--peer`: Peer addresses for cluster formation
- `--data-dir`: Directory for persistent storage
- `--tailscale`: Enable Tailscale discovery

## gRPC API

Blixard exposes two gRPC services for programmatic access:

### BlixardService
- `GetRaftStatus`: Query the current Raft consensus state
- `ProposeTask`: Submit tasks through Raft consensus

### ClusterService  
- `JoinCluster`: Join a node to the cluster
- `LeaveCluster`: Remove a node from the cluster
- `GetClusterStatus`: Get current cluster state
- `SendRaftMessage`: Internal Raft message routing
- `SubmitTask`: Submit resource-aware tasks
- `GetTaskStatus`: Query task execution status
- VM operations: `CreateVm`, `StartVm`, `StopVm`, `ListVms`, `GetVmStatus`
- `HealthCheck`: Node health status

Example client usage:
```bash
# Run the example gRPC client
cargo run --example blixard_grpc_client [server_address]

# Default connects to http://127.0.0.1:7001
cargo run --example blixard_grpc_client
```

## Development

The codebase is organized as:
- `src/main.rs`: CLI entry point
- `src/node.rs`: Node management and lifecycle
- `src/raft_manager.rs`: Raft consensus implementation
- `src/grpc_server.rs`: gRPC service implementations
- `src/storage.rs`: Persistent storage layer
- `src/vm_manager.rs`: VM lifecycle management
- `src/types.rs`: Core domain types
- `src/error.rs`: Error handling
- `proto/`: Protocol buffer definitions

## License

[License information here]