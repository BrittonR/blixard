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
    pub vm_preemptions_total: Counter<u64>,
    
    // Storage metrics
    pub storage_writes: Counter<u64>,
    pub storage_reads: Counter<u64>,
    pub storage_write_duration: Histogram<f64>,
    pub storage_read_duration: Histogram<f64>,
    
    // P2P Image Transfer metrics
    pub p2p_image_imports_total: Counter<u64>,
    pub p2p_image_imports_failed: Counter<u64>,
    pub p2p_image_downloads_total: Counter<u64>,
    pub p2p_image_downloads_failed: Counter<u64>,
    pub p2p_bytes_transferred: Counter<u64>,
    pub p2p_chunks_transferred: Counter<u64>,
    pub p2p_chunks_deduplicated: Counter<u64>,
    pub p2p_transfer_duration: Histogram<f64>,
    pub p2p_verification_success: Counter<u64>,
    pub p2p_verification_failed: Counter<u64>,
    pub p2p_cache_hits: Counter<u64>,
    pub p2p_cache_misses: Counter<u64>,
    pub p2p_active_transfers: UpDownCounter<i64>,
    
    // Connection pool metrics
    pub connection_pool_total: UpDownCounter<i64>,
    pub connection_pool_active: UpDownCounter<i64>,
    pub connection_pool_idle: UpDownCounter<i64>,
    pub connection_pool_created: Counter<u64>,
    pub connection_pool_evicted: Counter<u64>,
    pub connection_pool_reused: Counter<u64>,
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
            vm_preemptions_total: meter
                .u64_counter("vm.preemptions.total")
                .with_description("Total number of VM preemptions")
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
            
            // P2P Image Transfer metrics
            p2p_image_imports_total: meter
                .u64_counter("p2p.image.imports.total")
                .with_description("Total number of P2P image imports")
                .init(),
            p2p_image_imports_failed: meter
                .u64_counter("p2p.image.imports.failed")
                .with_description("Number of failed P2P image imports")
                .init(),
            p2p_image_downloads_total: meter
                .u64_counter("p2p.image.downloads.total")
                .with_description("Total number of P2P image downloads")
                .init(),
            p2p_image_downloads_failed: meter
                .u64_counter("p2p.image.downloads.failed")
                .with_description("Number of failed P2P image downloads")
                .init(),
            p2p_bytes_transferred: meter
                .u64_counter("p2p.bytes_transferred.total")
                .with_description("Total bytes transferred via P2P")
                .init(),
            p2p_chunks_transferred: meter
                .u64_counter("p2p.chunks_transferred.total")
                .with_description("Total chunks transferred via P2P")
                .init(),
            p2p_chunks_deduplicated: meter
                .u64_counter("p2p.chunks_deduplicated.total")
                .with_description("Number of chunks deduplicated during P2P transfers")
                .init(),
            p2p_transfer_duration: meter
                .f64_histogram("p2p.transfer.duration")
                .with_description("Duration of P2P transfers in seconds")
                .init(),
            p2p_verification_success: meter
                .u64_counter("p2p.verification.success")
                .with_description("Number of successful P2P image verifications")
                .init(),
            p2p_verification_failed: meter
                .u64_counter("p2p.verification.failed")
                .with_description("Number of failed P2P image verifications")
                .init(),
            p2p_cache_hits: meter
                .u64_counter("p2p.cache.hits")
                .with_description("Number of P2P cache hits")
                .init(),
            p2p_cache_misses: meter
                .u64_counter("p2p.cache.misses")
                .with_description("Number of P2P cache misses")
                .init(),
            p2p_active_transfers: meter
                .i64_up_down_counter("p2p.transfers.active")
                .with_description("Number of active P2P transfers")
                .init(),
            
            // Connection pool metrics
            connection_pool_total: meter
                .i64_up_down_counter("connection_pool.connections.total")
                .with_description("Total number of connections in the pool")
                .init(),
            connection_pool_active: meter
                .i64_up_down_counter("connection_pool.connections.active")
                .with_description("Number of active connections in the pool")
                .init(),
            connection_pool_idle: meter
                .i64_up_down_counter("connection_pool.connections.idle")
                .with_description("Number of idle connections in the pool")
                .init(),
            connection_pool_created: meter
                .u64_counter("connection_pool.connections.created")
                .with_description("Total connections created by the pool")
                .init(),
            connection_pool_evicted: meter
                .u64_counter("connection_pool.connections.evicted")
                .with_description("Total connections evicted from the pool")
                .init(),
            connection_pool_reused: meter
                .u64_counter("connection_pool.connections.reused")
                .with_description("Total connection reuses from the pool")
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
    
    Ok(METRICS.get().expect("Metrics was just initialized"))
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
        
        if let Err(e) = encoder.encode(&metric_families, &mut buffer) {
            return format!("# Error encoding metrics: {}\n", e);
        }
        
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

