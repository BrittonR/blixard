# Blixard Test Suite Fix Plan

## Objective
Transform the Blixard test suite from hollow tests that provide false confidence into a robust validation framework that actually verifies distributed system correctness.

## Phase 1: Remove Hollow Tests (Week 1)

### Task 1.1: Fix Tests with No Assertions
**Location**: `simulation/tests/grpc_mock_consensus_tests.rs`

For each test without assertions, implement proper verification:

```rust
// BEFORE (Hollow test):
#[tokio::test]
async fn test_three_node_leader_election() {
    let runtime = Runtime::new();
    runtime.block_on(async {
        let node1 = create_test_node(1, 7001).await;
        let node2 = create_test_node(2, 7002).await;
        let node3 = create_test_node(3, 7003).await;
        
        sleep(Duration::from_secs(2)).await;
        // Test ends with no verification!
    });
}

// AFTER (Meaningful test):
#[tokio::test]
async fn test_three_node_leader_election() {
    let runtime = Runtime::new();
    runtime.block_on(async {
        let mut cluster = TestCluster::new(3).await;
        
        // Wait for leader election with timeout
        let leader_id = wait_for_leader(&cluster, Duration::from_secs(10)).await
            .expect("Leader should be elected within 10 seconds");
        
        // Verify exactly one leader
        let leader_count = cluster.nodes()
            .filter(|n| n.is_leader())
            .count();
        assert_eq!(leader_count, 1, "Exactly one leader should exist");
        
        // Verify all nodes agree on leader
        for node in cluster.nodes() {
            assert_eq!(node.get_leader_id(), Some(leader_id), 
                      "Node {} should recognize {} as leader", node.id(), leader_id);
        }
        
        // Verify leader can process operations
        let result = cluster.get_node(leader_id)
            .submit_task("test_task", 1, 1024)
            .await;
        assert!(result.is_ok(), "Leader should accept tasks");
    });
}
```

**Specific tests to fix**:
- `test_single_node_bootstrap` - Verify node becomes leader, can accept operations
- `test_three_node_leader_election` - Verify leader election completes, all nodes agree
- `test_task_assignment_and_execution` - Verify task is assigned, executed, and result stored
- `test_leader_failover` - Verify new leader elected after old leader fails
- `test_network_partition_recovery` - Verify minority can't progress, majority continues

### Task 1.2: Replace Placeholder Tests
**Location**: `tests/network_partition_storage_tests.rs`

Replace entire placeholder implementation with real network partition testing:

```rust
// Delete the placeholder NetworkPartition struct and implement real partitioning:

use crate::test_helpers::{TestCluster, NetworkController};

async fn create_network_partition(
    cluster: &mut TestCluster,
    partition_a: Vec<u64>,
    partition_b: Vec<u64>,
) {
    // Use MadSim's network control or implement at peer_connector level
    for node_a in &partition_a {
        for node_b in &partition_b {
            cluster.block_connection(*node_a, *node_b).await;
            cluster.block_connection(*node_b, *node_a).await;
        }
    }
}

#[tokio::test]
async fn test_split_brain_prevention() {
    let mut cluster = TestCluster::new(5).await;
    cluster.wait_for_convergence().await;
    
    let initial_leader = cluster.get_leader_id()
        .expect("Should have leader before partition");
    
    // Create partition: [1,2] vs [3,4,5]
    create_network_partition(&mut cluster, vec![1,2], vec![3,4,5]).await;
    
    // Minority should not elect new leader or accept writes
    let minority_result = cluster.get_node(1)
        .submit_task("minority_task", 1, 1024)
        .await;
    assert!(minority_result.is_err(), "Minority partition should reject writes");
    
    // Majority should elect new leader and continue
    wait_for_condition(|| async {
        cluster.nodes()
            .filter(|n| vec![3,4,5].contains(&n.id()))
            .any(|n| n.is_leader())
    }, Duration::from_secs(10)).await
    .expect("Majority should elect new leader");
    
    let majority_result = cluster.get_node(3)
        .submit_task("majority_task", 1, 1024)
        .await;
    assert!(majority_result.is_ok(), "Majority partition should accept writes");
    
    // Heal partition and verify convergence
    cluster.heal_all_partitions().await;
    cluster.wait_for_convergence().await;
    
    // Verify all nodes have consistent state
    let final_tasks = cluster.get_all_tasks().await;
    assert_eq!(final_tasks.len(), 1, "Only majority task should be committed");
    assert_eq!(final_tasks[0].name, "majority_task");
}
```

## Phase 2: Eliminate Sleep Calls (Week 1-2)

