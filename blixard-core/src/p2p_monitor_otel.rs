use crate::p2p_monitor::{
    ConnectionQuality, ConnectionState, Direction, DiscoveryMethod, P2pErrorType, P2pMonitor,
};
use async_trait::async_trait;
use crate::metrics_otel::Metrics;
use opentelemetry::metrics::{Counter, Histogram, UpDownCounter};
use opentelemetry::KeyValue;
use std::sync::Arc;

/// OpenTelemetry-based implementation of P2pMonitor
pub struct OtelP2pMonitor {
    metrics: Arc<Metrics>,
    // Additional P2P-specific metrics not in core Metrics struct
    connection_state_transitions: Counter<u64>,
    connection_attempts: Counter<u64>,
    connection_duration: Histogram<f64>,
    connections_active: UpDownCounter<i64>,
    connection_errors: Counter<u64>,
    p2p_rtt: Histogram<f64>,
    p2p_message_latency: Histogram<f64>,
    p2p_messages_sent: Counter<u64>,
    p2p_messages_received: Counter<u64>,
    p2p_bandwidth: Histogram<f64>,
    discovered_peers: Counter<u64>,
    discovery_duration: Histogram<f64>,
    discovery_failures: Counter<u64>,
    buffered_messages: UpDownCounter<i64>,
    buffered_messages_bytes: UpDownCounter<i64>,
    message_buffer_overflows: Counter<u64>,
}

impl OtelP2pMonitor {
    /// Create a new OpenTelemetry P2P monitor
    pub fn new(metrics: Arc<Metrics>) -> Self {
        // Get meter from existing metrics infrastructure
        let meter = opentelemetry::global::meter("blixard_p2p");
        
        // Create P2P-specific metrics
        let connection_state_transitions = meter
            .u64_counter("p2p_connection_state_transitions_total")
            .with_description("Total number of P2P connection state transitions")
            .init();
            
        let connection_attempts = meter
            .u64_counter("p2p_connection_attempts_total")
            .with_description("Total number of P2P connection attempts")
            .init();
            
        let connection_duration = meter
            .f64_histogram("p2p_connection_duration_seconds")
            .with_description("Duration of P2P connections in seconds")
            .init();
            
        let connections_active = meter
            .i64_up_down_counter("p2p_connections_active")
            .with_description("Number of active P2P connections by state")
            .init();
            
        let connection_errors = meter
            .u64_counter("p2p_connection_errors_total")
            .with_description("Total number of P2P connection errors")
            .init();
            
        let p2p_rtt = meter
            .f64_histogram("p2p_rtt_milliseconds")
            .with_description("Round-trip time to peers in milliseconds")
            .init();
            
        let p2p_message_latency = meter
            .f64_histogram("p2p_message_latency_milliseconds")
            .with_description("Message delivery latency in milliseconds")
            .init();
            
        let p2p_messages_sent = meter
            .u64_counter("p2p_messages_sent_total")
            .with_description("Total number of P2P messages sent")
            .init();
            
        let p2p_messages_received = meter
            .u64_counter("p2p_messages_received_total")
            .with_description("Total number of P2P messages received")
            .init();
            
        let p2p_bandwidth = meter
            .f64_histogram("p2p_bandwidth_bytes_per_second")
            .with_description("P2P bandwidth usage in bytes per second")
            .init();
            
        let discovered_peers = meter
            .u64_counter("p2p_discovered_peers_total")
            .with_description("Total number of peers discovered")
            .init();
            
        let discovery_duration = meter
            .f64_histogram("p2p_discovery_duration_milliseconds")
            .with_description("Time taken to discover peers in milliseconds")
            .init();
            
        let discovery_failures = meter
            .u64_counter("p2p_discovery_failures_total")
            .with_description("Total number of peer discovery failures")
            .init();
            
        let buffered_messages = meter
            .i64_up_down_counter("p2p_buffered_messages_total")
            .with_description("Number of messages currently buffered per peer")
            .init();
            
        let buffered_messages_bytes = meter
            .i64_up_down_counter("p2p_buffered_messages_bytes")
            .with_description("Total bytes of messages currently buffered per peer")
            .init();
            
        let message_buffer_overflows = meter
            .u64_counter("p2p_message_buffer_overflows_total")
            .with_description("Total number of message buffer overflows")
            .init();
        
        Self {
            metrics,
            connection_state_transitions,
            connection_attempts,
            connection_duration,
            connections_active,
            connection_errors,
            p2p_rtt,
            p2p_message_latency,
            p2p_messages_sent,
            p2p_messages_received,
            p2p_bandwidth,
            discovered_peers,
            discovery_duration,
            discovery_failures,
            buffered_messages,
            buffered_messages_bytes,
            message_buffer_overflows,
        }
    }
}

#[async_trait]
impl P2pMonitor for OtelP2pMonitor {
    async fn record_connection_attempt(&self, peer_id: &str, success: bool) {
        let attributes = vec![
            KeyValue::new("peer_id", peer_id.to_string()),
            KeyValue::new("result", if success { "success" } else { "failed" }),
        ];
        
        self.connection_attempts.add(1, &attributes);
        
        if !success {
            self.metrics.grpc_requests_failed.add(1, &[
                KeyValue::new("method", "p2p_connect"),
                KeyValue::new("peer_id", peer_id.to_string()),
            ]);
        }
    }
    