/// Record VM preemption event
pub fn record_vm_preemption(vm_name: &str, node_id: u64, priority: u32, preemption_type: &str) {
    let metrics = metrics();
    let attrs = &[
        KeyValue::new("node_id", node_id.to_string()),
        KeyValue::new("priority", priority.to_string()),
        KeyValue::new("type", preemption_type.to_string()),
    ];
    
    metrics.vm_preemptions_total.add(1, attrs);
}

/// Record P2P image import operation
pub fn record_p2p_image_import(artifact_type: &str, success: bool, size_bytes: u64) {
    let metrics = metrics();
    let type_attr = KeyValue::new("artifact_type", artifact_type.to_string());
    
    metrics.p2p_image_imports_total.add(1, &[type_attr.clone()]);
    if !success {
        metrics.p2p_image_imports_failed.add(1, &[type_attr]);
    } else if size_bytes > 0 {
        metrics.p2p_bytes_transferred.add(size_bytes, &[type_attr]);
    }
}

/// Record P2P image download operation
pub fn record_p2p_image_download(image_id: &str, success: bool, duration_secs: f64) {
    let metrics = metrics();
    
    metrics.p2p_image_downloads_total.add(1, &[]);
    if !success {
        metrics.p2p_image_downloads_failed.add(1, &[]);
    }
    metrics.p2p_transfer_duration.record(duration_secs, &[]);
}

/// Record P2P chunk transfer
pub fn record_p2p_chunk_transfer(chunk_size: u64, was_deduplicated: bool) {
    let metrics = metrics();
    
    if was_deduplicated {
        metrics.p2p_chunks_deduplicated.add(1, &[]);
    } else {
        metrics.p2p_chunks_transferred.add(1, &[]);
        metrics.p2p_bytes_transferred.add(chunk_size, &[]);
    }
}

/// Record P2P image verification
pub fn record_p2p_verification(success: bool, verification_type: &str) {
    let metrics = metrics();
    let type_attr = KeyValue::new("verification_type", verification_type.to_string());
    
    if success {
        metrics.p2p_verification_success.add(1, &[type_attr]);
    } else {
        metrics.p2p_verification_failed.add(1, &[type_attr]);
    }
}

/// Record P2P cache access
pub fn record_p2p_cache_access(hit: bool, cache_type: &str) {
    let metrics = metrics();
    let type_attr = KeyValue::new("cache_type", cache_type.to_string());
    
    if hit {
        metrics.p2p_cache_hits.add(1, &[type_attr]);
    } else {
        metrics.p2p_cache_misses.add(1, &[type_attr]);
    }
}

/// Start tracking a P2P transfer
pub fn start_p2p_transfer() -> P2pTransferGuard {
    let metrics = metrics();
    metrics.p2p_active_transfers.add(1, &[]);
    P2pTransferGuard {
        start: std::time::Instant::now(),
    }
}

/// Guard for tracking P2P transfer duration
pub struct P2pTransferGuard {
    start: std::time::Instant,
}

impl Drop for P2pTransferGuard {
    fn drop(&mut self) {
        let metrics = metrics();
        metrics.p2p_active_transfers.add(-1, &[]);
        let duration = self.start.elapsed().as_secs_f64();
        metrics.p2p_transfer_duration.record(duration, &[]);
    }
}

/// Initialize metrics without exporter (for testing)
pub fn init_noop() -> Result<&'static Metrics, Box<dyn std::error::Error>> {
    let meter = global::meter("blixard");
    let metrics = Metrics::new(meter);
    
    METRICS.set(metrics).map_err(|_| "Metrics already initialized")?;
    Ok(METRICS.get().expect("Metrics was just initialized"))
}

/// Get the global metrics instance
pub fn metrics() -> &'static Metrics {
    METRICS.get().expect("Metrics not initialized. Call init_prometheus() or init_noop() first.")
}

