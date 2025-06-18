# RaftManager Testing Enhancement Plan

## Overview
This plan outlines the implementation of comprehensive tests for RaftManager, focusing on untested areas like snapshots and recovery.

## Phase 1: Unit Tests for Core RaftManager Methods
**File: `tests/raft_manager_unit_tests.rs`**

### Test Cases:
1. **RaftStateMachine Tests**:
   - `test_apply_task_proposal` - Verify task creation and state updates
   - `test_apply_worker_registration` - Test worker registration logic
   - `test_apply_vm_operations` - Test VM state transitions
   - `test_apply_invalid_proposal` - Verify error handling
   - `test_state_machine_persistence` - Ensure state changes are persisted

2. **Message Handling Tests**:
   - `test_handle_vote_request` - Test vote request processing
   - `test_handle_append_entries` - Test log replication
   - `test_handle_snapshot_message` - Test snapshot message processing
   - `test_message_queue_overflow` - Test backpressure handling

3. **Proposal Pipeline Tests**:
   - `test_proposal_when_leader` - Successful proposal submission
   - `test_proposal_when_follower` - Rejection when not leader
   - `test_concurrent_proposals` - Multiple simultaneous proposals
   - `test_proposal_serialization` - Verify proposal encoding/decoding

## Phase 2: Snapshot Integration Tests
**File: `tests/raft_snapshot_tests.rs`**

### Test Cases:
1. **Snapshot Creation Tests**:
   ```rust
   #[tokio::test]
   async fn test_snapshot_trigger_for_lagging_follower() {
       // Setup 3-node cluster
       let mut cluster = TestCluster::new(3).await;
       
       // Create entries on leader while one follower is disconnected
       cluster.isolate_node(2).await;
       
       // Submit multiple proposals to create log divergence
       for i in 0..10 {
           cluster.submit_task(format!("task-{}", i)).await;
       }
       
       // Reconnect follower
       cluster.heal_partition(2).await;
       
       // Verify snapshot is triggered
       wait_for_condition_with_backoff(
           || async { cluster.node(2).has_received_snapshot().await },
           Duration::from_secs(10),
           "Follower should receive snapshot"
       ).await;
   }
   ```

2. **Snapshot Transfer Tests**:
   - `test_snapshot_transfer_success` - Successful snapshot delivery
   - `test_snapshot_transfer_failure_retry` - Retry on failure
   - `test_concurrent_snapshot_requests` - Multiple followers need snapshots
   - `test_snapshot_during_config_change` - Snapshot with membership changes

3. **Snapshot Application Tests**:
   - `test_state_restoration_from_snapshot` - Verify complete state recovery
   - `test_log_compaction_after_snapshot` - Old entries are removed
   - `test_node_catchup_via_snapshot` - Lagging node catches up
   - `test_snapshot_metadata_consistency` - Metadata matches state

## Phase 3: Recovery and Persistence Tests
**File: `tests/raft_recovery_tests.rs`**

### Test Cases:
1. **Node Recovery Tests**:
   ```rust
   #[tokio::test]
   async fn test_node_restart_state_recovery() {
       let data_dir = TempDir::new().unwrap();
       
       // Start node and create state
       let node = TestNode::new_with_dir(1, &data_dir).await;
       node.bootstrap_single_node().await;
       
       // Add some state
       node.submit_task("task-1").await;
       node.register_worker("worker-1", 100).await;
       
       // Shutdown node
       let state_before = node.get_full_state().await;
       node.shutdown().await;
       
       // Restart node with same data directory
       let node = TestNode::new_with_dir(1, &data_dir).await;
       node.initialize().await;
       
       // Verify state is recovered
       let state_after = node.get_full_state().await;
       assert_eq!(state_before, state_after);
   }
   ```

2. **State Consistency Tests**:
   - `test_task_persistence_across_restart` - Tasks survive restart
   - `test_worker_state_recovery` - Worker registrations persist
   - `test_raft_log_recovery` - Log entries are recovered
   - `test_configuration_persistence` - Cluster config survives restart

3. **Cluster Recovery Tests**:
   - `test_majority_failure_recovery` - Cluster recovers from majority failure
   - `test_split_brain_prevention` - No duplicate leaders after partition
   - `test_data_consistency_after_recovery` - All nodes converge to same state

## Phase 4: MadSim Deterministic Tests
**File: `simulation/tests/raft_manager_sim_tests.rs`**

### Test Cases:
1. **Network Partition Tests**:
   ```rust
   #[madsim::test]
   async fn test_leader_isolation_new_election() {
       let handle = madsim::runtime::Handle::current();
       
       // Create 5-node cluster
       let nodes = create_test_cluster(5).await;
       
       // Wait for leader election
       let leader_id = wait_for_leader(&nodes).await;
       
       // Isolate leader from others
       handle.net.partition(leader_id, &other_nodes);
       
       // Verify new leader is elected
       wait_for_condition(|| async {
           let new_leader = get_current_leader(&other_nodes).await;
           new_leader.is_some() && new_leader != Some(leader_id)
       }).await;
   }
   ```

