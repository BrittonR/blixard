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
    pub vm_start_total: Counter<u64>,
    pub vm_start_failed: Counter<u64>,
    pub vm_stop_total: Counter<u64>,
    pub vm_stop_failed: Counter<u64>,
    pub vm_delete_total: Counter<u64>,
    pub vm_delete_failed: Counter<u64>,
    pub vm_health_checks_total: Counter<u64>,
    pub vm_health_check_failed: Counter<u64>,
    pub vm_status_changes: Counter<u64>,
    pub vm_recovery_success: Counter<u64>,
    pub vm_recovery_failed: Counter<u64>,
    
    // Resource monitoring metrics
    pub cluster_nodes_total: UpDownCounter<i64>,
    pub cluster_nodes_healthy: UpDownCounter<i64>,
    pub cluster_vcpus_total: UpDownCounter<i64>,
    pub cluster_vcpus_used: UpDownCounter<i64>,
    pub cluster_memory_mb_total: UpDownCounter<i64>,
    pub cluster_memory_mb_used: UpDownCounter<i64>,
    pub cluster_disk_gb_total: UpDownCounter<i64>,
    pub cluster_disk_gb_used: UpDownCounter<i64>,
    pub node_vcpus_available: UpDownCounter<i64>,
    pub node_memory_mb_available: UpDownCounter<i64>,
    pub node_disk_gb_available: UpDownCounter<i64>,
    pub vm_placement_attempts: Counter<u64>,
    pub vm_placement_failures: Counter<u64>,
    pub vm_placement_duration: Histogram<f64>,
    
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
            vm_start_total: meter
                .u64_counter("vm.start.total")
                .with_description("Total number of VM start attempts")
                .init(),
            vm_start_failed: meter
                .u64_counter("vm.start.failed")
                .with_description("Number of failed VM starts")
                .init(),
            vm_stop_total: meter
                .u64_counter("vm.stop.total")
                .with_description("Total number of VM stop attempts")
                .init(),
            vm_stop_failed: meter
                .u64_counter("vm.stop.failed")
                .with_description("Number of failed VM stops")
                .init(),
            vm_delete_total: meter
                .u64_counter("vm.delete.total")
                .with_description("Total number of VM delete attempts")
                .init(),
            vm_delete_failed: meter
                .u64_counter("vm.delete.failed")
                .with_description("Number of failed VM deletes")
                .init(),
            vm_health_checks_total: meter
                .u64_counter("vm.health_checks.total")
                .with_description("Total number of VM health checks performed")
                .init(),
            vm_health_check_failed: meter
                .u64_counter("vm.health_check.failed")
                .with_description("Number of failed VM health checks")
                .init(),
            vm_status_changes: meter
                .u64_counter("vm.status_changes.total")
                .with_description("Number of VM status changes detected by health monitoring")
                .init(),
            vm_recovery_success: meter
                .u64_counter("vm.recovery.success")
                .with_description("Number of successful VM recoveries")
                .init(),
            vm_recovery_failed: meter
                .u64_counter("vm.recovery.failed")
                .with_description("Number of failed VM recovery attempts")
                .init(),
            
            // Resource monitoring metrics
            cluster_nodes_total: meter
                .i64_up_down_counter("cluster.nodes.total")
                .with_description("Total number of nodes in cluster")
                .init(),
            cluster_nodes_healthy: meter
                .i64_up_down_counter("cluster.nodes.healthy")
                .with_description("Number of healthy nodes in cluster")
                .init(),
            cluster_vcpus_total: meter
                .i64_up_down_counter("cluster.vcpus.total")
                .with_description("Total vCPUs available in cluster")
                .init(),
            cluster_vcpus_used: meter
                .i64_up_down_counter("cluster.vcpus.used")
                .with_description("vCPUs currently used in cluster")
                .init(),
            cluster_memory_mb_total: meter
                .i64_up_down_counter("cluster.memory_mb.total")
                .with_description("Total memory in MB available in cluster")
                .init(),
            cluster_memory_mb_used: meter
                .i64_up_down_counter("cluster.memory_mb.used")
                .with_description("Memory in MB currently used in cluster")
                .init(),
            cluster_disk_gb_total: meter
                .i64_up_down_counter("cluster.disk_gb.total")
                .with_description("Total disk space in GB available in cluster")
                .init(),
            cluster_disk_gb_used: meter
                .i64_up_down_counter("cluster.disk_gb.used")
                .with_description("Disk space in GB currently used in cluster")
                .init(),
            node_vcpus_available: meter
                .i64_up_down_counter("node.vcpus.available")
                .with_description("vCPUs available on this node")
                .init(),
            node_memory_mb_available: meter
                .i64_up_down_counter("node.memory_mb.available")
                .with_description("Memory in MB available on this node")
                .init(),
            node_disk_gb_available: meter
                .i64_up_down_counter("node.disk_gb.available")
                .with_description("Disk space in GB available on this node")
                .init(),
            vm_placement_attempts: meter
                .u64_counter("vm.placement.attempts")
                .with_description("Total number of VM placement attempts")
                .init(),
            vm_placement_failures: meter
                .u64_counter("vm.placement.failures")
                .with_description("Number of failed VM placements")
                .init(),
            vm_placement_duration: meter
                .f64_histogram("vm.placement.duration")
                .with_description("Duration of VM placement decisions in seconds")
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

