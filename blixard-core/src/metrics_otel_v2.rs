//! OpenTelemetry-based metrics (simplified for v0.20)
//!
//! This module provides production-ready metrics using OpenTelemetry v0.20.

use opentelemetry::{
    global,
    metrics::{Counter, Histogram, Meter, UpDownCounter},
    KeyValue,
};
use std::sync::OnceLock;

/// Global metrics instance
static METRICS: OnceLock<Metrics> = OnceLock::new();

/// Container for all application metrics
pub struct Metrics {
    meter: Meter,
    
    // Raft metrics
    pub raft_proposals_total: Counter<u64>,
    pub raft_proposals_failed: Counter<u64>,
    pub raft_committed_entries: Counter<u64>,
    pub raft_applied_entries: Counter<u64>,
    pub raft_leader_changes: Counter<u64>,
    pub raft_term: UpDownCounter<i64>,
    pub raft_commit_index: UpDownCounter<i64>,
    pub raft_proposal_duration: Histogram<f64>,
    
    // Network metrics
    pub grpc_requests_total: Counter<u64>,
    pub grpc_requests_failed: Counter<u64>,
    pub grpc_request_duration: Histogram<f64>,
    pub peer_connections_active: UpDownCounter<i64>,
    pub peer_reconnect_attempts: Counter<u64>,
    
    // VM metrics
    pub vm_total: UpDownCounter<i64>,
    pub vm_running: UpDownCounter<i64>,
    pub vm_create_total: Counter<u64>,
    pub vm_create_failed: Counter<u64>,
    pub vm_create_duration: Histogram<f64>,
    
    // Storage metrics
    pub storage_writes: Counter<u64>,
    pub storage_reads: Counter<u64>,
    pub storage_write_duration: Histogram<f64>,
    pub storage_read_duration: Histogram<f64>,
}

impl Metrics {
    /// Create new metrics instance with the given meter
    fn new(meter: Meter) -> Self {
        Self {
            // Raft metrics
            raft_proposals_total: meter
                .u64_counter("raft.proposals.total")
                .with_description("Total number of Raft proposals")
                .init(),
            raft_proposals_failed: meter
                .u64_counter("raft.proposals.failed")
                .with_description("Number of failed Raft proposals")
                .init(),
            raft_committed_entries: meter
                .u64_counter("raft.committed_entries.total")
                .with_description("Total number of committed Raft entries")
                .init(),
            raft_applied_entries: meter
                .u64_counter("raft.applied_entries.total")
                .with_description("Total number of applied Raft entries")
                .init(),
            raft_leader_changes: meter
                .u64_counter("raft.leader_changes.total")
                .with_description("Number of leader changes")
                .init(),
            raft_term: meter
                .i64_up_down_counter("raft.term")
                .with_description("Current Raft term")
                .init(),
            raft_commit_index: meter
                .i64_up_down_counter("raft.commit_index")
                .with_description("Current Raft commit index")
                .init(),
            raft_proposal_duration: meter
                .f64_histogram("raft.proposal.duration")
                .with_description("Duration of Raft proposals in seconds")
                .init(),
            
            // Network metrics
            grpc_requests_total: meter
                .u64_counter("grpc.requests.total")
                .with_description("Total number of gRPC requests")
                .init(),
            grpc_requests_failed: meter
                .u64_counter("grpc.requests.failed")
                .with_description("Number of failed gRPC requests")
                .init(),
            grpc_request_duration: meter
                .f64_histogram("grpc.request.duration")
                .with_description("Duration of gRPC requests in seconds")
                .init(),
            peer_connections_active: meter
                .i64_up_down_counter("peer.connections.active")
                .with_description("Number of active peer connections")
                .init(),
            peer_reconnect_attempts: meter
                .u64_counter("peer.reconnect.attempts")
                .with_description("Number of peer reconnection attempts")
                .init(),
            
            // VM metrics
            vm_total: meter
                .i64_up_down_counter("vm.total")
                .with_description("Total number of VMs")
                .init(),
            vm_running: meter
                .i64_up_down_counter("vm.running")
                .with_description("Number of running VMs")
                .init(),
            vm_create_total: meter
                .u64_counter("vm.create.total")
                .with_description("Total number of VM create attempts")
                .init(),
            vm_create_failed: meter
                .u64_counter("vm.create.failed")
                .with_description("Number of failed VM creates")
                .init(),
            vm_create_duration: meter
                .f64_histogram("vm.create.duration")
                .with_description("Duration of VM creation in seconds")
                .init(),
            
            // Storage metrics
            storage_writes: meter
                .u64_counter("storage.writes.total")
                .with_description("Total number of storage writes")
                .init(),
            storage_reads: meter
                .u64_counter("storage.reads.total")
                .with_description("Total number of storage reads")
                .init(),
            storage_write_duration: meter
                .f64_histogram("storage.write.duration")
                .with_description("Duration of storage writes in seconds")
                .init(),
            storage_read_duration: meter
                .f64_histogram("storage.read.duration")
                .with_description("Duration of storage reads in seconds")
                .init(),
            
            meter,
        }
    }
}