### Task 2.1: Create Semantic Wait Functions
**Location**: `tests/common/test_timing.rs`

Add domain-specific wait functions:

```rust
pub async fn wait_for_leader(cluster: &TestCluster) -> Result<u64, TimeoutError> {
    wait_for_condition_with_backoff(
        || async {
            cluster.get_leader_id()
        },
        Duration::from_secs(10),
        || format!("Waiting for leader election in cluster of {} nodes", cluster.size()),
    ).await
}

pub async fn wait_for_replication(
    cluster: &TestCluster, 
    expected_count: usize
) -> Result<(), TimeoutError> {
    wait_for_condition_with_backoff(
        || async {
            let counts: Vec<_> = cluster.nodes()
                .map(|n| n.get_committed_count())
                .collect();
            counts.iter().all(|&c| c >= expected_count)
        },
        Duration::from_secs(5),
        || format!("Waiting for {} entries to replicate to all nodes", expected_count),
    ).await
}

pub async fn wait_for_task_completion(
    node: &TestNode,
    task_id: &str,
) -> Result<TaskResult, TimeoutError> {
    wait_for_condition_with_backoff(
        || async {
            node.get_task_result(task_id).await
        },
        Duration::from_secs(30),
        || format!("Waiting for task {} to complete", task_id),
    ).await
}
```

### Task 2.2: Replace All Sleep Calls
**Locations**: Multiple files

Replace each sleep with appropriate wait condition:

```rust
// BEFORE:
sleep(Duration::from_secs(2)).await; // Wait for convergence

// AFTER:
cluster.wait_for_convergence().await
    .expect("Cluster should converge within timeout");

// BEFORE:
sleep(Duration::from_millis(100)).await; // Let replication happen

// AFTER:
wait_for_replication(&cluster, expected_entries).await
    .expect("Replication should complete within timeout");
```

## Phase 3: Add Missing Distributed System Tests (Week 2-3)

### Task 3.1: Byzantine Failure Tests
**New file**: `tests/byzantine_failure_tests.rs`

```rust
#[tokio::test]
async fn test_conflicting_leader_claims() {
    // Test nodes claiming to be leader for same term
    let cluster = TestCluster::new(5).await;
    
    // Inject Byzantine behavior: node claims false leadership
    cluster.get_node(2).inject_byzantine_behavior(
        ByzantineBehavior::ClaimLeadership { term: 5 }
    );
    
    // Verify system rejects invalid claims
    // Verify honest nodes maintain consensus
}

#[tokio::test]
async fn test_corrupted_log_entries() {
    // Test handling of nodes sending corrupted data
}

#[tokio::test]
async fn test_omission_failures() {
    // Test nodes selectively dropping messages
}
```

### Task 3.2: Clock Skew Tests
**New file**: `tests/time_synchronization_tests.rs`

```rust
#[tokio::test]
async fn test_election_timeout_with_clock_skew() {
    let cluster = TestCluster::new(3).await;
    
    // Skew node 2's clock forward by 30 seconds
    cluster.get_node(2).set_clock_skew(Duration::from_secs(30));
    
    // Verify election still works correctly
    // Verify no spurious elections due to skew
}
```

### Task 3.3: Complex Network Scenarios
**New file**: `tests/advanced_network_tests.rs`

```rust
#[tokio::test]
async fn test_asymmetric_partition() {
    // Node 1 can send to 2 but not receive
    // Node 2 can receive from 1 but not send
    // Verify system handles asymmetric failures
}

#[tokio::test]
async fn test_rolling_network_failures() {
    // Continuously partition and heal different node pairs
    // Verify system maintains availability and consistency
}
```

## Phase 4: Strengthen Property Tests (Week 3-4)

### Task 4.1: Fix Weak Properties
**Location**: `tests/error_proptest.rs`

```rust
// BEFORE (Weak):
proptest! {
    #[test]
    fn test_error_display_not_empty(error in error_strategy()) {
        prop_assert!(!error.to_string().is_empty());
    }
}

// AFTER (Strong):
proptest! {
    #[test]
    fn test_error_recovery_preserves_state(
        initial_state in cluster_state_strategy(),
        error in error_strategy(),
    ) {
        // Inject error into system
        let mut cluster = TestCluster::from_state(initial_state.clone());
        cluster.inject_error(error.clone());
        
        // Attempt recovery
        let recovery_result = cluster.recover();
        
        // Verify invariants maintained
        prop_assert!(
            recovery_result.is_ok() || cluster.is_in_safe_degraded_mode(),
            "System should either recover or enter safe degraded mode"
        );
        
        if recovery_result.is_ok() {
            prop_assert_eq!(
                cluster.get_committed_state(),
                initial_state.get_committed_state(),
                "Committed state should be preserved after recovery"
            );
        }
    }
}
```

