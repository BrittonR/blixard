//! State tracking and history management for the fuzzer
//!
//! This module tracks all state transitions, resource usage, and operation
//! latencies to help with debugging and visualization.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// A snapshot of the system state at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    /// Timestamp of this snapshot
    pub timestamp: u128,

    /// Node states
    pub nodes: HashMap<u64, NodeState>,

    /// Current view/term for each node
    pub views: HashMap<u64, u64>,

    /// Commit points for each node
    pub commit_points: HashMap<u64, u64>,

    /// VM states
    pub vms: HashMap<String, VmState>,

    /// Active network partitions
    pub partitions: Vec<NetworkPartition>,

    /// In-flight messages
    pub messages_in_flight: usize,

    /// Resource usage
    pub resources: ResourceUsage,

    /// Active invariant violations
    pub violations: Vec<String>,
}

/// State of a single node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeState {
    pub id: u64,
    pub role: NodeRole,
    pub is_running: bool,
    pub last_heartbeat: u128,
    pub log_length: usize,
    pub applied_index: u64,
    pub clock_skew_ms: i64,
    pub byzantine: Option<String>,
}

/// Node role in the consensus protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeRole {
    Leader,
    Follower,
    Candidate,
    Unknown,
}

/// VM state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmState {
    pub id: String,
    pub status: VmStatus,
    pub cpu: u32,
    pub memory: u32,
    pub host_node: Option<u64>,
}

/// VM status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VmStatus {
    Creating,
    Running,
    Stopped,
    Deleted,
}

/// Network partition information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPartition {
    pub group_a: Vec<u64>,
    pub group_b: Vec<u64>,
}

/// Resource usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub total_cpu_percent: f64,
    pub total_memory_mb: u64,
    pub total_disk_io_mb: u64,
    pub network_bandwidth_mbps: f64,
}

/// An event that occurred during execution
#[derive(Debug, Clone)]
pub struct Event {
    pub timestamp: Instant,
    pub node_id: Option<u64>,
    pub event_type: EventType,
    pub details: String,
}

/// Types of events we track
#[derive(Debug, Clone)]
pub enum EventType {
    // State transitions
    RoleChange { from: NodeRole, to: NodeRole },
    ViewChange { from: u64, to: u64 },
    Commit { index: u64 },

    // Operations
    ClientRequest { request_id: u64 },
    OperationComplete { request_id: u64, duration: Duration },
    OperationFailed { request_id: u64, reason: String },

    // Failures
    NodeCrash,
    NodeRestart,
    NetworkPartitionStart,
    NetworkPartitionHealed,
    MessageDropped { from: u64, to: u64 },

    // Byzantine events
    ByzantineAction { action: String },

    // Anomalies
    AnomalyDetected { description: String },
    InvariantViolation { invariant: String },
}

/// Tracks operation latencies
#[derive(Debug, Default)]
pub struct LatencyTracker {
    /// Latencies for different operation types
    latencies: HashMap<String, Vec<Duration>>,

    /// In-flight operations
    in_flight: HashMap<u64, (String, Instant)>,
}

impl LatencyTracker {
    /// Start tracking an operation
    pub fn start_operation(&mut self, request_id: u64, op_type: String) {
        self.in_flight.insert(request_id, (op_type, Instant::now()));
    }

    /// Complete an operation and record its latency
    pub fn complete_operation(&mut self, request_id: u64) -> Option<Duration> {
        if let Some((op_type, start)) = self.in_flight.remove(&request_id) {
            let duration = start.elapsed();
            self.latencies.entry(op_type).or_default().push(duration);
            Some(duration)
        } else {
            None
        }
    }

    /// Get latency percentiles for an operation type
    pub fn get_percentiles(&self, op_type: &str) -> Option<LatencyPercentiles> {
        self.latencies.get(op_type).map(|durations| {
            let mut sorted = durations.clone();
            sorted.sort();

            let len = sorted.len();
            LatencyPercentiles {
                p50: sorted[len / 2],
                p90: sorted[len * 9 / 10],
                p95: sorted[len * 95 / 100],
                p99: sorted[len * 99 / 100],
                max: sorted[len - 1],
            }
        })
    }
}

/// Latency percentiles
#[derive(Debug, Clone)]
pub struct LatencyPercentiles {
    pub p50: Duration,
    pub p90: Duration,
    pub p95: Duration,
    pub p99: Duration,
    pub max: Duration,
}

/// Main state tracker
pub struct StateTracker {
    /// History of state snapshots
    snapshots: VecDeque<StateSnapshot>,

    /// Maximum snapshots to keep
    max_snapshots: usize,

    /// Event log
    events: Vec<Event>,

    /// Latency tracking
    latency_tracker: LatencyTracker,

    /// Anomaly detection
    anomaly_detector: AnomalyDetector,
}

impl StateTracker {
    /// Create a new state tracker
    pub fn new() -> Self {
        Self {
            snapshots: VecDeque::new(),
            max_snapshots: 1000,
            events: Vec::new(),
            latency_tracker: LatencyTracker::default(),
            anomaly_detector: AnomalyDetector::new(),
        }
    }

