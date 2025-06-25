//! Performance metrics and observability for Blixard
//!
//! This module provides comprehensive metrics collection for monitoring
//! the health and performance of the distributed system.

use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock;
use std::collections::HashMap;

/// Metrics registry for collecting system metrics
#[derive(Debug, Clone)]
pub struct MetricsRegistry {
    counters: Arc<RwLock<HashMap<String, u64>>>,
    gauges: Arc<RwLock<HashMap<String, f64>>>,
    histograms: Arc<RwLock<HashMap<String, Vec<Duration>>>>,
}

impl MetricsRegistry {
    /// Create a new metrics registry
    pub fn new() -> Self {
        Self {
            counters: Arc::new(RwLock::new(HashMap::new())),
            gauges: Arc::new(RwLock::new(HashMap::new())),
            histograms: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Increment a counter by 1
    pub fn increment_counter(&self, name: &str) {
        self.increment_counter_by(name, 1);
    }

    /// Increment a counter by a specific amount
    pub fn increment_counter_by(&self, name: &str, value: u64) {
        let mut counters = self.counters.write();
        *counters.entry(name.to_string()).or_insert(0) += value;
    }

    /// Set a gauge to a specific value
    pub fn set_gauge(&self, name: &str, value: f64) {
        let mut gauges = self.gauges.write();
        gauges.insert(name.to_string(), value);
    }

    /// Record a duration in a histogram
    pub fn record_duration(&self, name: &str, duration: Duration) {
        let mut histograms = self.histograms.write();
        histograms.entry(name.to_string()).or_insert_with(Vec::new).push(duration);
    }

    /// Get the current value of a counter
    pub fn get_counter(&self, name: &str) -> u64 {
        self.counters.read().get(name).copied().unwrap_or(0)
    }

    /// Get the current value of a gauge
    pub fn get_gauge(&self, name: &str) -> f64 {
        self.gauges.read().get(name).copied().unwrap_or(0.0)
    }

    /// Get histogram statistics
    pub fn get_histogram_stats(&self, name: &str) -> Option<HistogramStats> {
        let histograms = self.histograms.read();
        let values = histograms.get(name)?;
        
        if values.is_empty() {
            return None;
        }

        let mut sorted_values: Vec<Duration> = values.clone();
        sorted_values.sort();

        let count = sorted_values.len();
        let sum: Duration = sorted_values.iter().sum();
        let mean = sum / count as u32;
        
        let p50_idx = count / 2;
        let p90_idx = (count * 9) / 10;
        let p99_idx = (count * 99) / 100;

        Some(HistogramStats {
            count,
            mean,
            min: sorted_values[0],
            max: sorted_values[count - 1],
            p50: sorted_values[p50_idx.min(count - 1)],
            p90: sorted_values[p90_idx.min(count - 1)],
            p99: sorted_values[p99_idx.min(count - 1)],
        })
    }

    /// Get a snapshot of all metrics
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            counters: self.counters.read().clone(),
            gauges: self.gauges.read().clone(),
            histograms: self.histograms.read()
                .iter()
                .filter_map(|(k, _)| {
                    self.get_histogram_stats(k).map(|stats| (k.clone(), stats))
                })
                .collect(),
        }
    }

    /// Clear all metrics
    pub fn clear(&self) {
        self.counters.write().clear();
        self.gauges.write().clear();
        self.histograms.write().clear();
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for a histogram
#[derive(Debug, Clone)]
pub struct HistogramStats {
    pub count: usize,
    pub mean: Duration,
    pub min: Duration,
    pub max: Duration,
    pub p50: Duration,
    pub p90: Duration,
    pub p99: Duration,
}

/// A snapshot of all metrics at a point in time
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub counters: HashMap<String, u64>,
    pub gauges: HashMap<String, f64>,
    pub histograms: HashMap<String, HistogramStats>,
}

/// Timer for measuring operation durations
pub struct Timer {
    name: String,
    start: Instant,
    registry: MetricsRegistry,
}

impl Timer {
    /// Create a new timer
    pub fn new(name: String, registry: MetricsRegistry) -> Self {
        Self {
            name,
            start: Instant::now(),
            registry,
        }
    }

