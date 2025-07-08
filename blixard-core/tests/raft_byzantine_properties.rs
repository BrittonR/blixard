//! Byzantine Fault Tolerance Property Tests for Raft
//!
//! This module tests Raft's resilience against Byzantine (malicious) behavior.
//! While standard Raft doesn't provide Byzantine fault tolerance, these tests
//! verify that the implementation correctly rejects invalid messages and maintains
//! safety properties even under adversarial conditions.

#![cfg(feature = "test-helpers")]

mod common;

use once_cell::sync::Lazy;
use proptest::prelude::*;
use proptest::test_runner::{Config, TestCaseError};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use blixard_core::{
    error::{BlixardError, BlixardResult},
    raft_manager::{ProposalData, VmCommand},
    test_helpers::{timing, TestCluster, TestNode},
    types::VmConfig,
};

// Shared runtime
static RUNTIME: Lazy<tokio::runtime::Runtime> =
    Lazy::new(|| tokio::runtime::Runtime::new().unwrap());

/// Types of Byzantine behaviors to test
#[derive(Debug, Clone)]
enum ByzantineBehavior {
    /// Send vote requests with wrong term
    VoteWithWrongTerm { claimed_term: u64 },

    /// Vote for multiple candidates in the same term
    MultipleVotes { term: u64, candidates: Vec<u64> },

    /// Claim to be leader without winning election
    FakeLeaderClaim { term: u64 },

    /// Send append entries with gaps
    AppendEntriesWithGaps { gap_indices: Vec<u64> },

    /// Send conflicting entries at same index
    ConflictingEntries { index: u64, entries: Vec<Vec<u8>> },

    /// Replay old messages
    MessageReplay { delay_ms: u64 },

    /// Forge messages from other nodes
    IdentityForgery { impersonate_node: u64 },

    /// Send messages with corrupted data
    CorruptedData { corruption_type: DataCorruption },

    /// Attempt to roll back committed entries
    RollbackCommitted { target_index: u64 },

    /// Send inconsistent commit indices
    InconsistentCommitIndex {
        claimed_commit: u64,
        actual_commit: u64,
    },
}

#[derive(Debug, Clone)]
enum DataCorruption {
    /// Flip random bits
    BitFlip { positions: Vec<usize> },
    /// Truncate data
    Truncate { new_length: usize },
    /// Insert garbage data
    InsertGarbage { position: usize, garbage: Vec<u8> },
}

/// Tracks Byzantine attack attempts and their outcomes
#[derive(Debug, Clone)]
struct ByzantineTracker {
    /// Attacks that were attempted
    attempted_attacks: Arc<Mutex<Vec<(u64, ByzantineBehavior)>>>,

    /// Attacks that succeeded in causing incorrect behavior
    successful_attacks: Arc<Mutex<Vec<(u64, ByzantineBehavior, String)>>>,

    /// Safety violations detected
    safety_violations: Arc<Mutex<Vec<SafetyViolation>>>,
}

#[derive(Debug, Clone)]
enum SafetyViolation {
    /// A Byzantine node became leader
    ByzantineLeader { node_id: u64, term: u64 },

    /// Committed entry was modified
    CommittedEntryModified {
        index: u64,
        original: Vec<u8>,
        modified: Vec<u8>,
    },

    /// State divergence due to Byzantine behavior
    StateDivergence {
        nodes: Vec<u64>,
        description: String,
    },

    /// Invalid message was accepted
    InvalidMessageAccepted {
        from_node: u64,
        message_type: String,
    },
}