2. **Snapshot Under Faults Tests**:
   - `test_snapshot_with_packet_loss` - 20% packet loss during transfer
   - `test_snapshot_during_partition` - Snapshot across network partition
   - `test_snapshot_leader_change` - Leader changes during snapshot

3. **Recovery Simulation Tests**:
   - `test_cascading_node_failures` - Nodes fail one by one
   - `test_simultaneous_recovery` - Multiple nodes recover together
   - `test_time_based_snapshot_trigger` - Periodic snapshot generation

## Phase 5: Property-Based Tests
**File: `tests/raft_manager_proptest.rs`**

### Properties:
```rust
proptest! {
    #[test]
    fn prop_no_lost_commits(
        operations in vec(operation_strategy(), 1..100),
        failures in vec(failure_strategy(), 0..5)
    ) {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let mut cluster = TestCluster::new(3).await;
            
            // Apply operations and failures
            let committed = apply_with_failures(&mut cluster, operations, failures).await;
            
            // Verify all committed operations are present
            for node in cluster.nodes() {
                let state = node.get_committed_operations().await;
                assert_eq!(state, committed, "Node {} has inconsistent state", node.id());
            }
        });
    }
}
```

### Additional Properties:
- `prop_leader_completeness` - Leaders have all committed entries
- `prop_snapshot_completeness` - Snapshots contain all state
- `prop_membership_consistency` - Nodes agree on configuration
- `prop_state_machine_determinism` - Same ops produce same state

## Phase 6: Stress and Performance Tests
**File: `tests/raft_manager_stress_tests.rs`**

### Test Cases:
1. **High Load Tests**:
   - `test_thousand_concurrent_proposals` - Handle 1000+ operations
   - `test_rapid_leader_elections` - 10 elections in 60 seconds
   - `test_memory_usage_under_load` - Memory stays bounded

2. **Large Cluster Tests**:
   - `test_seven_node_consensus` - 7-node cluster operations
   - `test_nine_node_snapshot_efficiency` - Snapshot scales well
   - `test_large_state_snapshot` - 100MB+ state snapshots

## Test Utilities
**File: `tests/common/raft_test_utils.rs`**

### Utilities:
```rust
pub struct RaftTestHarness {
    raft_manager: Arc<RaftManager>,
    // Helper methods for common operations
}

pub struct SnapshotVerifier {
    // Methods to verify snapshot contents
}

pub struct StateComparator {
    // Compare states across nodes
}

pub struct ProposalGenerator {
    // Generate various proposal types
}
```

## Implementation Priority

1. **High Priority** (Week 1):
   - Unit tests for RaftStateMachine
   - Snapshot creation and trigger tests
   - Basic recovery tests
   - Test utilities

2. **Medium Priority** (Week 2):
   - Snapshot transfer and application tests
   - MadSim network partition tests
   - Core property-based tests

3. **Low Priority** (Week 3):
   - Stress tests
   - Large cluster tests
   - Advanced property tests

## Success Metrics

- 90%+ code coverage for RaftManager
- All snapshot code paths tested
- Recovery scenarios validated
- No flaky tests (100% reliability)
- Tests complete in < 5 minutes

## Dependencies

- `tokio-test` - Async test utilities
- `proptest` - Property testing
- `tempfile` - Temporary directories
- `tracing-test` - Test logging
- Existing `test_helpers` module

## Implementation Progress

### ✅ Completed

1. **Test Utilities** (`tests/common/raft_test_utils.rs`)
   - Created RaftTestHarness, SnapshotVerifier, StateComparator, ProposalGenerator
   - Added helper functions for cluster creation and leader detection

2. **RaftStateMachine Unit Tests** (`tests/raft_state_machine_tests.rs`)
   - 12 comprehensive tests for the isolated state machine
   - Tests cover all apply_* methods: tasks, workers, VMs
   - Tests include edge cases: non-existent entities, idempotency, overwrites
   - All tests pass reliably (100% success rate)

### Key Findings

1. **RaftStateMachine Already Isolated**: The state machine was already well-factored as a separate struct that only depends on the database. No refactoring was needed.

2. **Direct Testing Works Well**: We can test the business logic directly without the complexity of Raft consensus, channels, or distributed systems.

3. **Fast and Reliable**: Tests complete in ~0.2 seconds with 100% reliability.

### Next Steps

- Phase 1: Complete message handling and proposal pipeline tests
- Phase 2: Implement snapshot integration tests
- Phase 3: Add recovery and persistence tests
- MadSim for deterministic testing