    async fn record_connection_state_change(&self, peer_id: &str, from: ConnectionState, to: ConnectionState) {
        let attributes = vec![
            KeyValue::new("peer_id", peer_id.to_string()),
            KeyValue::new("from_state", from.as_str()),
            KeyValue::new("to_state", to.as_str()),
        ];
        
        self.connection_state_transitions.add(1, &attributes);
        
        // Update active connections gauge
        match (from, to) {
            (_, ConnectionState::Connected) => {
                self.connections_active.add(1, &[
                    KeyValue::new("state", "connected"),
                ]);
                self.metrics.peer_connections_active.add(1, &[]);
            }
            (ConnectionState::Connected, _) => {
                self.connections_active.add(-1, &[
                    KeyValue::new("state", "connected"),
                ]);
                self.metrics.peer_connections_active.add(-1, &[]);
            }
            _ => {}
        }
        
        // Track connecting state
        match (from, to) {
            (_, ConnectionState::Connecting) => {
                self.connections_active.add(1, &[
                    KeyValue::new("state", "connecting"),
                ]);
            }
            (ConnectionState::Connecting, _) => {
                self.connections_active.add(-1, &[
                    KeyValue::new("state", "connecting"),
                ]);
            }
            _ => {}
        }
    }
    
    async fn record_bytes_transferred(&self, peer_id: &str, direction: Direction, bytes: u64) {
        let attributes = vec![
            KeyValue::new("peer_id", peer_id.to_string()),
            KeyValue::new("direction", direction.as_str()),
        ];
        
        // Use existing P2P bytes counter
        self.metrics.p2p_bytes_transferred.add(bytes, &attributes);
    }
    
    async fn record_message(&self, peer_id: &str, message_type: &str, size: usize, latency_ms: Option<f64>) {
        let attributes = vec![
            KeyValue::new("peer_id", peer_id.to_string()),
            KeyValue::new("message_type", message_type.to_string()),
        ];
        
        self.p2p_messages_sent.add(1, &attributes);
        
        if let Some(latency) = latency_ms {
            self.p2p_message_latency.record(latency, &attributes);
        }
    }
    
    async fn record_rtt(&self, peer_id: &str, rtt_ms: f64) {
        let attributes = vec![
            KeyValue::new("peer_id", peer_id.to_string()),
        ];
        
        self.p2p_rtt.record(rtt_ms, &attributes);
    }
    
    async fn record_error(&self, peer_id: &str, error_type: P2pErrorType) {
        let attributes = vec![
            KeyValue::new("peer_id", peer_id.to_string()),
            KeyValue::new("error_type", error_type.as_str()),
        ];
        
        self.connection_errors.add(1, &attributes);
    }
    
    async fn record_peer_discovered(&self, peer_id: &str, method: DiscoveryMethod) {
        let attributes = vec![
            KeyValue::new("peer_id", peer_id.to_string()),
            KeyValue::new("discovery_method", method.as_str()),
        ];
        
        self.discovered_peers.add(1, &attributes);
    }
    
    async fn update_connection_quality(&self, peer_id: &str, quality: ConnectionQuality) {
        let attributes = vec![
            KeyValue::new("peer_id", peer_id.to_string()),
        ];
        
        // Record quality metrics
        self.p2p_rtt.record(quality.avg_rtt, &attributes);
        self.p2p_bandwidth.record(quality.bandwidth, &attributes);
        
        // Could add gauges for quality score if needed
    }
    
    async fn record_pool_metrics(&self, total: usize, active: usize, idle: usize) {
        // Use existing connection pool metrics
        self.metrics.connection_pool_total.add(total as i64, &[]);
        self.metrics.connection_pool_active.add(active as i64, &[]);
        self.metrics.connection_pool_idle.add(idle as i64, &[]);
    }
    
    async fn record_buffered_messages(&self, peer_id: &str, count: usize, bytes: usize) {
        let attributes = vec![
            KeyValue::new("peer_id", peer_id.to_string()),
        ];
        
        self.buffered_messages.add(count as i64, &attributes);
        self.buffered_messages_bytes.add(bytes as i64, &attributes);
    }
    
    async fn record_resource_sync(&self, peer_id: &str, resource_type: &str, bytes: u64, duration_ms: f64, success: bool) {
        let attributes = vec![
            KeyValue::new("peer_id", peer_id.to_string()),
            KeyValue::new("resource_type", resource_type.to_string()),
        ];
        
        if success {
            self.metrics.p2p_chunks_transferred.add(1, &attributes);
            self.metrics.p2p_bytes_transferred.add(bytes, &attributes);
            self.metrics.p2p_transfer_duration.record(duration_ms / 1000.0, &attributes);
        } else {
            self.metrics.p2p_image_downloads_failed.add(1, &attributes);
        }
    }
}