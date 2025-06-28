//! Transport metrics for monitoring and debugging

use prometheus::{
    HistogramOpts, HistogramVec, IntCounterVec, IntGaugeVec,
    Histogram, IntCounter, register_histogram_vec, register_int_counter_vec, register_int_gauge_vec,
};
use std::time::Duration;

/// Transport metrics collector
pub struct TransportMetrics {
    /// Latency histograms per transport type
    pub rpc_latency: HistogramVec,
    
    /// Connection metrics
    pub active_connections: IntGaugeVec,
    pub connection_errors: IntCounterVec,
    pub connection_attempts: IntCounterVec,
    
    /// Raft-specific metrics
    pub raft_message_latency: HistogramVec,
    pub raft_message_size: HistogramVec,
    pub election_timeouts: IntCounter,
    
    /// Transport selection metrics
    pub transport_selections: IntCounterVec,
    pub transport_fallbacks: IntCounterVec,
}

impl TransportMetrics {
    /// Create new transport metrics
    pub fn new() -> Result<Self, prometheus::Error> {
        let rpc_latency = register_histogram_vec!(
            HistogramOpts::new(
                "blixard_transport_rpc_latency_seconds",
                "RPC latency by transport type and method"
            ),
            &["transport", "service", "method"]
        )?;
        
        let active_connections = register_int_gauge_vec!(
            "blixard_transport_active_connections",
            "Number of active connections by transport type",
            &["transport", "peer_id"]
        )?;
        
        let connection_errors = register_int_counter_vec!(
            "blixard_transport_connection_errors_total",
            "Total connection errors by transport type and error type",
            &["transport", "error_type"]
        )?;
        
        let connection_attempts = register_int_counter_vec!(
            "blixard_transport_connection_attempts_total",
            "Total connection attempts by transport type",
            &["transport", "peer_id"]
        )?;
        
        let raft_message_latency = register_histogram_vec!(
            HistogramOpts::new(
                "blixard_raft_message_latency_seconds",
                "Raft message latency by transport type and message type"
            ),
            &["transport", "message_type"]
        )?;
        
        let raft_message_size = register_histogram_vec!(
            HistogramOpts::new(
                "blixard_raft_message_size_bytes",
                "Raft message size by transport type and message type"
            ),
            &["transport", "message_type"]
        )?;
        
        let election_timeouts = register_int_counter_vec!(
            "blixard_raft_election_timeouts_total",
            "Total number of Raft election timeouts",
            &[]
        )?.with_label_values(&[]);
        
        let transport_selections = register_int_counter_vec!(
            "blixard_transport_selections_total",
            "Transport selection decisions by service and selected transport",
            &["service", "transport"]
        )?;
        
        let transport_fallbacks = register_int_counter_vec!(
            "blixard_transport_fallbacks_total",
            "Transport fallback occurrences",
            &["from_transport", "to_transport", "reason"]
        )?;
        
        Ok(Self {
            rpc_latency,
            active_connections,
            connection_errors,
            connection_attempts,
            raft_message_latency,
            raft_message_size,
            election_timeouts,
            transport_selections,
            transport_fallbacks,
        })
    }
    
    /// Record an RPC call
    pub fn record_rpc(&self, transport: &str, service: &str, method: &str, duration: Duration) {
        self.rpc_latency
            .with_label_values(&[transport, service, method])
            .observe(duration.as_secs_f64());
        
        // Special handling for Raft messages
        if service == "raft" {
            self.raft_message_latency
                .with_label_values(&[transport, method])
                .observe(duration.as_secs_f64());
        }
    }
    
    /// Record a connection attempt
    pub fn record_connection_attempt(&self, transport: &str, peer_id: &str) {
        self.connection_attempts
            .with_label_values(&[transport, peer_id])
            .inc();
    }
    
    /// Record a connection error
    pub fn record_connection_error(&self, transport: &str, error_type: &str) {
        self.connection_errors
            .with_label_values(&[transport, error_type])
            .inc();
    }
    
    /// Update active connection count
    pub fn set_active_connections(&self, transport: &str, peer_id: &str, count: i64) {
        self.active_connections
            .with_label_values(&[transport, peer_id])
            .set(count);
    }
    
    /// Record transport selection
    pub fn record_transport_selection(&self, service: &str, transport: &str) {
        self.transport_selections
            .with_label_values(&[service, transport])
            .inc();
    }
    
    /// Record transport fallback
    pub fn record_transport_fallback(&self, from: &str, to: &str, reason: &str) {
        self.transport_fallbacks
            .with_label_values(&[from, to, reason])
            .inc();
    }
    
    /// Record Raft message size
    pub fn record_raft_message_size(&self, transport: &str, message_type: &str, size: usize) {
        self.raft_message_size
            .with_label_values(&[transport, message_type])
            .observe(size as f64);
    }
    
    /// Record election timeout
    pub fn record_election_timeout(&self) {
        self.election_timeouts.inc();
    }
}

/// Global transport metrics instance
lazy_static::lazy_static! {
    pub static ref TRANSPORT_METRICS: TransportMetrics = {
        TransportMetrics::new().expect("Failed to create transport metrics")
    };
}