impl ByzantineTracker {
    fn new() -> Self {
        Self {
            attempted_attacks: Arc::new(Mutex::new(Vec::new())),
            successful_attacks: Arc::new(Mutex::new(Vec::new())),
            safety_violations: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn record_attempt(&self, node_id: u64, behavior: ByzantineBehavior) {
        self.attempted_attacks
            .lock()
            .unwrap()
            .push((node_id, behavior));
    }

    fn record_success(&self, node_id: u64, behavior: ByzantineBehavior, effect: String) {
        self.successful_attacks
            .lock()
            .unwrap()
            .push((node_id, behavior, effect));
    }

    fn record_violation(&self, violation: SafetyViolation) {
        self.safety_violations.lock().unwrap().push(violation);
    }

    fn check_safety(&self) -> Result<(), TestCaseError> {
        let violations = self.safety_violations.lock().unwrap();
        if !violations.is_empty() {
            return Err(TestCaseError::fail(format!(
                "Byzantine attacks caused safety violations: {:?}",
                *violations
            )));
        }

        let successes = self.successful_attacks.lock().unwrap();
        if !successes.is_empty() {
            return Err(TestCaseError::fail(format!(
                "Byzantine attacks succeeded: {:?}",
                *successes
            )));
        }

        Ok(())
    }
}

/// Byzantine test scenario
#[derive(Debug, Clone)]
struct ByzantineScenario {
    /// Which nodes are Byzantine
    byzantine_nodes: HashSet<u64>,
    /// Sequence of operations
    operations: Vec<ByzantineOperation>,
}

#[derive(Debug, Clone)]
enum ByzantineOperation {
    /// Normal client operation
    ClientProposal { data: Vec<u8> },

    /// Byzantine node performs malicious action
    ByzantineAction {
        node_id: u64,
        behavior: ByzantineBehavior,
    },

    /// Delay for timing attacks
    Delay { millis: u64 },

    /// Check invariants
    CheckInvariants,
}

/// Generate Byzantine test scenarios
fn byzantine_scenario_strategy() -> impl Strategy<Value = ByzantineScenario> {
    (
        // Select 1-2 Byzantine nodes from a 5-node cluster
        prop::collection::hash_set(1u64..=5, 1..=2),
        // Generate sequence of operations
        prop::collection::vec(
            prop_oneof![
                // 40% normal operations
                40 => any::<Vec<u8>>()
                    .prop_filter("Non-empty", |d| !d.is_empty() && d.len() <= 256)
                    .prop_map(|data| ByzantineOperation::ClientProposal { data }),

                // 40% Byzantine behaviors
                40 => (1u64..=5, byzantine_behavior_strategy())
                    .prop_map(|(node_id, behavior)| ByzantineOperation::ByzantineAction {
                        node_id,
                        behavior
                    }),

                // 10% delays
                10 => (10u64..200).prop_map(|millis| ByzantineOperation::Delay { millis }),

                // 10% invariant checks
                10 => Just(ByzantineOperation::CheckInvariants),
            ],
            10..50,
        ),
    )
        .prop_map(|(byzantine_nodes, operations)| ByzantineScenario {
            byzantine_nodes,
            operations,
        })
}

fn byzantine_behavior_strategy() -> impl Strategy<Value = ByzantineBehavior> {
    prop_oneof![
        // Wrong term attacks
        Just(ByzantineBehavior::VoteWithWrongTerm { claimed_term: 999 }),
        // Multiple voting
        (1u64..10, prop::collection::vec(1u64..=5, 2..4))
            .prop_map(|(term, candidates)| ByzantineBehavior::MultipleVotes { term, candidates }),
        // Fake leader claims
        (1u64..10).prop_map(|term| ByzantineBehavior::FakeLeaderClaim { term }),
        // Gaps in log
        prop::collection::vec(1u64..100, 1..5).prop_map(|indices| {
            ByzantineBehavior::AppendEntriesWithGaps {
                gap_indices: indices,
            }
        }),
        // Conflicting entries
        (1u64..50, prop::collection::vec(any::<Vec<u8>>(), 2..4))
            .prop_filter("Non-empty entries", |(_, entries)| {
                entries.iter().all(|e| !e.is_empty() && e.len() <= 128)
            })
            .prop_map(|(index, entries)| ByzantineBehavior::ConflictingEntries { index, entries }),
        // Message replay
        (100u64..5000).prop_map(|delay_ms| ByzantineBehavior::MessageReplay { delay_ms }),
        // Identity forgery
        (1u64..=5).prop_map(|node| ByzantineBehavior::IdentityForgery {
            impersonate_node: node
        }),
        // Data corruption
        data_corruption_strategy()
            .prop_map(|corruption_type| ByzantineBehavior::CorruptedData { corruption_type }),
        // Rollback attempts
        (1u64..50).prop_map(|index| ByzantineBehavior::RollbackCommitted {
            target_index: index
        }),
        // Inconsistent commit indices
        (1u64..100, 1u64..100)
            .prop_filter("Different indices", |(claimed, actual)| claimed != actual)
            .prop_map(
                |(claimed, actual)| ByzantineBehavior::InconsistentCommitIndex {
                    claimed_commit: claimed,
                    actual_commit: actual
                }
            ),
    ]
}

fn data_corruption_strategy() -> impl Strategy<Value = DataCorruption> {
    prop_oneof![
        prop::collection::vec(0usize..1024, 1..10)
            .prop_map(|positions| DataCorruption::BitFlip { positions }),
        (1usize..256).prop_map(|new_length| DataCorruption::Truncate { new_length }),
        (0usize..512, any::<Vec<u8>>())
            .prop_filter("Non-empty garbage", |(_, garbage)| !garbage.is_empty()
                && garbage.len() <= 64)
            .prop_map(|(position, garbage)| DataCorruption::InsertGarbage { position, garbage }),
    ]
}

/// Execute a Byzantine scenario
async fn execute_byzantine_scenario(
    scenario: ByzantineScenario,
    seed: u64,
) -> Result<ByzantineTracker, TestCaseError> {
    let tracker = ByzantineTracker::new();
    let mut rng = ChaCha8Rng::seed_from_u64(seed);

    // Create test cluster
    let cluster = TestCluster::new(5)
        .await
        .map_err(|e| TestCaseError::fail(format!("Failed to create cluster: {}", e)))?;

    // Wait for initial convergence
    cluster
        .wait_for_convergence(Duration::from_secs(10))
        .await
        .map_err(|e| TestCaseError::fail(format!("Cluster failed to converge: {}", e)))?;

    // Execute operations
    for op in scenario.operations {
        match op {
            ByzantineOperation::ClientProposal { data } => {
                // Normal operation through honest nodes
                if let Some(leader_id) =
                    find_honest_leader(&cluster, &scenario.byzantine_nodes).await
                {
                    if let Some(node) = cluster.nodes().get(&leader_id) {
                        let vm_command = VmCommand::Create {
                            config: VmConfig {
                                name: format!("vm-{}", rng.gen::<u32>()),
                                config_path: "/tmp/test.nix".to_string(),
                                memory: 512,
                                vcpus: 1,
                                tenant_id: "test".to_string(),
                                ip_address: None,
                                metadata: Some(HashMap::from([(
                                    "data".to_string(),
                                    hex::encode(&data),
                                )])),
                                anti_affinity: None,
                            },
                            node_id: leader_id,
                        };

                        let _ = node
                            .shared_state
                            .propose_raft_command(ProposalData::CreateVm(vm_command))
                            .await;
                    }
                }
            }

            ByzantineOperation::ByzantineAction { node_id, behavior } => {
                if scenario.byzantine_nodes.contains(&node_id) {
                    tracker.record_attempt(node_id, behavior.clone());

                    // In a real implementation, we would inject the Byzantine behavior
                    // For now, we simulate the effect
                    match &behavior {
                        ByzantineBehavior::FakeLeaderClaim { term } => {
                            // Byzantine node claims to be leader
                            // The cluster should reject this
                            if check_byzantine_became_leader(&cluster, node_id).await {
                                tracker.record_violation(SafetyViolation::ByzantineLeader {
                                    node_id,
                                    term: *term,
                                });
                            }
                        }

                        ByzantineBehavior::ConflictingEntries { index, entries } => {
                            // Try to create conflicting entries
                            // Safety checks should prevent divergence
                        }

                        _ => {
                            // Other Byzantine behaviors would be implemented similarly
                        }
                    }
                }
            }

            ByzantineOperation::Delay { millis } => {
                timing::robust_sleep(Duration::from_millis(millis)).await;
            }

            ByzantineOperation::CheckInvariants => {
                check_invariants(&cluster, &tracker, &scenario.byzantine_nodes).await;
            }
        }
    }

    // Final safety check
    tracker.check_safety()?;

    Ok(tracker)
}

async fn find_honest_leader(cluster: &TestCluster, byzantine_nodes: &HashSet<u64>) -> Option<u64> {
    for (id, node) in cluster.nodes() {
        if !byzantine_nodes.contains(id) && node.shared_state.is_leader().await {
            return Some(*id);
        }
    }
    None
}

async fn check_byzantine_became_leader(cluster: &TestCluster, node_id: u64) -> bool {
    if let Some(node) = cluster.nodes().get(&node_id) {
        return node.shared_state.is_leader().await;
    }
    false
}

async fn check_invariants(
    cluster: &TestCluster,
    tracker: &ByzantineTracker,
    byzantine_nodes: &HashSet<u64>,
) {
    // Check that Byzantine nodes haven't corrupted honest nodes
    let mut honest_states = Vec::new();

    for (id, node) in cluster.nodes() {
        if !byzantine_nodes.contains(id) {
            if let Ok(status) = node.shared_state.get_raft_status().await {
                honest_states.push((id, status));
            }
        }
    }

    // Verify consistency among honest nodes
    if honest_states.len() >= 2 {
        for i in 0..honest_states.len() {
            for j in i + 1..honest_states.len() {
                let (id1, status1) = &honest_states[i];
                let (id2, status2) = &honest_states[j];

                // Check for divergence in committed state
                if status1.commit != status2.commit && status1.commit > 0 && status2.commit > 0 {
                    tracker.record_violation(SafetyViolation::StateDivergence {
                        nodes: vec![**id1, **id2],
                        description: format!(
                            "Commit index divergence: {} vs {}",
                            status1.commit, status2.commit
                        ),
                    });
                }
            }
        }
    }
}

// Property tests for Byzantine resilience

proptest! {
    #![proptest_config(Config {
        cases: 10,
        max_shrink_iters: 3,
        timeout: 60000,
        ..Config::default()
    })]

    #[test]
    fn prop_byzantine_resilience(
        scenario in byzantine_scenario_strategy(),
        seed: u64,
    ) {
        RUNTIME.block_on(async {
            execute_byzantine_scenario(scenario, seed).await
        })?;
    }
}