/// Try to get the global metrics instance, returning None if not initialized
pub fn try_metrics() -> Option<&'static Metrics> {
    METRICS.get()
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
    use opentelemetry::{KeyValue, Key};
    
    pub const TRANSPORT_TYPE: Key = Key::from_static_str("transport.type");
    pub const MESSAGE_TYPE: Key = Key::from_static_str("message.type");
    
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
    
    pub fn artifact_type(value: &str) -> KeyValue {
        KeyValue::new("artifact.type", value.to_string())
    }
    
    pub fn image_id(value: &str) -> KeyValue {
        KeyValue::new("image.id", value.to_string())
    }
    
    pub fn chunk_hash(value: &str) -> KeyValue {
        KeyValue::new("chunk.hash", value.to_string())
    }
    
    pub fn transfer_direction(value: &str) -> KeyValue {
        KeyValue::new("transfer.direction", value.to_string())
    }
}

/// P2P transfer statistics for monitoring dashboards
#[derive(Debug, Clone)]
pub struct P2pTransferStats {
    pub total_imports: u64,
    pub failed_imports: u64,
    pub total_downloads: u64,
    pub failed_downloads: u64,
    pub bytes_transferred: u64,
    pub chunks_transferred: u64,
    pub chunks_deduplicated: u64,
    pub verification_success: u64,
    pub verification_failed: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub active_transfers: i64,
}

/// Get current P2P transfer statistics
pub fn get_p2p_transfer_stats() -> P2pTransferStats {
    // This would need to be implemented with actual metric reading
    // For now, return a placeholder
    P2pTransferStats {
        total_imports: 0,
        failed_imports: 0,
        total_downloads: 0,
        failed_downloads: 0,
        bytes_transferred: 0,
        chunks_transferred: 0,
        chunks_deduplicated: 0,
        verification_success: 0,
        verification_failed: 0,
        cache_hits: 0,
        cache_misses: 0,
        active_transfers: 0,
    }
}

/// Record connection pool statistics
pub fn record_connection_pool_stats(total: usize, active: usize, idle: usize) {
    let metrics = metrics();
    
    // Update gauges with absolute values
    metrics.connection_pool_total.add(total as i64, &[]);
    metrics.connection_pool_active.add(active as i64, &[]);
    metrics.connection_pool_idle.add(idle as i64, &[]);
}

/// Record connection pool event
pub fn record_connection_pool_event(event: ConnectionPoolEvent) {
    let metrics = metrics();
    
    match event {
        ConnectionPoolEvent::Created => {
            metrics.connection_pool_created.add(1, &[]);
        }
        ConnectionPoolEvent::Reused => {
            metrics.connection_pool_reused.add(1, &[]);
        }
        ConnectionPoolEvent::Evicted => {
            metrics.connection_pool_evicted.add(1, &[]);
        }
    }
}

/// Connection pool events
#[derive(Debug, Clone, Copy)]
pub enum ConnectionPoolEvent {
    Created,
    Reused,
    Evicted,
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
    
    #[test]
    fn test_p2p_metrics() {
        let _ = init_noop();
        
        // Test P2P import metrics
        record_p2p_image_import("microvm", true, 1024 * 1024);
        record_p2p_image_import("container", false, 0);
        
        // Test P2P download metrics
        record_p2p_image_download("test-image-123", true, 2.5);
        record_p2p_image_download("test-image-456", false, 0.1);
        
        // Test chunk transfer metrics
        record_p2p_chunk_transfer(4096, false);
        record_p2p_chunk_transfer(4096, true);
        
        // Test verification metrics
        record_p2p_verification(true, "nar_hash");
        record_p2p_verification(false, "chunk_hash");
        
        // Test cache metrics
        record_p2p_cache_access(true, "chunk");
        record_p2p_cache_access(false, "image");
    }
}

/// Record VM recovery attempt
pub fn record_vm_recovery_attempt(vm_name: &str, recovery_type: &str) {
    let metrics = metrics();
    let attrs = &[
        KeyValue::new("vm_name", vm_name.to_string()),
        KeyValue::new("type", recovery_type.to_string()),
    ];
    
    // Use existing VM operation metrics as proxy
    metrics.vm_create_total.add(1, attrs);
}

/// Record remediation action
pub fn record_remediation_action(issue_type: &str, action: &str) {
    let metrics = metrics();
    let attrs = &[
        KeyValue::new("issue_type", issue_type.to_string()),
        KeyValue::new("action", action.to_string()),
    ];
    
    // Use a general counter for remediation actions
    // In a real implementation, we'd add a specific metric for this
    metrics.grpc_requests_total.add(1, attrs);
}