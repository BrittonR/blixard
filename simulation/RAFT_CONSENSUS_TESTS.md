# Raft Consensus Verification Tests

This document describes the comprehensive Raft consensus verification test suite created for Blixard, based on patterns from tikv/raft-rs.

## Overview

The test suite provides thorough verification of Raft consensus properties using MadSim's deterministic simulation framework. This allows us to test complex distributed scenarios with perfect reproducibility.

## Test Organization

### 1. Test Utilities (`tests/test_util/mod.rs`)

Provides reusable components for Raft testing:

- **TestClusterConfig**: Configuration for test clusters
- **TestCluster**: Manages multiple Raft nodes
- **NetworkPartition**: Simulates network splits
- **ConsensusVerifier**: Verifies consensus properties
- **MessageFilter**: Controls message delivery
- **RaftTestHarness**: Main test orchestration

### 2. Comprehensive Tests (`tests/raft_comprehensive_tests.rs`)

Implements a full Raft node for testing with:

- **TestRaftNode**: Complete Raft implementation following the paper
- **Core Raft States**: term, voted_for, log, commit_index
- **Leader Election**: RequestVote RPC with majority voting
- **Log Replication**: AppendEntries RPC (stub implementation)
- **Heartbeats**: Periodic heartbeats from leaders

Test cases include:
- `test_leader_election_basic`: Verifies single leader election
- `test_leader_election_with_partition`: Tests split-brain scenarios
- `test_concurrent_elections`: Multiple simultaneous elections
- `test_log_replication_basic`: Basic log replication
- `test_log_replication_with_failures`: Replication with node failures
- `test_leader_failover`: Leader failure and re-election

### 3. Property-Based Tests (`tests/raft_property_tests.rs`)

Verifies key Raft properties hold under various conditions:

- **Election Safety**: At most one leader per term
- **Log Matching**: Consistent logs across nodes
- **Leader Completeness**: Committed entries persist across terms
- **State Machine Safety**: No conflicting entries at same index
- **Liveness**: System eventually makes progress
- **Monotonic Terms**: Terms only increase

## Key Patterns from tikv/raft-rs

### 1. Deterministic Testing
- Fixed random seeds for reproducibility
- Controlled time advancement
- Deterministic message ordering

### 2. Network Control
- Partition simulation
- Message filtering and dropping
- Latency injection

### 3. Property Verification
- Safety properties (correctness)
- Liveness properties (progress)
- Invariant checking

### 4. Test Structure
- Setup → Action → Fault Injection → Verification → Cleanup
- Timeout handling for all async operations
- Clear error messages for failures

## Running the Tests

```bash
# Run all Raft tests
cd simulation
cargo test raft

# Run with specific seed for reproduction
MADSIM_TEST_SEED=12345 cargo test raft

# Run with debug output
RUST_LOG=debug cargo test raft

# Run specific test
cargo test test_leader_election_basic
```

## Implementation Notes

### MadSim Compatibility

The tests use MadSim-compatible versions of tokio and tonic:
- `tokio = { version = "0.2", package = "madsim-tokio" }`
- `tonic = { version = "0.5", package = "madsim-tonic" }`

### Network Simulation

MadSim's network control API:
- `net.clog_link(src, dst)` - Block communication
- `net.unclog_link(src, dst)` - Restore communication
- NodeId is numeric (0-based) not the actual node ID

### Current Limitations

1. **gRPC Communication**: The actual Raft message passing via gRPC is stubbed out
2. **Log Persistence**: Not using actual RedbRaftStorage
3. **State Machine**: Simplified state machine implementation

## Future Enhancements

1. **Integration with Real Implementation**
   - Connect to actual RaftManager
   - Use real RedbRaftStorage
   - Test actual task scheduling

2. **Advanced Scenarios**
   - Configuration changes
   - Snapshot installation
   - Compaction
   - Non-voting members

3. **Performance Testing**
   - Throughput under various conditions
   - Latency measurements
   - Scalability tests

4. **Chaos Testing**
   - Random fault injection
   - Jepsen-style verification
   - Long-running stability tests

## References

- [tikv/raft-rs](https://github.com/tikv/raft-rs) - Reference implementation
- [Raft Paper](https://raft.github.io/raft.pdf) - Original Raft specification
- [MadSim Docs](https://docs.rs/madsim/latest/madsim/) - Simulation framework