    /// Take a snapshot of the current state
    pub fn snapshot(&mut self, state: StateSnapshot) {
        // Check for anomalies
        if let Some(prev) = self.snapshots.back() {
            let anomalies = self.anomaly_detector.check(prev, &state);
            for anomaly in anomalies {
                self.record_event(Event {
                    timestamp: Instant::now(),
                    node_id: None,
                    event_type: EventType::AnomalyDetected {
                        description: anomaly,
                    },
                    details: String::new(),
                });
            }
        }

        // Add snapshot
        self.snapshots.push_back(state);

        // Trim old snapshots
        while self.snapshots.len() > self.max_snapshots {
            self.snapshots.pop_front();
        }
    }

    /// Record an event
    pub fn record_event(&mut self, event: Event) {
        self.events.push(event);
    }

    /// Get recent events
    pub fn recent_events(&self, count: usize) -> &[Event] {
        let start = self.events.len().saturating_sub(count);
        &self.events[start..]
    }

    /// Get state history
    pub fn get_history(&self) -> Vec<StateSnapshot> {
        self.snapshots.iter().cloned().collect()
    }

    /// Get interesting moments (state changes, failures, etc.)
    pub fn get_interesting_moments(&self) -> Vec<InterestingMoment> {
        let mut moments = Vec::new();

        // Look for role changes
        for (i, snapshot) in self.snapshots.iter().enumerate() {
            if i > 0 {
                let prev = &self.snapshots[i - 1];

                // Check for leader changes
                let prev_leaders: Vec<_> = prev
                    .nodes
                    .values()
                    .filter(|n| n.role == NodeRole::Leader)
                    .map(|n| n.id)
                    .collect();

                let curr_leaders: Vec<_> = snapshot
                    .nodes
                    .values()
                    .filter(|n| n.role == NodeRole::Leader)
                    .map(|n| n.id)
                    .collect();

                if prev_leaders != curr_leaders {
                    moments.push(InterestingMoment {
                        timestamp: snapshot.timestamp,
                        description: format!(
                            "Leader change: {:?} -> {:?}",
                            prev_leaders, curr_leaders
                        ),
                        snapshot_index: i,
                    });
                }

                // Check for partitions
                if snapshot.partitions.len() != prev.partitions.len() {
                    moments.push(InterestingMoment {
                        timestamp: snapshot.timestamp,
                        description: format!(
                            "Network partition change: {} -> {} partitions",
                            prev.partitions.len(),
                            snapshot.partitions.len()
                        ),
                        snapshot_index: i,
                    });
                }
            }
        }

        moments
    }
}

/// An interesting moment in the execution
#[derive(Debug, Clone)]
pub struct InterestingMoment {
    pub timestamp: u128,
    pub description: String,
    pub snapshot_index: usize,
}

/// Detects anomalies in state transitions
struct AnomalyDetector {
    /// Expected heartbeat interval
    heartbeat_interval: Duration,

    /// Maximum allowed clock skew
    max_clock_skew: Duration,
}

impl AnomalyDetector {
    fn new() -> Self {
        Self {
            heartbeat_interval: Duration::from_millis(150),
            max_clock_skew: Duration::from_secs(60),
        }
    }

    /// Check for anomalies between two states
    fn check(&self, prev: &StateSnapshot, curr: &StateSnapshot) -> Vec<String> {
        let mut anomalies = Vec::new();

        // Check for missing heartbeats
        for node_id in curr.nodes.keys() {
            if let Some(prev_node) = prev.nodes.get(node_id) {
                let time_diff = curr.timestamp - prev_node.last_heartbeat;
                if time_diff > self.heartbeat_interval.as_nanos() * 10 {
                    anomalies.push(format!(
                        "Node {} missing heartbeats for {}ms",
                        node_id,
                        time_diff / 1_000_000
                    ));
                }
            }
        }

        // Check for extreme clock skew
        for (node_id, node) in &curr.nodes {
            if node.clock_skew_ms.abs() > self.max_clock_skew.as_millis() as i64 {
                anomalies.push(format!(
                    "Node {} has extreme clock skew: {}ms",
                    node_id, node.clock_skew_ms
                ));
            }
        }

        // Check for split brain (multiple leaders)
        let leaders: Vec<_> = curr
            .nodes
            .values()
            .filter(|n| n.role == NodeRole::Leader)
            .map(|n| n.id)
            .collect();

        if leaders.len() > 1 {
            anomalies.push(format!(
                "Split brain detected: multiple leaders {:?}",
                leaders
            ));
        }

        // Check for progress stall
        let max_commit = curr.commit_points.values().max().copied().unwrap_or(0);
        let prev_max_commit = prev.commit_points.values().max().copied().unwrap_or(0);

        if max_commit == prev_max_commit && curr.messages_in_flight == 0 {
            // No progress and no messages in flight
            anomalies
                .push("System appears stalled: no progress and no messages in flight".to_string());
        }

        anomalies
    }
}

impl Default for StateTracker {
    fn default() -> Self {
        Self::new()
    }
}