    /// Stop the timer and record the duration
    pub fn stop(self) {
        let duration = self.start.elapsed();
        self.registry.record_duration(&self.name, duration);
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        // Record duration when timer is dropped if not explicitly stopped
        let duration = self.start.elapsed();
        self.registry.record_duration(&self.name, duration);
    }
}

/// Metric names used throughout the system
pub mod names {
    // Raft metrics
    pub const RAFT_PROPOSALS_TOTAL: &str = "raft.proposals.total";
    pub const RAFT_PROPOSALS_FAILED: &str = "raft.proposals.failed";
    pub const RAFT_COMMITTED_ENTRIES: &str = "raft.committed_entries.total";
    pub const RAFT_APPLIED_ENTRIES: &str = "raft.applied_entries.total";
    pub const RAFT_LEADER_CHANGES: &str = "raft.leader_changes.total";
    pub const RAFT_TERM: &str = "raft.term";
    pub const RAFT_COMMIT_INDEX: &str = "raft.commit_index";
    pub const RAFT_APPLIED_INDEX: &str = "raft.applied_index";
    pub const RAFT_LOG_SIZE: &str = "raft.log_size";
    pub const RAFT_SNAPSHOT_COUNT: &str = "raft.snapshots.total";
    pub const RAFT_SNAPSHOT_SIZE: &str = "raft.snapshot.size_bytes";
    pub const RAFT_COMPACTION_COUNT: &str = "raft.compactions.total";

    // Timing metrics
    pub const RAFT_PROPOSAL_DURATION: &str = "raft.proposal.duration";
    pub const RAFT_COMMIT_DURATION: &str = "raft.commit.duration";
    pub const RAFT_APPLY_DURATION: &str = "raft.apply.duration";
    pub const RAFT_SNAPSHOT_DURATION: &str = "raft.snapshot.duration";
    pub const STORAGE_WRITE_DURATION: &str = "storage.write.duration";
    pub const STORAGE_READ_DURATION: &str = "storage.read.duration";

    // Network metrics
    pub const GRPC_REQUESTS_TOTAL: &str = "grpc.requests.total";
    pub const GRPC_REQUESTS_FAILED: &str = "grpc.requests.failed";
    pub const GRPC_REQUEST_DURATION: &str = "grpc.request.duration";
    pub const PEER_CONNECTIONS_ACTIVE: &str = "peer.connections.active";
    pub const PEER_RECONNECT_ATTEMPTS: &str = "peer.reconnect.attempts";
    pub const PEER_MESSAGES_SENT: &str = "peer.messages.sent";
    pub const PEER_MESSAGES_RECEIVED: &str = "peer.messages.received";
    pub const PEER_MESSAGE_BYTES_SENT: &str = "peer.message.bytes_sent";
    pub const PEER_MESSAGE_BYTES_RECEIVED: &str = "peer.message.bytes_received";

    // VM metrics
    pub const VM_TOTAL: &str = "vm.total";
    pub const VM_RUNNING: &str = "vm.running";
    pub const VM_STOPPED: &str = "vm.stopped";
    pub const VM_FAILED: &str = "vm.failed";
    pub const VM_CREATE_TOTAL: &str = "vm.create.total";
    pub const VM_CREATE_FAILED: &str = "vm.create.failed";
    pub const VM_CREATE_DURATION: &str = "vm.create.duration";
    pub const VM_START_DURATION: &str = "vm.start.duration";
    pub const VM_STOP_DURATION: &str = "vm.stop.duration";
    pub const VM_SCHEDULE_DURATION: &str = "vm.schedule.duration";
    pub const VM_PLACEMENT_DECISIONS: &str = "vm.placement.decisions";

    // Resource metrics
    pub const RESOURCE_CPU_USED: &str = "resource.cpu.used";
    pub const RESOURCE_CPU_TOTAL: &str = "resource.cpu.total";
    pub const RESOURCE_MEMORY_USED: &str = "resource.memory.used";
    pub const RESOURCE_MEMORY_TOTAL: &str = "resource.memory.total";
    pub const RESOURCE_DISK_USED: &str = "resource.disk.used";
    pub const RESOURCE_DISK_TOTAL: &str = "resource.disk.total";

    // Task metrics
    pub const TASK_SUBMITTED: &str = "task.submitted";
    pub const TASK_COMPLETED: &str = "task.completed";
    pub const TASK_FAILED: &str = "task.failed";
    pub const TASK_DURATION: &str = "task.duration";
    pub const TASK_QUEUE_SIZE: &str = "task.queue.size";

