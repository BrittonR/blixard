use async_trait::async_trait;

/// Represents the connection state of a P2P peer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectionState {
    /// Not connected to the peer
    Disconnected,
    /// Currently establishing connection
    Connecting,
    /// Successfully connected
    Connected,
    /// In the process of disconnecting
    Disconnecting,
    /// Connection attempt failed
    Failed,
}

impl ConnectionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConnectionState::Disconnected => "disconnected",
            ConnectionState::Connecting => "connecting",
            ConnectionState::Connected => "connected",
            ConnectionState::Disconnecting => "disconnecting",
            ConnectionState::Failed => "failed",
        }
    }
}

/// Direction of data transfer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Inbound,
    Outbound,
}

impl Direction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Direction::Inbound => "inbound",
            Direction::Outbound => "outbound",
        }
    }
}

/// Types of P2P errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum P2pErrorType {
    Timeout,
    ConnectionRefused,
    ConnectionReset,
    TlsError,
    DnsResolution,
    ProtocolError,
    Unknown,
}

impl P2pErrorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            P2pErrorType::Timeout => "timeout",
            P2pErrorType::ConnectionRefused => "refused",
            P2pErrorType::ConnectionReset => "reset",
            P2pErrorType::TlsError => "tls",
            P2pErrorType::DnsResolution => "dns",
            P2pErrorType::ProtocolError => "protocol",
            P2pErrorType::Unknown => "unknown",
        }
    }
}

/// Discovery method used to find a peer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiscoveryMethod {
    Dns,
    Mdns,
    Static,
    Relay,
    Manual,
}

impl DiscoveryMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            DiscoveryMethod::Dns => "dns",
            DiscoveryMethod::Mdns => "mdns",
            DiscoveryMethod::Static => "static",
            DiscoveryMethod::Relay => "relay",
            DiscoveryMethod::Manual => "manual",
        }
    }
}

/// Connection quality metrics for a peer
#[derive(Debug, Clone)]
pub struct ConnectionQuality {
    /// Recent success rate (0.0 to 1.0)
    pub success_rate: f64,
    /// Average round-trip time in milliseconds
    pub avg_rtt: f64,
    /// QUIC packet loss rate (0.0 to 1.0)
    pub packet_loss: f64,
    /// Estimated bandwidth in bytes per second
    pub bandwidth: f64,
}

impl ConnectionQuality {
    /// Calculate a quality score from 0.0 to 1.0
    pub fn score(&self) -> f64 {
        // Weighted scoring:
        // - 40% success rate
        // - 30% RTT (normalized, lower is better)
        // - 20% packet loss (inverted, lower is better)
        // - 10% bandwidth (normalized)

        let rtt_score = 1.0 - (self.avg_rtt / 1000.0).min(1.0); // Normalize to 1s max
        let packet_loss_score = 1.0 - self.packet_loss;
        let bandwidth_score = (self.bandwidth / 1_000_000.0).min(1.0); // Normalize to 1MB/s

        0.4 * self.success_rate + 0.3 * rtt_score + 0.2 * packet_loss_score + 0.1 * bandwidth_score
    }
}

/// Trait for monitoring P2P connections and metrics
#[async_trait]
pub trait P2pMonitor: Send + Sync {
    /// Record a connection attempt to a peer
    async fn record_connection_attempt(&self, peer_id: &str, success: bool);

    /// Record a connection state change
    async fn record_connection_state_change(
        &self,
        peer_id: &str,
        from: ConnectionState,
        to: ConnectionState,
    );

    /// Record bytes transferred to/from a peer
    async fn record_bytes_transferred(&self, peer_id: &str, direction: Direction, bytes: u64);

    /// Record a message sent/received
    async fn record_message(
        &self,
        peer_id: &str,
        message_type: &str,
        size: usize,
        latency_ms: Option<f64>,
    );

    /// Record round-trip time measurement
    async fn record_rtt(&self, peer_id: &str, rtt_ms: f64);

    /// Record a P2P error
    async fn record_error(&self, peer_id: &str, error_type: P2pErrorType);

    /// Record peer discovery
    async fn record_peer_discovered(&self, peer_id: &str, method: DiscoveryMethod);

    /// Update connection quality metrics
    async fn update_connection_quality(&self, peer_id: &str, quality: ConnectionQuality);

    /// Record connection pool metrics
    async fn record_pool_metrics(&self, total: usize, active: usize, idle: usize);

    /// Record buffered messages
    async fn record_buffered_messages(&self, peer_id: &str, count: usize, bytes: usize);

    /// Record resource synchronization metrics
    async fn record_resource_sync(
        &self,
        peer_id: &str,
        resource_type: &str,
        bytes: u64,
        duration_ms: f64,
        success: bool,
    );
}

/// No-op implementation for testing
pub struct NoOpMonitor;

#[async_trait]
impl P2pMonitor for NoOpMonitor {
    async fn record_connection_attempt(&self, _peer_id: &str, _success: bool) {}
    async fn record_connection_state_change(
        &self,
        _peer_id: &str,
        _from: ConnectionState,
        _to: ConnectionState,
    ) {
    }
    async fn record_bytes_transferred(&self, _peer_id: &str, _direction: Direction, _bytes: u64) {}
    async fn record_message(
        &self,
        _peer_id: &str,
        _message_type: &str,
        _size: usize,
        _latency_ms: Option<f64>,
    ) {
    }
    async fn record_rtt(&self, _peer_id: &str, _rtt_ms: f64) {}
    async fn record_error(&self, _peer_id: &str, _error_type: P2pErrorType) {}
    async fn record_peer_discovered(&self, _peer_id: &str, _method: DiscoveryMethod) {}
    async fn update_connection_quality(&self, _peer_id: &str, _quality: ConnectionQuality) {}
    async fn record_pool_metrics(&self, _total: usize, _active: usize, _idle: usize) {}
    async fn record_buffered_messages(&self, _peer_id: &str, _count: usize, _bytes: usize) {}
    async fn record_resource_sync(
        &self,
        _peer_id: &str,
        _resource_type: &str,
        _bytes: u64,
        _duration_ms: f64,
        _success: bool,
    ) {
    }
}
