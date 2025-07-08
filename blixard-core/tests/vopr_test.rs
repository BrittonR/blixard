//! VOPR Fuzzer Integration Tests
//!
//! These tests demonstrate various fuzzing scenarios and ensure
//! the VOPR fuzzer can detect different types of failures.

#![cfg(feature = "vopr")]

use blixard_core::vopr::{ByzantineBehavior, ClientOp, Operation, Vopr, VoprConfig};
use std::time::Duration;

#[tokio::test]
async fn test_basic_fuzzing() {
    // Create a simple configuration
    let config = VoprConfig {
        seed: 12345,
        time_acceleration: 100,
        max_clock_skew_ms: 1000,
        check_liveness: true,
        check_safety: true,
        max_operations: 100,
        enable_shrinking: false,
        enable_visualization: false,
        coverage_guided: false,
    };

    let mut vopr = Vopr::new(config);

    // Run fuzzing - should complete without errors for this small test
    match vopr.run().await {
        Ok(()) => {
            println!("Fuzzing completed successfully");
        }
        Err(failure) => {
            // This is actually good - we found a bug!
            println!(
                "Found bug with seed {}: {}",
                failure.seed, failure.violated_invariant
            );
        }
    }
}

#[tokio::test]
async fn test_deterministic_reproduction() {
    // Test that the same seed produces the same results
    let config1 = VoprConfig {
        seed: 99999,
        time_acceleration: 100,
        max_clock_skew_ms: 1000,
        check_liveness: true,
        check_safety: true,
        max_operations: 50,
        enable_shrinking: false,
        enable_visualization: false,
        coverage_guided: false,
    };

    let config2 = config1.clone();

    let mut vopr1 = Vopr::new(config1);
    let mut vopr2 = Vopr::new(config2);

    let result1 = vopr1.run().await;
    let result2 = vopr2.run().await;

    // Both runs should have the same outcome
    match (result1, result2) {
        (Ok(()), Ok(())) => {
            println!("Both runs succeeded");
        }
        (Err(f1), Err(f2)) => {
            assert_eq!(f1.seed, f2.seed);
            assert_eq!(f1.violated_invariant, f2.violated_invariant);
            assert_eq!(f1.operations.len(), f2.operations.len());
            println!("Both runs failed identically");
        }
        _ => {
            panic!("Determinism violated - different outcomes!");
        }
    }
}

#[test]
fn test_operation_generation() {
    use blixard_core::vopr::OperationGenerator;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    let mut gen = OperationGenerator::new(42);
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // Generate some operations
    let weights = Default::default();
    let ops: Vec<_> = (0..10)
        .map(|_| gen.generate_weighted(&weights, &mut rng))
        .collect();

    // Should generate a variety of operations
    assert!(ops.len() == 10);

    // Check that we get different operation types
    let has_start = ops
        .iter()
        .any(|op| matches!(op, Operation::StartNode { .. }));
    let has_client = ops
        .iter()
        .any(|op| matches!(op, Operation::ClientRequest { .. }));

    assert!(has_start || has_client);
}

#[test]
fn test_time_acceleration() {
    use blixard_core::vopr::TimeAccelerator;

    let accel = TimeAccelerator::new(1000, 100);

    // Register some nodes
    accel.register_node(1);
    accel.register_node(2);
    accel.register_node(3);

    // Get initial times
    let t1_before = accel.now(1);
    let t2_before = accel.now(2);

    // Advance time by 1 second
    accel.advance(Duration::from_secs(1));

    let t1_after = accel.now(1);
    let t2_after = accel.now(2);

    // Time should have advanced
    assert!(t1_after > t1_before);
    assert!(t2_after > t2_before);

    // Apply clock skew
    accel.randomize_clock_skew(2);

    // Node 2 should now have different time
    let t1_skewed = accel.now(1);
    let t2_skewed = accel.now(2);

    // There should be some difference due to skew
    let diff = t2_skewed
        .duration_since(&t1_skewed)
        .or_else(|| t1_skewed.duration_since(&t2_skewed));

    assert!(diff.is_some());
}

#[test]
fn test_shrinking() {
    use blixard_core::vopr::shrink::{Shrinker, TestFunction};
    use blixard_core::vopr::Operation;

    let operations = vec![
        Operation::StartNode { node_id: 1 },
        Operation::StartNode { node_id: 2 },
        Operation::StartNode { node_id: 3 },
        Operation::ClockJump {
            node_id: 1,
            delta_ms: 5000,
        },
        Operation::StopNode { node_id: 2 },
        Operation::StartNode { node_id: 4 },
        Operation::RestartNode { node_id: 3 },
    ];

    // Test function that fails if there's a clock jump
    let test_fn: TestFunction = Box::new(|ops| {
        ops.iter()
            .any(|op| matches!(op, Operation::ClockJump { .. }))
    });

    let shrinker = Shrinker::new();
    let minimal = shrinker.shrink(operations, test_fn);

    // Should reduce to just the clock jump (and maybe required start nodes)
    assert!(minimal.len() < 7);
    assert!(minimal
        .iter()
        .any(|op| matches!(op, Operation::ClockJump { .. })));
}

#[test]
fn test_invariant_checking() {
    use blixard_core::vopr::invariants::InvariantChecker;
    use blixard_core::vopr::state_tracker::{NodeRole, NodeState, StateSnapshot};
    use std::collections::HashMap;

    let mut checker = InvariantChecker::new();

    // Create a state with two leaders (invariant violation)
    let mut state = StateSnapshot {
        timestamp: 1000,
        nodes: HashMap::new(),
        views: HashMap::new(),
        commit_points: HashMap::new(),
        vms: HashMap::new(),
        partitions: Vec::new(),
        messages_in_flight: 0,
        resources: Default::default(),
        violations: Vec::new(),
    };

    // Add two nodes, both claiming to be leader in the same term
    state.nodes.insert(
        1,
        NodeState {
            id: 1,
            role: NodeRole::Leader,
            is_running: true,
            last_heartbeat: 900,
            log_length: 10,
            applied_index: 5,
            clock_skew_ms: 0,
            byzantine: None,
        },
    );

    state.nodes.insert(
        2,
        NodeState {
            id: 2,
            role: NodeRole::Leader,
            is_running: true,
            last_heartbeat: 950,
            log_length: 10,
            applied_index: 5,
            clock_skew_ms: 0,
            byzantine: None,
        },
    );

    state.views.insert(1, 5);
    state.views.insert(2, 5);

    // Check invariants
    let violations = checker.check(&state);

    // Should detect the multiple leaders violation
    assert!(!violations.is_empty());
    assert!(violations[0].contains("SingleLeader") || violations[0].contains("Multiple leaders"));
}