### Task 4.2: Add Distributed Property Tests
**New section in**: `tests/raft_proptest.rs`

```rust
proptest! {
    #[test]
    fn prop_linearizability(
        operations in vec(operation_strategy(), 1..100),
        partition_schedule in partition_schedule_strategy(),
    ) {
        // Execute operations concurrently with network partitions
        // Verify linearizable history exists
    }
    
    #[test]
    fn prop_eventual_consistency(
        initial_states in vec(node_state_strategy(), 3..7),
        operations in vec(operation_strategy(), 1..50),
    ) {
        // Start nodes with different states
        // Apply operations with temporary partitions
        // Verify all nodes eventually converge to same state
    }
}
```

## Phase 5: Improve Test Infrastructure (Week 4)

### Task 5.1: Unify Timing Utilities
Remove duplicates and standardize on single implementation:

```rust
// In tests/common/mod.rs - remove duplicate implementations
// Keep only the one in test_helpers with proper diagnostics

pub async fn wait_for_condition_with_diagnostics<F, Fut, T>(
    condition: F,
    timeout: Duration,
    diagnostic_fn: impl Fn() -> String,
) -> Result<T, TimeoutError>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Option<T>>,
{
    // Include diagnostic output on timeout
    // Log progress for debugging
}
```

### Task 5.2: Add Test Observability
Create test execution tracer:

```rust
pub struct TestTracer {
    events: Vec<TestEvent>,
}

impl TestTracer {
    pub fn record_operation(&mut self, op: Operation) { }
    pub fn record_state_change(&mut self, change: StateChange) { }
    pub fn generate_sequence_diagram(&self) -> String { }
    pub fn verify_linearizability(&self) -> Result<(), LinearizabilityError> { }
}
```

## Phase 6: Continuous Testing Improvements (Ongoing)

### Task 6.1: Chaos Testing Framework
```rust
pub struct ChaosMonkey {
    fault_injection_rate: f64,
    fault_types: Vec<FaultType>,
}

#[tokio::test]
async fn test_chaos_resilience() {
    let cluster = TestCluster::new(5).await;
    let chaos = ChaosMonkey::aggressive();
    
    // Run workload while chaos monkey injects faults
    let workload = spawn_workload(cluster.clone(), 1000_operations);
    let chaos_task = chaos.run_against(cluster.clone());
    
    let (workload_result, _) = join!(workload, chaos_task);
    
    // Verify system maintained invariants despite chaos
    verify_invariants(&cluster).await;
    assert!(workload_result.success_rate() > 0.95);
}
```

### Task 6.2: Performance Regression Tests
Add benchmarks that fail if performance degrades:

```rust
#[bench]
fn bench_leader_election_time(b: &mut Bencher) {
    b.iter(|| {
        let cluster = TestCluster::new(5);
        let start = Instant::now();
        cluster.wait_for_leader().await.unwrap();
        let elapsed = start.elapsed();
        
        assert!(elapsed < Duration::from_secs(3), 
                "Leader election took too long: {:?}", elapsed);
    });
}
```

## Success Criteria

1. **No hollow tests**: Every test has meaningful assertions
2. **No sleep() calls**: All replaced with semantic wait functions  
3. **Distributed properties tested**: Network partitions, Byzantine failures, consistency
4. **Property tests verify invariants**: Not just superficial properties
5. **Test confidence**: Team trusts test suite to catch real bugs

## Prioritization

1. **P0 (Week 1)**: Fix hollow tests - they're actively harmful
2. **P1 (Week 1-2)**: Remove sleep() calls - improve reliability
3. **P2 (Week 2-3)**: Add missing distributed tests - catch real bugs
4. **P3 (Week 3-4)**: Strengthen properties - better coverage
5. **P4 (Week 4+)**: Infrastructure improvements - long-term maintainability

## Validation

After each phase, run:
```bash
# Verify no tests with empty bodies
rg "async \{[^}]*sleep[^}]*\}" tests/

# Verify no raw sleep calls
rg "sleep\(Duration" tests/ | grep -v "wait_for"

# Run with stress profile to find flaky tests
cargo nt-stress --runs 100

# Verify determinism
./scripts/verify-determinism.sh
```

## Outcome

A test suite that actually validates distributed system correctness, catches real bugs before production, and gives developers confidence to make changes.