# Test Infrastructure Documentation

## Current Test Status

As of June 20, 2025:
- **Total Tests**: ~190 tests across 31 test binaries
- **Pass Rate**: 100% (all active tests passing)
- **Ignored Tests**: 11 explicitly ignored + 4 in disabled file

## Ignored Tests Analysis

### 1. Network Partition & Distributed Systems Tests (3 tests)

#### `test_partition_healing_reconciliation` 
- **File**: `tests/network_partition_storage_tests.rs`
- **Reason**: 5-node cluster formation has timing issues
- **Status**: Needs investigation of timing issues in large clusters

#### `test_eventual_consistency_timing`
- **File**: `tests/distributed_storage_consistency_tests.rs`
- **Reason**: Flaky after consensus enforcement changes
- **Status**: May need timing adjustments for new Raft-based architecture

#### `test_network_partition_behavior`
- **File**: `tests/distributed_storage_consistency_tests.rs`
- **Reason**: Requires network partition simulation infrastructure
- **Status**: Placeholder - waiting for partition simulation framework

### 2. Tests Needing TestCluster Migration (2 tests)

These tests bypass Raft consensus and directly manipulate nodes, which is no longer valid:

#### `test_database_concurrent_access`
- **File**: `tests/node_tests.rs`
- **Current**: Uses direct Node operations
- **Needed**: Convert to use TestCluster for proper Raft consensus

#### `test_vm_lifecycle_operations`
- **File**: `tests/node_tests.rs`
- **Current**: Tests VM lifecycle with direct Node operations
- **Needed**: Convert to use TestCluster and gRPC clients

### 3. Tests Requiring Full Raft Setup (5 tests)

All in `tests/node_tests.rs`, these need proper cluster setup:
- `test_raft_manager_initialization`
- `test_task_submission`
- `test_task_status_retrieval`
- `test_cluster_status_after_join`
- `test_leave_cluster`

### 4. Manual/Stress Tests (2 tests)

Appropriately ignored for normal test runs:

#### `test_three_node_cluster_stress`
- **File**: `tests/three_node_cluster_tests.rs`
- **Purpose**: 30-second stress test with rapid operations
- **Usage**: `cargo test test_three_node_cluster_stress -- --ignored --nocapture`

#### `test_removed_node_message_handling`
- **File**: `tests/three_node_cluster_tests.rs`
- **Purpose**: Debug test for removed node message handling
- **Usage**: `cargo test test_removed_node_message_handling -- --ignored --nocapture`

### 5. Disabled Test File

#### `simulation/tests/grpc_tests.rs.disabled`
Contains 4 MadSim tests that were disabled during framework updates:
- `test_grpc_server_startup`
- `test_vm_operations`
- `test_cluster_operations`
- `test_concurrent_clients`

## Recommendations

### High Priority (Should Address)
1. **TestCluster Migration** (2 tests) - Important functionality lacking coverage
2. **Raft Setup Tests** (5 tests) - Core functionality that should be tested

### Medium Priority (Future Work)
1. **5-node Cluster Timing** - Investigate timing issues for larger clusters
2. **Eventual Consistency Test** - Adjust timing for Raft architecture

### Low Priority (Can Remain Ignored)
1. **Network Partition Tests** - Require significant infrastructure investment
2. **Manual/Stress Tests** - Appropriately ignored for CI
3. **Disabled MadSim Tests** - Can be addressed when updating simulation tests

## Test Infrastructure Improvements

### Recent Fixes (June 2025)
- Fixed TestCluster node ID allocation for extreme scenarios
- Fixed duplicate join request handling
- Fixed leader selection in add_node operations
- Achieved 100% pass rate for active tests

### Testing Best Practices
1. Use `TestCluster` for all multi-node tests
2. Use `timing::wait_for_condition_with_backoff()` instead of sleep()
3. Tests requiring Raft consensus must use TestCluster, not direct Node operations
4. Run stress tests manually with `--ignored` flag when needed

### Running Tests
```bash
# All active tests
cargo test --all-features

# Include ignored tests
cargo test --all-features -- --ignored

# Specific ignored test
cargo test test_name -- --ignored --nocapture

# With nextest (recommended)
cargo nextest run --all-features
```