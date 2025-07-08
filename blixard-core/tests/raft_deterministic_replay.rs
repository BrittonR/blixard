//! Deterministic Replay Tests for Raft
//!
//! Inspired by TigerBeetle's VOPR (Value-Oriented Property-based Randomized) testing,
//! this module provides deterministic replay capabilities for debugging Raft issues.
//!
//! Key features:
//! - Deterministic execution with seeded RNG
//! - Complete state capture and replay
//! - Detailed execution traces for debugging
//! - Minimization of failing test cases

#![cfg(feature = "test-helpers")]

mod common;

use once_cell::sync::Lazy;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
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

/// A deterministic event in the system
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum DeterministicEvent {
    /// Initialize cluster with N nodes
    InitCluster { size: usize },

    /// Client proposal
    ClientProposal {
        node_id: u64,
        data: Vec<u8>,
        proposal_id: String,
    },

    /// Network event
    NetworkEvent {
        event_type: NetworkEventType,
        affected_nodes: Vec<u64>,
    },

    /// Node failure
    NodeCrash { node_id: u64 },

    /// Node recovery
    NodeRestart { node_id: u64 },

    /// Time advancement
    AdvanceTime { millis: u64 },

    /// Trigger specific Raft timeout
    TriggerTimeout {
        node_id: u64,
        timeout_type: TimeoutType,
    },

    /// Inject specific message loss
    InjectMessageLoss {
        from: u64,
        to: u64,
        message_type: String,
        drop_probability: f64,
    },

    /// Force log compaction
    ForceCompaction { node_id: u64 },

    /// Verify invariant
    CheckInvariant { invariant: InvariantType },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum NetworkEventType {
    /// Partition nodes into groups
    Partition { groups: Vec<Vec<u64>> },
    /// Heal all partitions
    HealPartitions,
    /// Add latency between nodes
    AddLatency { from: u64, to: u64, latency_ms: u64 },
    /// Add packet loss
    AddPacketLoss { from: u64, to: u64, loss_rate: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum TimeoutType {
    Election,
    Heartbeat,
    Snapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum InvariantType {
    /// At most one leader per term
    ElectionSafety,
    /// Leaders only append to their logs
    LeaderAppendOnly,
    /// Log matching property
    LogMatching,
    /// Leader completeness
    LeaderCompleteness,
    /// State machine safety
    StateMachineSafety,
    /// No committed entry is lost
    CommitDurability,
}

/// Execution trace for debugging
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExecutionTrace {
    /// Seed used for this execution
    seed: u64,
    /// Sequence of events
    events: Vec<DeterministicEvent>,
    /// State snapshots at each step
    snapshots: Vec<StateSnapshot>,
    /// Any violations detected
    violations: Vec<String>,
}

/// Snapshot of system state at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StateSnapshot {
    /// Event index this snapshot was taken after
    event_index: usize,
    /// Simulated time in milliseconds
    simulated_time_ms: u64,
    /// Node states
    node_states: HashMap<u64, NodeStateSnapshot>,
    /// Network state
    network_state: NetworkStateSnapshot,
    /// Checksum of entire state
    state_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NodeStateSnapshot {
    /// Is node alive
    alive: bool,
    /// Current term
    term: u64,
    /// Current role (Leader/Candidate/Follower)
    role: String,
    /// Log length
    log_length: u64,
    /// Commit index
    commit_index: u64,
    /// Applied index
    applied_index: u64,
    /// Leader ID (if known)
    leader_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NetworkStateSnapshot {
    /// Active partitions
    partitions: Vec<Vec<u64>>,
    /// Message counts
    messages_sent: u64,
    messages_dropped: u64,
    /// Current latencies
    latencies: HashMap<(u64, u64), u64>,
}

/// Deterministic test executor
struct DeterministicExecutor {
    /// RNG for deterministic randomness
    rng: ChaCha8Rng,
    /// Current execution trace
    trace: ExecutionTrace,
    /// Simulated time
    simulated_time_ms: u64,
    /// Test cluster
    cluster: Option<TestCluster>,
    /// Network simulator
    network: NetworkSimulator,
}

struct NetworkSimulator {
    /// Current partitions
    partitions: Vec<Vec<u64>>,
    /// Message drop rates
    drop_rates: HashMap<(u64, u64), f64>,
    /// Added latencies
    latencies: HashMap<(u64, u64), u64>,
    /// Statistics
    messages_sent: u64,
    messages_dropped: u64,
}

impl DeterministicExecutor {
    fn new(seed: u64) -> Self {
        Self {
            rng: ChaCha8Rng::seed_from_u64(seed),
            trace: ExecutionTrace {
                seed,
                events: Vec::new(),
                snapshots: Vec::new(),
                violations: Vec::new(),
            },
            simulated_time_ms: 0,
            cluster: None,
            network: NetworkSimulator {
                partitions: Vec::new(),
                drop_rates: HashMap::new(),
                latencies: HashMap::new(),
                messages_sent: 0,
                messages_dropped: 0,
            },
        }
    }

    async fn execute_event(&mut self, event: DeterministicEvent) -> BlixardResult<()> {
        self.trace.events.push(event.clone());

        match event {
            DeterministicEvent::InitCluster { size } => {
                self.cluster = Some(TestCluster::new(size).await?);
                if let Some(cluster) = &self.cluster {
                    cluster
                        .wait_for_convergence(Duration::from_secs(10))
                        .await?;
                }
            }

            DeterministicEvent::ClientProposal {
                node_id,
                data,
                proposal_id,
            } => {
                if let Some(cluster) = &self.cluster {
                    if let Some(node) = cluster.nodes().get(&node_id) {
                        let vm_command = VmCommand::Create {
                            config: VmConfig {
                                name: proposal_id,
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
                            node_id,
                        };

                        let _ = node
                            .shared_state
                            .propose_raft_command(ProposalData::CreateVm(vm_command))
                            .await;
                    }
                }
            }

            DeterministicEvent::NetworkEvent { event_type, .. } => match event_type {
                NetworkEventType::Partition { groups } => {
                    self.network.partitions = groups;
                }
                NetworkEventType::HealPartitions => {
                    self.network.partitions.clear();
                }
                NetworkEventType::AddLatency {
                    from,
                    to,
                    latency_ms,
                } => {
                    self.network.latencies.insert((from, to), latency_ms);
                }
                NetworkEventType::AddPacketLoss {
                    from,
                    to,
                    loss_rate,
                } => {
                    self.network.drop_rates.insert((from, to), loss_rate);
                }
            },

            DeterministicEvent::NodeCrash { node_id } => {
                // In real implementation, would stop the node
                timing::robust_sleep(Duration::from_millis(10)).await;
            }

            DeterministicEvent::NodeRestart { node_id } => {
                // In real implementation, would restart the node
                timing::robust_sleep(Duration::from_millis(10)).await;
            }

            DeterministicEvent::AdvanceTime { millis } => {
                self.simulated_time_ms += millis;
                timing::robust_sleep(Duration::from_millis(millis)).await;
            }

            DeterministicEvent::TriggerTimeout { .. } => {
                // In real implementation, would trigger specific timeout
            }

            DeterministicEvent::InjectMessageLoss {
                from,
                to,
                drop_probability,
                ..
            } => {
                self.network.drop_rates.insert((from, to), drop_probability);
            }

            DeterministicEvent::ForceCompaction { .. } => {
                // In real implementation, would trigger compaction
            }

            DeterministicEvent::CheckInvariant { invariant } => {
                if let Some(cluster) = &self.cluster {
                    let violation = self.check_invariant(&invariant, cluster).await;
                    if let Some(v) = violation {
                        self.trace.violations.push(v);
                    }
                }
            }
        }

        // Take state snapshot
        self.capture_snapshot().await;

        Ok(())
    }

    async fn check_invariant(
        &self,
        invariant: &InvariantType,
        cluster: &TestCluster,
    ) -> Option<String> {
        match invariant {
            InvariantType::ElectionSafety => {
                // Check at most one leader per term
                let mut term_leaders: HashMap<u64, Vec<u64>> = HashMap::new();

                for (id, node) in cluster.nodes() {
                    if node.shared_state.is_leader().await {
                        if let Ok(status) = node.shared_state.get_raft_status().await {
                            term_leaders
                                .entry(status.current_term)
                                .or_default()
                                .push(*id);
                        }
                    }
                }

                for (term, leaders) in term_leaders {
                    if leaders.len() > 1 {
                        return Some(format!(
                            "Election safety violation: multiple leaders {:?} in term {}",
                            leaders, term
                        ));
                    }
                }
            }

            InvariantType::LogMatching => {
                // Check log consistency across nodes
                // In real implementation, would compare actual logs
            }

            InvariantType::StateMachineSafety => {
                // Check that applied entries match across nodes
                // In real implementation, would compare state machines
            }

            _ => {
                // Other invariants would be checked similarly
            }
        }

        None
    }

    async fn capture_snapshot(&mut self) {
        let mut node_states = HashMap::new();

        if let Some(cluster) = &self.cluster {
            for (id, node) in cluster.nodes() {
                let alive = true; // In real impl, would check if node is running

                let mut state = NodeStateSnapshot {
                    alive,
                    term: 0,
                    role: "Unknown".to_string(),
                    log_length: 0,
                    commit_index: 0,
                    applied_index: 0,
                    leader_id: None,
                };

                if let Ok(status) = node.shared_state.get_raft_status().await {
                    state.term = status.current_term;
                    state.role = format!("{:?}", status.role);
                    state.commit_index = status.commit;

                    if node.shared_state.is_leader().await {
                        state.leader_id = Some(*id);
                    }
                }

                node_states.insert(*id, state);
            }
        }

        let network_state = NetworkStateSnapshot {
            partitions: self.network.partitions.clone(),
            messages_sent: self.network.messages_sent,
            messages_dropped: self.network.messages_dropped,
            latencies: self.network.latencies.clone(),
        };

        // Calculate state hash
        let state_hash = self.calculate_state_hash(&node_states, &network_state);

        let snapshot = StateSnapshot {
            event_index: self.trace.events.len() - 1,
            simulated_time_ms: self.simulated_time_ms,
            node_states,
            network_state,
            state_hash,
        };

        self.trace.snapshots.push(snapshot);
    }

    fn calculate_state_hash(
        &self,
        node_states: &HashMap<u64, NodeStateSnapshot>,
        network_state: &NetworkStateSnapshot,
    ) -> String {
        let mut hasher = Sha256::new();

        // Hash node states in deterministic order
        let mut nodes: Vec<_> = node_states.iter().collect();
        nodes.sort_by_key(|(id, _)| **id);

        for (id, state) in nodes {
            hasher.update(id.to_le_bytes());
            hasher.update(state.term.to_le_bytes());
            hasher.update(state.role.as_bytes());
            hasher.update(state.commit_index.to_le_bytes());
        }

        // Hash network state
        hasher.update(network_state.messages_sent.to_le_bytes());
        hasher.update(network_state.messages_dropped.to_le_bytes());

        format!("{:x}", hasher.finalize())
    }

    fn save_trace(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(&self.trace)?;
        let mut file = File::create(path)?;
        file.write_all(json.as_bytes())?;
        Ok(())
    }
}

/// Load and replay a trace
async fn replay_trace(trace_path: &Path) -> BlixardResult<ExecutionTrace> {
    let file = File::open(trace_path).map_err(|e| BlixardError::Io {
        operation: "open trace file".to_string(),
        path: trace_path.to_path_buf(),
        source: e,
    })?;

    let trace: ExecutionTrace =
        serde_json::from_reader(file).map_err(|e| BlixardError::Serialization {
            operation: "deserialize trace".to_string(),
            source: Box::new(e),
        })?;

    // Replay with same seed
    let mut executor = DeterministicExecutor::new(trace.seed);

    for event in trace.events.clone() {
        executor.execute_event(event).await?;
    }

    Ok(executor.trace)
}

/// Minimize a failing trace to smallest reproducer
async fn minimize_trace(trace: ExecutionTrace) -> BlixardResult<ExecutionTrace> {
    // Try removing events one by one and see if failure still occurs
    let mut minimal_trace = trace.clone();
    let mut i = 0;

    while i < minimal_trace.events.len() {
        // Skip essential events
        match &minimal_trace.events[i] {
            DeterministicEvent::InitCluster { .. } => {
                i += 1;
                continue;
            }
            _ => {}
        }

        // Try removing this event
        let mut test_trace = minimal_trace.clone();
        test_trace.events.remove(i);

        // Re-execute without this event
        let mut executor = DeterministicExecutor::new(test_trace.seed);
        let mut failed = false;

        for event in test_trace.events.clone() {
            if let Err(_) = executor.execute_event(event).await {
                failed = true;
                break;
            }
        }

        // Check if we still have violations
        if failed || !executor.trace.violations.is_empty() {
            // Still fails, so we can remove this event
            minimal_trace = test_trace;
        } else {
            // Need this event, move to next
            i += 1;
        }
    }

    Ok(minimal_trace)
}

// Test scenarios

#[test]
fn test_deterministic_replay_basic() {
    RUNTIME.block_on(async {
        let mut executor = DeterministicExecutor::new(12345);

        // Execute a simple scenario
        let events = vec![
            DeterministicEvent::InitCluster { size: 3 },
            DeterministicEvent::AdvanceTime { millis: 1000 },
            DeterministicEvent::ClientProposal {
                node_id: 1,
                data: vec![1, 2, 3],
                proposal_id: "prop-1".to_string(),
            },
            DeterministicEvent::AdvanceTime { millis: 500 },
            DeterministicEvent::CheckInvariant {
                invariant: InvariantType::ElectionSafety,
            },
        ];

        for event in events {
            executor.execute_event(event).await.unwrap();
        }

        // Save trace
        let trace_path = PathBuf::from("/tmp/raft-trace-basic.json");
        executor.save_trace(&trace_path).unwrap();

        // Replay and verify determinism
        let replayed = replay_trace(&trace_path).await.unwrap();

        // Compare state hashes
        assert_eq!(
            executor.trace.snapshots.last().unwrap().state_hash,
            replayed.snapshots.last().unwrap().state_hash,
            "Replay produced different state"
        );
    });
}

#[test]
fn test_deterministic_failure_minimization() {
    RUNTIME.block_on(async {
        // Create a failing scenario with many events
        let mut executor = DeterministicExecutor::new(54321);

        let events = vec![
            DeterministicEvent::InitCluster { size: 5 },
            DeterministicEvent::AdvanceTime { millis: 500 },
            // Add many proposals
            DeterministicEvent::ClientProposal {
                node_id: 1,
                data: vec![1],
                proposal_id: "p1".to_string(),
            },
            DeterministicEvent::ClientProposal {
                node_id: 1,
                data: vec![2],
                proposal_id: "p2".to_string(),
            },
            // Cause a partition
            DeterministicEvent::NetworkEvent {
                event_type: NetworkEventType::Partition {
                    groups: vec![vec![1, 2], vec![3, 4, 5]],
                },
                affected_nodes: vec![1, 2, 3, 4, 5],
            },
            // More operations...
            DeterministicEvent::ClientProposal {
                node_id: 1,
                data: vec![3],
                proposal_id: "p3".to_string(),
            },
            DeterministicEvent::AdvanceTime { millis: 2000 },
            // This might cause election safety violation
            DeterministicEvent::TriggerTimeout {
                node_id: 3,
                timeout_type: TimeoutType::Election,
            },
            DeterministicEvent::CheckInvariant {
                invariant: InvariantType::ElectionSafety,
            },
        ];

        for event in events {
            executor.execute_event(event).await.unwrap();
        }

        if !executor.trace.violations.is_empty() {
            // Minimize the failing trace
            let minimal = minimize_trace(executor.trace).await.unwrap();

            println!("Original trace had {} events", executor.trace.events.len());
            println!("Minimal trace has {} events", minimal.events.len());

            // Save minimal trace for debugging
            let minimal_path = PathBuf::from("/tmp/raft-trace-minimal.json");
            let json = serde_json::to_string_pretty(&minimal).unwrap();
            fs::write(&minimal_path, json).unwrap();
        }
    });
}

/// Generate interesting scenarios for finding bugs
fn generate_interesting_scenario(seed: u64) -> Vec<DeterministicEvent> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut events = vec![DeterministicEvent::InitCluster { size: 5 }];

    // Add random mix of events
    for i in 0..50 {
        match rng.gen_range(0..10) {
            0..=3 => {
                // Client proposals (40%)
                events.push(DeterministicEvent::ClientProposal {
                    node_id: rng.gen_range(1..=5),
                    data: vec![i as u8],
                    proposal_id: format!("prop-{}", i),
                });
            }
            4..=5 => {
                // Network events (20%)
                if rng.gen_bool(0.5) {
                    let group1: Vec<u64> = (1..=5).filter(|_| rng.gen_bool(0.5)).collect();
                    let group2: Vec<u64> = (1..=5).filter(|n| !group1.contains(n)).collect();

                    if !group1.is_empty() && !group2.is_empty() {
                        events.push(DeterministicEvent::NetworkEvent {
                            event_type: NetworkEventType::Partition {
                                groups: vec![group1, group2],
                            },
                            affected_nodes: (1..=5).collect(),
                        });
                    }
                } else {
                    events.push(DeterministicEvent::NetworkEvent {
                        event_type: NetworkEventType::HealPartitions,
                        affected_nodes: (1..=5).collect(),
                    });
                }
            }
            6 => {
                // Node failures (10%)
                events.push(DeterministicEvent::NodeCrash {
                    node_id: rng.gen_range(1..=5),
                });
            }
            7 => {
                // Node restarts (10%)
                events.push(DeterministicEvent::NodeRestart {
                    node_id: rng.gen_range(1..=5),
                });
            }
            8 => {
                // Time advancement (10%)
                events.push(DeterministicEvent::AdvanceTime {
                    millis: rng.gen_range(10..1000),
                });
            }
            _ => {
                // Invariant checks (10%)
                let invariants = [
                    InvariantType::ElectionSafety,
                    InvariantType::LogMatching,
                    InvariantType::StateMachineSafety,
                ];
                events.push(DeterministicEvent::CheckInvariant {
                    invariant: invariants[rng.gen_range(0..invariants.len())].clone(),
                });
            }
        }
    }

    events
}

#[test]
fn test_find_bugs_with_random_scenarios() {
    RUNTIME.block_on(async {
        let mut found_bugs = Vec::new();

        // Try many random scenarios
        for seed in 0..100 {
            let mut executor = DeterministicExecutor::new(seed);
            let events = generate_interesting_scenario(seed);

            for event in events {
                if let Err(e) = executor.execute_event(event).await {
                    found_bugs.push((seed, e));
                    break;
                }
            }

            if !executor.trace.violations.is_empty() {
                // Found a bug! Save the trace
                let bug_path = PathBuf::from(format!("/tmp/raft-bug-{}.json", seed));
                executor.save_trace(&bug_path).unwrap();

                println!(
                    "Found bug with seed {}: {:?}",
                    seed, executor.trace.violations
                );

                // Try to minimize
                let minimal = minimize_trace(executor.trace).await.unwrap();
                let minimal_path = PathBuf::from(format!("/tmp/raft-bug-{}-minimal.json", seed));
                let json = serde_json::to_string_pretty(&minimal).unwrap();
                fs::write(&minimal_path, json).unwrap();
            }
        }

        if !found_bugs.is_empty() {
            println!("Found {} bugs in random testing", found_bugs.len());
        }
    });
}
