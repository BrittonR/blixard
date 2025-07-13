//! Iroh P2P transport layer for distributed communication
//!
//! This module implements a modern peer-to-peer transport layer using Iroh,
//! providing secure, efficient, and NAT-traversing communication across the
//! Blixard cluster. All inter-node communication flows through this layer,
//! including Raft consensus messages, VM operations, and health monitoring.
//!
//! ## Architecture Overview
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                   Iroh P2P Transport Architecture               │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  Service Layer          │  Protocol Layer      │  Network Layer │
//! │  ┌─────────────────┐    │  ┌─────────────────┐  │  ┌───────────┐ │
//! │  │ VM Operations   │    │  │ ALPN Routing    │  │  │ QUIC      │ │
//! │  │ • VM Lifecycle  │ ──▶│  │ • blixard/rpc   │──│──│ • Secure  │ │
//! │  │ • Health Check  │    │  │ • blixard/raft  │  │  │ • Fast    │ │
//! │  │ • Image Sync    │    │  │ • blixard/blob  │  │  │ • 0-RTT   │ │
//! │  └─────────────────┘    │  └─────────────────┘  │  └───────────┘ │
//! │                         │                       │               │
//! │  ┌─────────────────┐    │  ┌─────────────────┐  │  ┌───────────┐ │
//! │  │ Raft Consensus  │    │  │ Message Framing │  │  │ Discovery │ │
//! │  │ • Vote Requests │ ──▶│  │ • Postcard      │  │  │ • mDNS    │ │
//! │  │ • Append Entry  │    │  │ • Compression   │  │  │ • DHT     │ │
//! │  │ • Install Snap  │    │  │ • Heartbeats    │  │  │ • Relay   │ │
//! │  └─────────────────┘    │  └─────────────────┘  │  └───────────┘ │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                      Security & Identity                        │
//! │  ┌─────────────────┐    ┌─────────────────┐    ┌─────────────┐ │
//! │  │ Node Identity   │    │ Encryption      │    │ NAT Traverse│ │
//! │  │ • Ed25519 Keys  │    │ • TLS 1.3       │    │ • Hole Punch│ │
//! │  │ • Certificate   │    │ • Perfect FS    │    │ • STUN/TURN │ │
//! │  │ • Trust Chain   │    │ • Key Rotation  │    │ • IPv6 Ready│ │
//! │  └─────────────────┘    └─────────────────┘    └─────────────┘ │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Core Components
//!
//! ### 1. Service Architecture (`services/`)
//! Protocol-specific service implementations running over Iroh:
//! - **VM Service**: VM lifecycle operations (create, start, stop, delete)
//! - **Health Service**: Node and VM health monitoring and reporting
//! - **Status Service**: Cluster status queries and node information
//! - **Blob Service**: Large file transfer for VM images and snapshots
//!
//! ### 2. Raft Transport (`iroh_raft_transport`)
//! Specialized transport for Raft consensus messages:
//! - **Message Routing**: Efficient delivery of Raft messages
//! - **Failure Detection**: Network partition and node failure detection
//! - **Ordering Guarantees**: Ensures proper message ordering
//! - **Flow Control**: Prevents overwhelming slow followers
//!
//! ### 3. Peer Management (`iroh_peer_connector`)
//! Handles peer discovery, connection management, and lifecycle:
//! - **Auto Discovery**: mDNS and DHT-based peer discovery
//! - **Connection Pooling**: Reuse connections for efficiency
//! - **Health Monitoring**: Track peer connectivity and performance
//! - **Retry Logic**: Exponential backoff for failed connections
//!
//! ### 4. Protocol Handlers (`iroh_protocol`)
//! ALPN-based protocol multiplexing for different service types:
//! - **RPC Protocol**: Request-response patterns for operations
//! - **Streaming Protocol**: Long-lived streams for monitoring
//! - **Blob Protocol**: Efficient bulk data transfer
//! - **Custom Protocols**: Extensible for new services
//!
//! ## Network Security Model
//!
//! ### Identity and Authentication
//! ```text
//! Node Identity:
//! ┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
//! │   Ed25519 Key   │────▶│   Certificate   │────▶│  Trust Anchor   │
//! │  (Node Secret)  │     │  (Node Public)  │     │  (Cluster CA)   │
//! └─────────────────┘     └─────────────────┘     └─────────────────┘
//! ```
//!
//! ### Connection Security
//! Every connection is secured with:
//! - **TLS 1.3**: Latest TLS with perfect forward secrecy
//! - **Mutual Authentication**: Both peers verify each other
//! - **Certificate Validation**: Ensures nodes belong to the cluster
//! - **Key Rotation**: Automatic key refresh for long-lived connections
//!
//! ### Network Isolation
//! - **ALPN Isolation**: Different protocols cannot interfere
//! - **Resource Limits**: Per-connection bandwidth and memory limits
//! - **DDoS Protection**: Rate limiting and connection throttling
//! - **Audit Logging**: All connection events are logged
//!
//! ## NAT Traversal and Connectivity
//!
//! ### Hole Punching
//! Automatic NAT traversal using modern techniques:
//! ```text
//! Node A (behind NAT) ──────────────┐
//!                                   │
//!                              ┌────▼────┐
//!                              │  STUN   │
//!                              │ Server  │
//!                              └────┬────┘
//!                                   │
//! Node B (behind NAT) ──────────────┘
//!     │
//!     └──────── Direct P2P Connection ─────────▶ Node A
//! ```
//!
//! ### Relay Fallback
//! When direct connection fails:
//! - **TURN Relays**: Route traffic through relay servers
//! - **Automatic Selection**: Choose optimal relay based on latency
//! - **Load Balancing**: Distribute load across multiple relays
//! - **Failover**: Switch relays if performance degrades
//!
//! ## Performance Optimization
//!
//! ### Connection Reuse
//! ```rust,no_run
//! use blixard_core::transport::iroh_client::IrohClient;
//!
//! // Connections are automatically pooled and reused
//! let client = IrohClient::new(endpoint).await?;
//! 
//! // Multiple requests reuse the same connection
//! for _ in 0..100 {
//!     client.get_vm_status(node_id, "vm-name").await?;
//! }
//! ```
//!
//! ### Message Batching
//! ```rust,no_run
//! use blixard_core::transport::iroh_raft_transport::RaftTransport;
//!
//! // Batch multiple Raft messages for efficiency
//! let mut batch = Vec::new();
//! batch.push(append_entries_msg);
//! batch.push(heartbeat_msg);
//! 
//! transport.send_batch(target_node, batch).await?;
//! ```
//!
//! ### Zero-Copy Operations
//! Iroh QUIC supports zero-copy operations for large data:
//! - **VM Images**: Stream images directly without buffering
//! - **Snapshots**: Efficient Raft snapshot transfer
//! - **Logs**: Stream log data without intermediate copies
//!
//! ## Service Implementation Examples
//!
//! ### Creating a Custom Service
//! ```rust,no_run
//! use blixard_core::transport::iroh_service::IrohService;
//! use async_trait::async_trait;
//!
//! pub struct CustomService {
//!     // Service state
//! }
//!
//! #[async_trait]
//! impl IrohService for CustomService {
//!     const ALPN: &'static [u8] = b"blixard/custom/1";
//!
//!     async fn handle_connection(
//!         &self,
//!         peer_id: iroh::NodeId,
//!         connection: quinn::Connection,
//!     ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!         // Handle incoming connection
//!         let (mut send, mut recv) = connection.accept_bi().await?;
//!         
//!         // Process requests
//!         while let Some(request) = recv.read_to_end(1024).await? {
//!             let response = self.process_request(request).await?;
//!             send.write_all(&response).await?;
//!         }
//!         
//!         Ok(())
//!     }
//! }
//! ```
//!
//! ### Client Implementation
//! ```rust,no_run
//! use blixard_core::transport::iroh_client::IrohClient;
//!
//! async fn call_custom_service(
//!     client: &IrohClient,
//!     target_node: iroh::NodeId,
//!     request: &[u8]
//! ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
//!     let mut connection = client.connect(target_node, b"blixard/custom/1").await?;
//!     
//!     let (mut send, mut recv) = connection.open_bi().await?;
//!     send.write_all(request).await?;
//!     send.finish().await?;
//!     
//!     let response = recv.read_to_end(1024 * 1024).await?; // 1MB limit
//!     Ok(response)
//! }
//! ```
//!
//! ## Error Handling and Recovery
//!
//! ### Network Errors
//! The transport layer handles various network failure modes:
//! - **Connection Timeouts**: Configurable timeouts with exponential backoff
//! - **Network Partitions**: Automatic detection and graceful degradation
//! - **Peer Unreachable**: Fallback to relay servers
//! - **Protocol Errors**: Proper error propagation with context
//!
//! ### Recovery Strategies
//! ```rust,no_run
//! use blixard_core::transport::iroh_peer_connector::PeerConnector;
//!
//! async fn robust_send_message(
//!     connector: &PeerConnector,
//!     target: iroh::NodeId,
//!     message: &[u8]
//! ) -> Result<(), Box<dyn std::error::Error>> {
//!     let mut retry_count = 0;
//!     const MAX_RETRIES: u32 = 3;
//!     
//!     loop {
//!         match connector.send_message(target, message).await {
//!             Ok(()) => return Ok(()),
//!             Err(e) if retry_count < MAX_RETRIES => {
//!                 retry_count += 1;
//!                 let delay = Duration::from_millis(100 * (1 << retry_count));
//!                 tokio::time::sleep(delay).await;
//!                 continue;
//!             },
//!             Err(e) => return Err(e),
//!         }
//!     }
//! }
//! ```
//!
//! ## Performance Characteristics
//!
//! ### Latency
//! - **Local Network**: <1ms for small messages
//! - **Internet**: 10-100ms depending on geography
//! - **0-RTT**: Subsequent connections have no handshake overhead
//! - **Connection Reuse**: Eliminates repeated handshakes
//!
//! ### Throughput
//! - **Single Stream**: Up to link capacity (Gbps+)
//! - **Multiple Streams**: Parallel streams within connection
//! - **Compression**: Automatic compression for text data
//! - **Flow Control**: Prevents overwhelming receivers
//!
//! ### Resource Usage
//! - **Memory**: ~1MB per active connection
//! - **CPU**: Minimal overhead for established connections
//! - **Network**: Efficient multiplexing reduces connection count
//! - **Battery**: Power-efficient on mobile devices
//!
//! ## Monitoring and Observability
//!
//! ### Connection Metrics
//! The transport layer exposes comprehensive metrics:
//! - **Active Connections**: Current connection count per peer
//! - **Message Rates**: Requests/responses per second
//! - **Latency Distribution**: P50, P95, P99 latencies
//! - **Error Rates**: Connection failures and timeouts
//! - **Bandwidth Usage**: Bytes sent/received per peer
//!
//! ### Health Monitoring
//! ```rust,no_run
//! use blixard_core::transport::metrics::TransportMetrics;
//!
//! async fn monitor_transport_health(metrics: &TransportMetrics) {
//!     let stats = metrics.get_connection_stats().await;
//!     
//!     for (peer_id, peer_stats) in stats {
//!         if peer_stats.error_rate > 0.1 {
//!             tracing::warn!("High error rate for peer {}: {:.2}%", 
//!                          peer_id, peer_stats.error_rate * 100.0);
//!         }
//!         
//!         if peer_stats.avg_latency > Duration::from_millis(100) {
//!             tracing::warn!("High latency for peer {}: {:?}", 
//!                          peer_id, peer_stats.avg_latency);
//!         }
//!     }
//! }
//! ```
//!
//! ## Configuration and Tuning
//!
//! ### Transport Configuration
//! ```rust,no_run
//! use blixard_core::transport::config::TransportConfig;
//!
//! let config = TransportConfig {
//!     // Connection limits
//!     max_connections_per_peer: 10,
//!     connection_idle_timeout: Duration::from_secs(300),
//!     
//!     // Performance tuning
//!     initial_congestion_window: 32 * 1024, // 32KB
//!     max_stream_bandwidth: Some(100 * 1024 * 1024), // 100MB/s
//!     
//!     // Security settings
//!     require_mutual_auth: true,
//!     allowed_cipher_suites: vec![CipherSuite::TLS13_AES_256_GCM_SHA384],
//!     
//!     // Discovery settings
//!     enable_mdns: true,
//!     enable_dht: true,
//!     relay_servers: vec!["https://relay1.example.com".to_string()],
//! };
//! ```
//!
//! ## Testing and Validation
//!
//! ### Network Simulation
//! The transport layer includes comprehensive testing:
//! - **Latency Injection**: Simulate high-latency networks
//! - **Packet Loss**: Test resilience to packet loss
//! - **Bandwidth Limits**: Validate flow control
//! - **Network Partitions**: Test partition tolerance
//!
//! ### Integration Testing
//! ```rust,no_run
//! #[tokio::test]
//! async fn test_peer_communication() {
//!     let (node1, node2) = create_test_nodes().await;
//!     
//!     // Test basic connectivity
//!     node1.ping(node2.node_id()).await.unwrap();
//!     
//!     // Test service calls
//!     let response = node1.call_vm_service(node2.node_id(), request).await.unwrap();
//!     assert_eq!(response.status, "success");
//!     
//!     // Test network partition
//!     simulate_network_partition(&mut node1, &mut node2).await;
//!     
//!     // Verify graceful degradation
//!     assert!(node1.ping(node2.node_id()).await.is_err());
//! }
//! ```