/// Update cluster resource metrics from a cluster resource summary
pub fn update_cluster_resource_metrics(summary: &crate::vm_scheduler::ClusterResourceSummary) {
    let metrics = metrics();
    
    // Update cluster-wide resource metrics
    metrics.cluster_nodes_total.add(summary.total_nodes as i64, &[]);
    metrics.cluster_vcpus_total.add(summary.total_vcpus as i64, &[]);
    metrics.cluster_vcpus_used.add(summary.used_vcpus as i64, &[]);
    metrics.cluster_memory_mb_total.add(summary.total_memory_mb as i64, &[]);
    metrics.cluster_memory_mb_used.add(summary.used_memory_mb as i64, &[]);
    metrics.cluster_disk_gb_total.add(summary.total_disk_gb as i64, &[]);
    metrics.cluster_disk_gb_used.add(summary.used_disk_gb as i64, &[]);
}

/// Update node-specific resource metrics
pub fn update_node_resource_metrics(node_id: u64, usage: &crate::vm_scheduler::NodeResourceUsage) {
    let metrics = metrics();
    let node_id_attr = attributes::node_id(node_id);
    
    // Update per-node resource availability
    metrics.node_vcpus_available.add(usage.available_vcpus() as i64, &[node_id_attr.clone()]);
    metrics.node_memory_mb_available.add(usage.available_memory_mb() as i64, &[node_id_attr.clone()]);
    metrics.node_disk_gb_available.add(usage.available_disk_gb() as i64, &[node_id_attr]);
}

/// Record VM placement attempt
pub fn record_vm_placement_attempt(strategy: &str, success: bool, duration_secs: f64) {
    let metrics = metrics();
    let strategy_attr = KeyValue::new("strategy", strategy.to_string());
    
    metrics.vm_placement_attempts.add(1, &[strategy_attr.clone()]);
    if !success {
        metrics.vm_placement_failures.add(1, &[strategy_attr.clone()]);
    }
    metrics.vm_placement_duration.record(duration_secs, &[strategy_attr]);
}

/// Record VM lifecycle operation
pub fn record_vm_operation(operation: &str, success: bool) {
    let metrics = metrics();
    let operation_attr = KeyValue::new("operation", operation.to_string());
    
    match operation {
        "create" => {
            metrics.vm_create_total.add(1, &[operation_attr.clone()]);
            if !success {
                metrics.vm_create_failed.add(1, &[operation_attr]);
            }
        }
        "start" => {
            metrics.vm_start_total.add(1, &[operation_attr.clone()]);
            if !success {
                metrics.vm_start_failed.add(1, &[operation_attr]);
            }
        }
        "stop" => {
            metrics.vm_stop_total.add(1, &[operation_attr.clone()]);
            if !success {
                metrics.vm_stop_failed.add(1, &[operation_attr]);
            }
        }
        "delete" => {
            metrics.vm_delete_total.add(1, &[operation_attr.clone()]);
            if !success {
                metrics.vm_delete_failed.add(1, &[operation_attr]);
            }
        }
        _ => {} // Unknown operation
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
    
    pub fn recovery_type(value: &str) -> KeyValue {
        KeyValue::new("recovery.type", value.to_string())
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