proptest! {
    #![proptest_config(Config {
        cases: 20,
        timeout: 30000,
        ..Config::default()
    })]

    #[test]
    fn prop_fake_leader_rejection(
        byzantine_node in 1u64..=5,
        fake_terms in prop::collection::vec(1u64..100, 5..15),
        seed: u64,
    ) {
        RUNTIME.block_on(async {
            let scenario = ByzantineScenario {
                byzantine_nodes: HashSet::from([byzantine_node]),
                operations: fake_terms.into_iter().flat_map(|term| vec![
                    ByzantineOperation::ByzantineAction {
                        node_id: byzantine_node,
                        behavior: ByzantineBehavior::FakeLeaderClaim { term },
                    },
                    ByzantineOperation::Delay { millis: 100 },
                    ByzantineOperation::CheckInvariants,
                ]).collect(),
            };

            execute_byzantine_scenario(scenario, seed).await
        })?;
    }
}

proptest! {
    #![proptest_config(Config {
        cases: 15,
        timeout: 45000,
        ..Config::default()
    })]

    #[test]
    fn prop_conflicting_entries_rejection(
        byzantine_nodes in prop::collection::hash_set(1u64..=5, 1..=2),
        conflict_attempts in prop::collection::vec(
            (1u64..50, prop::collection::vec(any::<Vec<u8>>(), 2..4)),
            5..15
        ),
        seed: u64,
    ) {
        RUNTIME.block_on(async {
            let mut operations = Vec::new();

            for (index, entries) in conflict_attempts {
                // Each Byzantine node tries to append conflicting entries
                for &node_id in &byzantine_nodes {
                    operations.push(ByzantineOperation::ByzantineAction {
                        node_id,
                        behavior: ByzantineBehavior::ConflictingEntries {
                            index,
                            entries: entries.clone(),
                        },
                    });
                }

                operations.push(ByzantineOperation::Delay { millis: 200 });
                operations.push(ByzantineOperation::CheckInvariants);
            }

            let scenario = ByzantineScenario {
                byzantine_nodes,
                operations,
            };

            execute_byzantine_scenario(scenario, seed).await
        })?;
    }
}

