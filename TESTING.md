# Testing Guide for Blixard Rust Implementation

## Overview

This guide covers various ways to test the Rust implementation of Blixard, from unit tests to full cluster testing.

## Quick Start

```bash
# Run all tests
./test_current_impl.sh

# Or manually:
cargo test                          # All tests
cargo test --lib                    # Unit tests only
cargo test property_test            # Property-based tests
cargo test --features simulation    # Simulation tests with madsim
```

## Test Categories

### 1. Unit Tests

Located in `src/*.rs` files as `#[cfg(test)]` modules:

```bash
# Run unit tests for specific modules
cargo test storage::tests
cargo test raft_node::tests
cargo test state_machine::tests
```

### 2. Property-Based Tests (PropTest)

Tests state machine properties and invariants:

```bash
cargo test property_test

# With more test cases
PROPTEST_CASES=1000 cargo test property_test
```

### 3. Integration Tests

Located in `tests/` directory:

```bash
# Run specific integration test
cargo test test_single_node_raft_cluster
cargo test test_three_node_raft_cluster
cargo test test_storage_persistence
```

### 4. Simulation Tests (MadSim)

Deterministic simulation testing:

```bash
# Requires 'simulation' feature
cargo test --features simulation test_vm_creation_with_network_partition
cargo test --features simulation test_deterministic_vm_scheduling
```

### 5. Model Checking (StateRight)

Currently disabled but framework is in place:

```bash
# When enabled:
cargo test model_check -- --ignored
```

## Manual Testing

### Single Node Setup

```bash
# Terminal 1: Start a single node
cargo run -- \
    --node-id 1 \
    --data-dir /tmp/blixard-node1 \
    --bind-addr 127.0.0.1:9001
```

### Multi-Node Cluster

```bash
# Terminal 1: Start first node
cargo run -- \
    --node-id 1 \
    --data-dir /tmp/blixard-node1 \
    --bind-addr 127.0.0.1:9001

# Terminal 2: Start second node
cargo run -- \
    --node-id 2 \
    --data-dir /tmp/blixard-node2 \
    --bind-addr 127.0.0.1:9002 \
    --join-addr 127.0.0.1:9001

# Terminal 3: Start third node
cargo run -- \
    --node-id 3 \
    --data-dir /tmp/blixard-node3 \
    --bind-addr 127.0.0.1:9003 \
    --join-addr 127.0.0.1:9001
```

### Testing Raft Consensus

```bash
# Watch logs for leader election
tail -f /tmp/blixard-node*/raft.log

# Kill the leader node and watch re-election
# Find leader PID and kill it
ps aux | grep blixard
kill <LEADER_PID>
```

### Testing Storage Persistence

```bash
# Start a node and let it run
cargo run -- --node-id 1 --data-dir /tmp/test-persist --bind-addr 127.0.0.1:9001

# Kill it (Ctrl+C)

# Restart with same data dir - should restore state
cargo run -- --node-id 1 --data-dir /tmp/test-persist --bind-addr 127.0.0.1:9001
```

## Performance Testing

### Benchmarks

```bash
# Run benchmarks (if available)
cargo bench

# Profile with flamegraph
cargo install flamegraph
cargo flamegraph --bench <benchmark_name>
```

### Load Testing

```bash
# Simple load test script
cat > load_test.sh << 'EOF'
#!/bin/bash
# Send many requests in parallel
for i in {1..100}; do
    (cargo run -- service create "service-$i" --node-id $((i % 3 + 1))) &
done
wait
EOF

chmod +x load_test.sh
./load_test.sh
```

## Debugging Failed Tests

### Enable Debug Logging

```bash
# Set log level
RUST_LOG=debug cargo test test_name

# Or for specific modules
RUST_LOG=blixard::raft_node=debug cargo test

# With backtrace
RUST_BACKTRACE=1 cargo test test_name
```

### Run Single Test

```bash
# Run specific test with output
cargo test test_single_node_raft_cluster -- --nocapture
```

### Use Test Tools

```bash
# Check for race conditions with Loom
cargo test --features loom

# Run with sanitizers
RUSTFLAGS="-Z sanitizer=address" cargo test --target x86_64-unknown-linux-gnu
```

## CI Testing

```bash
# What CI should run
cargo fmt -- --check
cargo clippy -- -D warnings
cargo test
cargo test --features all-tests
```

## Test Coverage

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --out Html

# Open coverage report
open tarpaulin-report.html
```

## Common Issues

### Port Already in Use
```bash
# Find and kill processes using test ports
lsof -i :9001-9010
pkill -f blixard
```

### Test Data Cleanup
```bash
# Clean all test data
rm -rf /tmp/blixard-*
rm -rf test-data/
```

### Flaky Tests
- Run with `--test-threads=1` to avoid race conditions
- Increase timeouts in integration tests
- Check for hardcoded ports that might conflict

## Next Steps

1. Implement missing service management commands
2. Add stress tests for Raft consensus
3. Implement chaos testing (network partitions, node failures)
4. Add performance benchmarks
5. Set up continuous integration