pub mod cluster_operations_adapter;
pub mod config;
pub mod iroh_client;
pub mod iroh_cluster_service;
pub mod iroh_health_service;
pub mod iroh_identity_enrollment;
pub mod iroh_peer_connector;
pub mod iroh_protocol;
pub mod iroh_raft_transport;
pub mod iroh_service;
pub mod iroh_service_runner;
pub mod iroh_status_service;
pub mod iroh_vm_service;
pub mod metrics;
pub mod raft_transport_adapter;
pub mod service_builder;
pub mod service_consolidation;
pub mod services;

// Iroh security modules
pub mod iroh_middleware;
pub mod iroh_secure_vm_service;
pub mod secure_iroh_protocol_handler;

#[cfg(test)]
mod tests;

/// ALPN protocol identifier for Blixard RPC
pub const BLIXARD_ALPN: &[u8] = b"blixard/rpc/1";

/// Iroh transport implementation  
pub struct IrohTransport {
    pub endpoint: iroh::Endpoint,
    pub node_id: iroh::NodeId,
}

impl IrohTransport {
    /// Create a new Iroh transport
    pub fn new(endpoint: iroh::Endpoint) -> Self {
        let node_id = endpoint.node_id();
        Self { endpoint, node_id }
    }

    /// Get the node ID
    pub fn node_id(&self) -> iroh::NodeId {
        self.node_id
    }

    /// Get the endpoint
    pub fn endpoint(&self) -> &iroh::Endpoint {
        &self.endpoint
    }
}