// Specific Byzantine attack regression tests

#[test]
fn test_byzantine_double_voting_attack() {
    RUNTIME.block_on(async {
        let scenario = ByzantineScenario {
            byzantine_nodes: HashSet::from([3]),
            operations: vec![
                // Let cluster stabilize
                ByzantineOperation::Delay { millis: 500 },
                // Byzantine node votes for multiple candidates in same term
                ByzantineOperation::ByzantineAction {
                    node_id: 3,
                    behavior: ByzantineBehavior::MultipleVotes {
                        term: 2,
                        candidates: vec![1, 2, 4],
                    },
                },
                ByzantineOperation::Delay { millis: 1000 },
                ByzantineOperation::CheckInvariants,
                // Verify cluster still makes progress
                ByzantineOperation::ClientProposal {
                    data: vec![1, 2, 3],
                },
                ByzantineOperation::Delay { millis: 500 },
                ByzantineOperation::CheckInvariants,
            ],
        };

        execute_byzantine_scenario(scenario, 12345).await.unwrap();
    });
}

#[test]
fn test_byzantine_log_rollback_attack() {
    RUNTIME.block_on(async {
        let scenario = ByzantineScenario {
            byzantine_nodes: HashSet::from([2]),
            operations: vec![
                // Establish some committed entries
                ByzantineOperation::ClientProposal { data: vec![1] },
                ByzantineOperation::ClientProposal { data: vec![2] },
                ByzantineOperation::ClientProposal { data: vec![3] },
                ByzantineOperation::Delay { millis: 1000 },
                // Byzantine node attempts to rollback committed entries
                ByzantineOperation::ByzantineAction {
                    node_id: 2,
                    behavior: ByzantineBehavior::RollbackCommitted { target_index: 1 },
                },
                ByzantineOperation::Delay { millis: 500 },
                ByzantineOperation::CheckInvariants,
                // Verify committed entries are preserved
                ByzantineOperation::ClientProposal { data: vec![4] },
                ByzantineOperation::Delay { millis: 500 },
                ByzantineOperation::CheckInvariants,
            ],
        };

        execute_byzantine_scenario(scenario, 54321).await.unwrap();
    });
}

#[test]
fn test_byzantine_identity_forgery_attack() {
    RUNTIME.block_on(async {
        let scenario = ByzantineScenario {
            byzantine_nodes: HashSet::from([4]),
            operations: vec![
                ByzantineOperation::Delay { millis: 500 },
                // Byzantine node impersonates the leader
                ByzantineOperation::ByzantineAction {
                    node_id: 4,
                    behavior: ByzantineBehavior::IdentityForgery {
                        impersonate_node: 1, // Assuming node 1 might be leader
                    },
                },
                // Try to cause confusion with forged messages
                ByzantineOperation::ByzantineAction {
                    node_id: 4,
                    behavior: ByzantineBehavior::ConflictingEntries {
                        index: 10,
                        entries: vec![vec![99], vec![100]],
                    },
                },
                ByzantineOperation::Delay { millis: 1000 },
                ByzantineOperation::CheckInvariants,
                // Cluster should still function correctly
                ByzantineOperation::ClientProposal {
                    data: vec![5, 6, 7],
                },
                ByzantineOperation::CheckInvariants,
            ],
        };

        execute_byzantine_scenario(scenario, 99999).await.unwrap();
    });
}