/// Initialize metrics with Prometheus exporter
pub fn init_prometheus() -> Result<&'static Metrics, Box<dyn std::error::Error>> {
    use opentelemetry_sdk::metrics::MeterProvider;
    
    let registry = prometheus::Registry::new();
    let exporter = opentelemetry_prometheus::exporter()
        .with_registry(registry.clone())
        .build()?;
    
    // Store the registry first since we'll move the exporter
    PROMETHEUS_REGISTRY.set(registry).ok();
    
    let provider = MeterProvider::builder()
        .with_reader(exporter)
        .build();
    
    global::set_meter_provider(provider);
    
    let meter = global::meter("blixard");
    let metrics = Metrics::new(meter);
    
    METRICS.set(metrics).map_err(|_| "Metrics already initialized")?;
    
    Ok(METRICS.get().unwrap())
}


/// Prometheus registry instance
static PROMETHEUS_REGISTRY: OnceLock<prometheus::Registry> = OnceLock::new();

/// Get Prometheus metrics as a string
pub fn prometheus_metrics() -> String {
    use prometheus::{Encoder, TextEncoder};
    
    if let Some(registry) = PROMETHEUS_REGISTRY.get() {
        let mut buffer = Vec::new();
        let encoder = TextEncoder::new();
        let metric_families = registry.gather();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        String::from_utf8(buffer).unwrap_or_else(|_| "# Error encoding metrics\n".to_string())
    } else {
        String::from("# Metrics not initialized\n")
    }
}

/// Initialize metrics without exporter (for testing)
pub fn init_noop() -> Result<&'static Metrics, Box<dyn std::error::Error>> {
    let meter = global::meter("blixard");
    let metrics = Metrics::new(meter);
    
    METRICS.set(metrics).map_err(|_| "Metrics already initialized")?;
    Ok(METRICS.get().unwrap())
}

/// Get the global metrics instance
pub fn metrics() -> &'static Metrics {
    METRICS.get().expect("Metrics not initialized. Call init_prometheus() or init_noop() first.")
}

/// Timer guard for recording operation duration
pub struct Timer {
    start: std::time::Instant,
    histogram: Histogram<f64>,
    attributes: Vec<KeyValue>,
}

impl Timer {
    /// Create a new timer
    pub fn new(histogram: Histogram<f64>) -> Self {
        Self {
            start: std::time::Instant::now(),
            histogram,
            attributes: vec![],
        }
    }
    
    /// Create a timer with attributes
    pub fn with_attributes(histogram: Histogram<f64>, attributes: Vec<KeyValue>) -> Self {
        Self {
            start: std::time::Instant::now(),
            histogram,
            attributes,
        }
    }
    
    /// Record the duration and consume the timer
    pub fn record(self) {
        let duration = self.start.elapsed().as_secs_f64();
        self.histogram.record(duration, &self.attributes);
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        let duration = self.start.elapsed().as_secs_f64();
        self.histogram.record(duration, &self.attributes);
    }
}

/// Common attribute keys
pub mod attributes {
    use opentelemetry::KeyValue;
    
    pub fn node_id(id: u64) -> KeyValue {
        KeyValue::new("node.id", id as i64)
    }
    
    pub fn peer_id(id: u64) -> KeyValue {
        KeyValue::new("peer.id", id as i64)
    }
    
    pub fn vm_name(name: &str) -> KeyValue {
        KeyValue::new("vm.name", name.to_string())
    }
    
    pub fn table(name: &str) -> KeyValue {
        KeyValue::new("table", name.to_string())
    }
    
    pub fn operation(name: &str) -> KeyValue {
        KeyValue::new("operation", name.to_string())
    }
    
    pub fn method(name: &str) -> KeyValue {
        KeyValue::new("method", name.to_string())
    }
    
    pub fn status(value: &str) -> KeyValue {
        KeyValue::new("status", value.to_string())
    }
    
    pub fn error(value: bool) -> KeyValue {
        KeyValue::new("error", value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_metrics_initialization() {
        let result = init_noop();
        assert!(result.is_ok());
        
        let metrics = metrics();
        
        // Test counter
        metrics.raft_proposals_total.add(1, &[]);
        
        // Test up-down counter
        metrics.raft_term.add(5, &[]);
        
        // Test histogram with timer
        {
            let _timer = Timer::new(metrics.raft_proposal_duration.clone());
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}