    // Storage metrics
    pub const STORAGE_DB_SIZE: &str = "storage.db.size_bytes";
    pub const STORAGE_ENTRIES_COUNT: &str = "storage.entries.count";
    pub const STORAGE_COMPACTIONS: &str = "storage.compactions.total";
    pub const STORAGE_WRITES: &str = "storage.writes.total";
    pub const STORAGE_READS: &str = "storage.reads.total";
    pub const STORAGE_DELETES: &str = "storage.deletes.total";

    // System metrics
    pub const NODE_UPTIME: &str = "node.uptime.seconds";
    pub const NODE_STATE_TRANSITIONS: &str = "node.state.transitions";
    pub const CLUSTER_SIZE: &str = "cluster.size";
    pub const CLUSTER_VOTERS: &str = "cluster.voters";
    pub const CLUSTER_LEARNERS: &str = "cluster.learners";
}

/// Global metrics registry instance
static GLOBAL_METRICS: once_cell::sync::Lazy<MetricsRegistry> = 
    once_cell::sync::Lazy::new(MetricsRegistry::new);

/// Get the global metrics registry
pub fn global() -> &'static MetricsRegistry {
    &GLOBAL_METRICS
}

/// Convenience macros for metrics
#[macro_export]
macro_rules! metrics_inc {
    ($name:expr) => {
        $crate::metrics::global().increment_counter($name)
    };
    ($name:expr, $value:expr) => {
        $crate::metrics::global().increment_counter_by($name, $value)
    };
}

#[macro_export]
macro_rules! metrics_gauge {
    ($name:expr, $value:expr) => {
        $crate::metrics::global().set_gauge($name, $value as f64)
    };
}

#[macro_export]
macro_rules! metrics_timer {
    ($name:expr) => {
        $crate::metrics::Timer::new($name.to_string(), $crate::metrics::global().clone())
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_metrics() {
        let metrics = MetricsRegistry::new();
        
        assert_eq!(metrics.get_counter("test.counter"), 0);
        
        metrics.increment_counter("test.counter");
        assert_eq!(metrics.get_counter("test.counter"), 1);
        
        metrics.increment_counter_by("test.counter", 5);
        assert_eq!(metrics.get_counter("test.counter"), 6);
    }

    #[test]
    fn test_gauge_metrics() {
        let metrics = MetricsRegistry::new();
        
        assert_eq!(metrics.get_gauge("test.gauge"), 0.0);
        
        metrics.set_gauge("test.gauge", 42.5);
        assert_eq!(metrics.get_gauge("test.gauge"), 42.5);
        
        metrics.set_gauge("test.gauge", 10.0);
        assert_eq!(metrics.get_gauge("test.gauge"), 10.0);
    }

    #[test]
    fn test_histogram_metrics() {
        let metrics = MetricsRegistry::new();
        
        assert!(metrics.get_histogram_stats("test.histogram").is_none());
        
        // Record some durations
        metrics.record_duration("test.histogram", Duration::from_millis(10));
        metrics.record_duration("test.histogram", Duration::from_millis(20));
        metrics.record_duration("test.histogram", Duration::from_millis(30));
        metrics.record_duration("test.histogram", Duration::from_millis(40));
        metrics.record_duration("test.histogram", Duration::from_millis(50));
        
        let stats = metrics.get_histogram_stats("test.histogram").unwrap();
        assert_eq!(stats.count, 5);
        assert_eq!(stats.min, Duration::from_millis(10));
        assert_eq!(stats.max, Duration::from_millis(50));
        assert_eq!(stats.p50, Duration::from_millis(30));
    }

    #[test]
    fn test_timer() {
        let metrics = MetricsRegistry::new();
        
        {
            let _timer = Timer::new("test.timer".to_string(), metrics.clone());
            std::thread::sleep(Duration::from_millis(10));
        }
        
        let stats = metrics.get_histogram_stats("test.timer").unwrap();
        assert_eq!(stats.count, 1);
        assert!(stats.mean >= Duration::from_millis(10));
    }

    #[test]
    fn test_snapshot() {
        let metrics = MetricsRegistry::new();
        
        metrics.increment_counter("counter1");
        metrics.increment_counter_by("counter2", 5);
        metrics.set_gauge("gauge1", 42.0);
        metrics.record_duration("histogram1", Duration::from_millis(100));
        
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.counters.get("counter1"), Some(&1));
        assert_eq!(snapshot.counters.get("counter2"), Some(&5));
        assert_eq!(snapshot.gauges.get("gauge1"), Some(&42.0));
        assert!(snapshot.histograms.contains_key("histogram1"));
    }
}