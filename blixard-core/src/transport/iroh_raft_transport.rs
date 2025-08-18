//! Iroh transport adapter for Raft consensus
//!
//! This module provides an efficient transport layer for Raft messages over Iroh P2P,
//! with optimizations for:
//! - Message batching to reduce network overhead
//! - Prioritization of different Raft message types
//! - Efficient streaming for log entries and snapshots
//! - Integration with existing Raft manager

use iroh::endpoint::{Connection, RecvStream, SendStream};
use iroh::{Endpoint, NodeId};
use raft::prelude::*;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::task::JoinHandle;

use crate::error::{BlixardError, BlixardResult};
#[cfg(feature = "observability")]
use crate::metrics_otel::{attributes, safe_metrics, Timer};
use crate::node_shared::SharedNodeState;
use crate::transport::iroh_protocol::{
    generate_request_id, read_message, write_message, MessageType as ProtocolMessageType,
};

/// Metrics for Iroh Raft transport
#[derive(Debug, Clone)]
pub struct IrohRaftMetrics {
    pub active_connections: usize,
    pub messages_sent: u64,
    pub messages_received: u64,
}

#[cfg(feature = "observability")]
lazy_static::lazy_static! {
    static ref RAFT_MESSAGES_SENT: opentelemetry::metrics::Counter<u64> = {
        opentelemetry::global::meter("blixard")
            .u64_counter("raft.messages.sent")
            .with_description("Number of Raft messages sent over Iroh transport")
            .init()
    };
    static ref RAFT_MESSAGES_RECEIVED: opentelemetry::metrics::Counter<u64> = {
        opentelemetry::global::meter("blixard")
            .u64_counter("raft.messages.received")
            .with_description("Number of Raft messages received over Iroh transport")
            .init()
    };
}
use crate::config_global;

/// Raft-specific message types for optimization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RaftMessagePriority {
    /// Election messages (RequestVote, Vote) - highest priority
    Election,
    /// Heartbeat messages - high priority
    Heartbeat,
    /// Log append messages - normal priority
    LogAppend,
    /// Snapshot messages - low priority (bulk transfer)
    Snapshot,
}

impl RaftMessagePriority {
    fn from_raft_message(msg: &Message) -> Self {
        match msg.msg_type() {
            MessageType::MsgRequestVote | MessageType::MsgRequestVoteResponse => Self::Election,
            MessageType::MsgHeartbeat | MessageType::MsgHeartbeatResponse => Self::Heartbeat,
            MessageType::MsgSnapshot => Self::Snapshot,
            _ => Self::LogAppend,
        }
    }

    fn as_u8(&self) -> u8 {
        match self {
            Self::Election => 0,
            Self::Heartbeat => 1,
            Self::LogAppend => 2,
            Self::Snapshot => 3,
        }
    }
}

/// Buffered Raft message with metadata
struct BufferedRaftMessage {
    message: Message,
    priority: RaftMessagePriority,
    timestamp: Instant,
    #[allow(dead_code)] // Future: implement retry logic with attempt tracking
    attempts: u32,
}

/// Connection state for a peer
struct PeerConnection {
    node_id: u64,
    iroh_node_id: NodeId,
    connection: Connection,
    /// Separate streams for different message priorities
    election_stream: Option<SendStream>,
    heartbeat_stream: Option<SendStream>,
    append_stream: Option<SendStream>,
    snapshot_stream: Option<SendStream>,
    last_activity: Instant,
}

impl PeerConnection {
    async fn get_stream(
        &mut self,
        priority: RaftMessagePriority,
    ) -> BlixardResult<&mut SendStream> {
        match priority {
            RaftMessagePriority::Election => {
                if self.election_stream.is_none() {
                    self.election_stream = Some(self.connection.open_uni().await.map_err(|e| {
                        BlixardError::Internal {
                            message: format!("Failed to open stream: {}", e),
                        }
                    })?);
                }
                self.election_stream
                    .as_mut()
                    .ok_or_else(|| BlixardError::Internal {
                        message: "Election stream should exist after creation".to_string(),
                    })
            }
            RaftMessagePriority::Heartbeat => {
                if self.heartbeat_stream.is_none() {
                    self.heartbeat_stream =
                        Some(self.connection.open_uni().await.map_err(|e| {
                            BlixardError::Internal {
                                message: format!("Failed to open stream: {}", e),
                            }
                        })?);
                }
                self.heartbeat_stream
                    .as_mut()
                    .ok_or_else(|| BlixardError::Internal {
                        message: "Heartbeat stream should exist after creation".to_string(),
                    })
            }
            RaftMessagePriority::LogAppend => {
                if self.append_stream.is_none() {
                    self.append_stream = Some(self.connection.open_uni().await.map_err(|e| {
                        BlixardError::Internal {
                            message: format!("Failed to open stream: {}", e),
                        }
                    })?);
                }
                self.append_stream
                    .as_mut()
                    .ok_or_else(|| BlixardError::Internal {
                        message: "Append stream should exist after creation".to_string(),
                    })
            }
            RaftMessagePriority::Snapshot => {
                if self.snapshot_stream.is_none() {
                    self.snapshot_stream = Some(self.connection.open_uni().await.map_err(|e| {
                        BlixardError::Internal {
                            message: format!("Failed to open stream: {}", e),
                        }
                    })?);
                }
                self.snapshot_stream
                    .as_mut()
                    .ok_or_else(|| BlixardError::Internal {
                        message: "Snapshot stream should exist after creation".to_string(),
                    })
            }
        }
    }
}

/// Iroh transport adapter for Raft
pub struct IrohRaftTransport {
    /// Reference to shared node state
    node: Arc<SharedNodeState>,
    /// Iroh endpoint for P2P communication
    endpoint: Endpoint,
    /// Our Iroh node ID
    #[allow(dead_code)] // Future: implement node ID-based message routing
    local_node_id: NodeId,
    /// Active connections to peers
    connections: Arc<RwLock<HashMap<u64, PeerConnection>>>,
    /// Message buffer for batching and retry
    message_buffer: Arc<Mutex<HashMap<u64, VecDeque<BufferedRaftMessage>>>>,
    /// Channel to send incoming Raft messages to the Raft manager
    #[allow(dead_code)] // Future: implement direct Raft message forwarding
    raft_rx_tx: mpsc::UnboundedSender<(u64, Message)>,
    /// Background task handles
    tasks: Mutex<Vec<JoinHandle<()>>>,
    /// Shutdown channel
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
    /// Metrics
    messages_sent: Arc<Mutex<HashMap<String, u64>>>,
    #[allow(dead_code)] // Future: implement received message metrics
    messages_received: Arc<Mutex<HashMap<String, u64>>>,
}

impl IrohRaftTransport {
    /// Create a new Iroh Raft transport
    pub fn new(
        node: Arc<SharedNodeState>,
        endpoint: Endpoint,
        local_node_id: NodeId,
        raft_rx_tx: mpsc::UnboundedSender<(u64, Message)>,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        Self {
            node,
            endpoint,
            local_node_id,
            connections: Arc::new(RwLock::new(HashMap::new())),
            message_buffer: Arc::new(Mutex::new(HashMap::new())),
            raft_rx_tx,
            tasks: Mutex::new(Vec::new()),
            shutdown_tx,
            shutdown_rx,
            messages_sent: Arc::new(Mutex::new(HashMap::new())),
            messages_received: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start the transport (accept incoming connections, process outgoing messages)
    pub async fn start(&self) -> BlixardResult<()> {
        // IMPORTANT: We do NOT spawn an accept task here because the Raft transport
        // should not accept incoming connections directly. The iroh_service_runner
        // handles all incoming connections through the Router, which properly filters
        // by ALPN protocol and dispatches to the appropriate service.
        //
        // Raft messages are sent directly between peers using the send_message() method,
        // which opens outgoing connections as needed. This prevents the Raft transport
        // from intercepting RPC connections meant for the cluster service.

        // Start message batch processor for outgoing messages
        let batch_task = self.spawn_batch_processor();

        // Start connection health checker
        let health_task = self.spawn_health_checker();

        // Store task handles
        let mut tasks = self.tasks.lock().await;
        tasks.push(batch_task);
        tasks.push(health_task);

        tracing::info!("Iroh Raft transport started (outgoing connections only)");
        Ok(())
    }

    /// Send a Raft message to a peer
    pub async fn send_message(&self, to: u64, message: Message) -> BlixardResult<()> {
        tracing::info!(
            "RAFT_SEND: Attempting to send {:?} message to node {}", 
            message.msg_type(), to
        );
        
        // Check if we know about this peer first
        let peer_info = self.node.get_peer(to).await;
        if peer_info.is_none() {
            // Peer is not in our cluster, silently drop the message
            tracing::warn!(
                "RAFT_SEND: Dropping message to unknown peer {} - not in peer list", 
                to
            );
            return Ok(());
        }
        
        tracing::info!(
            "RAFT_SEND: Found peer {} in peer list with P2P info: node_id={:?}, addresses={:?}",
            to,
            peer_info.as_ref().unwrap().p2p_node_id,
            peer_info.as_ref().unwrap().p2p_addresses
        );

        let priority = RaftMessagePriority::from_raft_message(&message);

        // For high-priority messages AND MsgAppend (critical for config changes), try to send immediately
        if matches!(
            priority,
            RaftMessagePriority::Election | RaftMessagePriority::Heartbeat | RaftMessagePriority::LogAppend
        ) {
            tracing::debug!(
                "RAFT_SEND: High-priority message {:?} to {}, attempting immediate send",
                message.msg_type(), to
            );
            tracing::info!("RAFT_SEND: Attempting immediate send to node {}", to);
            if let Err(e) = self.try_send_immediate(to, &message, priority).await {
                tracing::warn!(
                    "RAFT_SEND: Immediate send failed for node {}: {}, buffering", 
                    to, e
                );
                self.buffer_message(to, message, priority).await?;
            } else {
                tracing::info!(
                    "RAFT_SEND: Successfully sent {:?} message immediately to node {}",
                    message.msg_type(), to
                );
            }
        } else {
            // For lower priority messages, always buffer for batching
            tracing::debug!(
                "RAFT_SEND: Buffering {:?} message to node {} for batch sending",
                message.msg_type(), to
            );
            self.buffer_message(to, message, priority).await?;
        }

        Ok(())
    }

    /// Try to send a message immediately
    async fn try_send_immediate(
        &self,
        to: u64,
        message: &Message,
        priority: RaftMessagePriority,
    ) -> BlixardResult<()> {
        tracing::info!(
            "RAFT_SEND_IMMEDIATE: Starting immediate send of {:?} to node {}",
            message.msg_type(), to
        );
        
        #[cfg(feature = "observability")]
        let _timer = safe_metrics().ok().map(|m| Timer::new(m.raft_proposal_duration.clone()));

        // Get or create connection
        tracing::info!("RAFT_SEND_IMMEDIATE: Getting or creating connection to node {}", to);
        let mut conn = match self.get_or_create_connection(to).await {
            Ok(conn) => {
                tracing::info!(
                    "RAFT_SEND_IMMEDIATE: Successfully got connection to node {} (Iroh ID: {})",
                    to, conn.iroh_node_id
                );
                conn
            }
            Err(e) => {
                tracing::error!(
                    "RAFT_SEND_IMMEDIATE: Failed to get/create connection to node {}: {}",
                    to, e
                );
                return Err(e);
            }
        };

        // Serialize Raft message
        tracing::debug!(
            "RAFT_SEND_IMMEDIATE: Serializing {:?} message for node {}",
            message.msg_type(), to
        );
        let msg_data = crate::raft_codec::serialize_message(message)?;
        tracing::debug!(
            "RAFT_SEND_IMMEDIATE: Serialized message has {} bytes",
            msg_data.len()
        );

        // Create RPC request for the raft service
        let request_id = generate_request_id();
        let request = crate::transport::iroh_protocol::RpcRequest {
            service: "raft".to_string(),
            method: "raft.message".to_string(),
            payload: bytes::Bytes::from(msg_data),
        };
        
        // Serialize RPC request
        let request_bytes = crate::transport::iroh_protocol::serialize_payload(&request)?;

        // Open bidirectional stream for RPC
        tracing::debug!(
            "RAFT_SEND_IMMEDIATE: Opening bi stream for priority {:?} to node {}",
            priority, to
        );
        let (mut send, mut recv) = conn.connection.open_bi().await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to open bi stream: {}", e),
            })?;

        // Send RPC request
        tracing::debug!(
            "RAFT_SEND_IMMEDIATE: Writing RPC message to stream for node {} (request_id: {:?})",
            to, request_id
        );
        write_message(&mut send, ProtocolMessageType::Request, request_id, &request_bytes).await?;
        
        // Close the send stream to signal we're done sending
        send.finish().map_err(|e| BlixardError::Internal {
            message: format!("Failed to finish send stream: {}", e),
        })?;
        
        // Wait for response (just to confirm delivery) 
        let _ = read_message(&mut recv).await;
        
        tracing::debug!(
            "RAFT_SEND_IMMEDIATE: Successfully sent {:?} message to node {}",
            message.msg_type(), to
        );

        // Update metrics
        self.record_message_sent(message.msg_type());

        // Update last activity
        conn.last_activity = Instant::now();
        
        tracing::info!(
            "RAFT_SEND_IMMEDIATE: Successfully sent {:?} message to node {}",
            message.msg_type(), to
        );

        Ok(())
    }

    /// Buffer a message for batched sending
    async fn buffer_message(
        &self,
        to: u64,
        message: Message,
        priority: RaftMessagePriority,
    ) -> BlixardResult<()> {
        tracing::debug!(
            "RAFT_BUFFER: Buffering {:?} message (priority {:?}) for node {}",
            message.msg_type(), priority, to
        );
        
        let mut buffer = self.message_buffer.lock().await;
        let queue = buffer.entry(to).or_insert_with(VecDeque::new);
        
        let initial_size = queue.len();

        // Add message to queue based on priority
        let buffered = BufferedRaftMessage {
            message,
            priority,
            timestamp: Instant::now(),
            attempts: 0,
        };

        // Insert based on priority (higher priority at front)
        let insert_pos = queue
            .iter()
            .position(|m| m.priority.as_u8() > priority.as_u8())
            .unwrap_or(queue.len());
        queue.insert(insert_pos, buffered);

        // Limit buffer size
        let max_buffered = config_global::get()?.cluster.peer.max_buffered_messages;
        while queue.len() > max_buffered {
            queue.pop_back();
        }
        
        tracing::debug!(
            "RAFT_BUFFER: Buffered message for node {} (queue size: {} -> {}, max: {})",
            to, initial_size, queue.len(), max_buffered
        );

        Ok(())
    }

    /// Get or create a connection to a peer
    async fn get_or_create_connection(&self, peer_id: u64) -> BlixardResult<PeerConnection> {
        tracing::debug!("RAFT_CONNECT: Getting or creating connection to peer {}", peer_id);
        
        // Check existing connections
        {
            let connections = self.connections.read().await;
            if let Some(conn) = connections.get(&peer_id) {
                tracing::debug!(
                    "RAFT_CONNECT: Found existing connection to peer {} (Iroh ID: {})",
                    peer_id, conn.iroh_node_id
                );
                return Ok(PeerConnection {
                    node_id: conn.node_id,
                    iroh_node_id: conn.iroh_node_id,
                    connection: conn.connection.clone(),
                    election_stream: None,
                    heartbeat_stream: None,
                    append_stream: None,
                    snapshot_stream: None,
                    last_activity: conn.last_activity,
                });
            }
        }
        
        tracing::debug!("RAFT_CONNECT: No existing connection, creating new one for peer {}", peer_id);

        // Create new connection using NodeRegistry
        let node_registry = self.node.node_registry();
        
        tracing::debug!(
            "RAFT_CONNECT: Looking up peer {} in node registry",
            peer_id
        );
        
        // First check what's in the registry
        let registry_entries = node_registry.list_nodes().await;
        tracing::debug!(
            "RAFT_CONNECT: Node registry contains {} entries: {:?}",
            registry_entries.len(),
            registry_entries.iter().map(|e| (e.cluster_node_id, &e.iroh_node_id)).collect::<Vec<_>>()
        );
        
        // Get the complete NodeAddr from the registry
        let node_addr = match node_registry.get_node_addr(peer_id).await {
            Ok(addr) => {
                tracing::debug!(
                    "RAFT_CONNECT: Found peer {} in registry with Iroh ID {}, {} direct addresses, relay: {:?}",
                    peer_id,
                    addr.node_id,
                    addr.direct_addresses().count(),
                    addr.relay_url()
                );
                addr
            }
            Err(e) => {
                tracing::warn!(
                    "RAFT_CONNECT: Peer {} not found in node registry: {}. Trying cluster members fallback. Available peers: {:?}",
                    peer_id, e,
                    registry_entries.iter().map(|e| e.cluster_node_id).collect::<Vec<_>>()
                );
                
                // Fallback: try to get P2P info from cluster members
                if let Some(peer_info) = self.node.get_peer(peer_id).await {
                    if let Some(p2p_node_id_str) = &peer_info.p2p_node_id {
                        if let Ok(iroh_node_id) = p2p_node_id_str.parse::<iroh::NodeId>() {
                            tracing::info!(
                                "RAFT_CONNECT: Found peer {} in cluster members, creating NodeAddr from P2P info",
                                peer_id
                            );
                            
                            // Create NodeAddr from cluster member P2P info
                            let mut node_addr = iroh::NodeAddr::new(iroh_node_id);
                            
                            // Add addresses if available
                            let addrs: Vec<std::net::SocketAddr> = peer_info.p2p_addresses
                                .iter()
                                .filter_map(|a| a.parse().ok())
                                .collect();
                            if !addrs.is_empty() {
                                node_addr = node_addr.with_direct_addresses(addrs);
                            }
                            
                            // Add relay URL if available
                            if let Some(relay) = &peer_info.p2p_relay_url {
                                if let Ok(relay_url) = relay.parse() {
                                    node_addr = node_addr.with_relay_url(relay_url);
                                }
                            }
                            
                            // Also register in node registry for future lookups
                            if let Err(reg_err) = node_registry.register_node(
                                peer_id,
                                iroh_node_id,
                                node_addr.clone(),
                                Some(peer_info.address.clone())
                            ).await {
                                tracing::warn!(
                                    "RAFT_CONNECT: Failed to register peer {} in node registry during fallback: {}",
                                    peer_id, reg_err
                                );
                            } else {
                                tracing::info!(
                                    "RAFT_CONNECT: Successfully registered peer {} in node registry during fallback",
                                    peer_id
                                );
                            }
                            
                            node_addr
                        } else {
                            tracing::error!(
                                "RAFT_CONNECT: Invalid Iroh node ID for peer {} in cluster members: {}",
                                peer_id, p2p_node_id_str
                            );
                            return Err(BlixardError::ClusterError(
                                format!("Peer {} has invalid Iroh node ID: {}", peer_id, p2p_node_id_str)
                            ));
                        }
                    } else {
                        tracing::error!(
                            "RAFT_CONNECT: Peer {} found in cluster members but has no P2P node ID",
                            peer_id
                        );
                        return Err(BlixardError::ClusterError(
                            format!("Peer {} has no P2P node ID", peer_id)
                        ));
                    }
                } else {
                    tracing::error!(
                        "RAFT_CONNECT: Peer {} not found in node registry or cluster members",
                        peer_id
                    );
                    return Err(BlixardError::ClusterError(
                        format!("Peer {} not found in node registry or cluster members", peer_id)
                    ));
                }
            }
        };
        
        let iroh_node_id = node_addr.node_id;
        
        tracing::info!(
            "RAFT_CONNECT: Attempting to connect to peer {} with Iroh node ID {} using {} direct addresses and relay: {:?}",
            peer_id,
            iroh_node_id,
            node_addr.direct_addresses().count(),
            node_addr.relay_url()
        );

        // Connect to peer using standard BLIXARD_ALPN
        tracing::debug!(
            "RAFT_CONNECT: Calling endpoint.connect() for peer {} with ALPN {:?}",
            peer_id, 
            std::str::from_utf8(crate::transport::BLIXARD_ALPN).unwrap_or("invalid-utf8")
        );
        
        // Add timeout to connection attempt (10 seconds)
        let connection_timeout = Duration::from_secs(10);
        let connection = match tokio::time::timeout(
            connection_timeout,
            self.endpoint.connect(node_addr, crate::transport::BLIXARD_ALPN)
        ).await {
            Ok(Ok(conn)) => {
                tracing::info!(
                    "RAFT_CONNECT: Successfully connected to peer {} (Iroh ID: {})",
                    peer_id, iroh_node_id
                );
                conn
            }
            Ok(Err(e)) => {
                tracing::error!(
                    "RAFT_CONNECT: Failed to connect to peer {} (Iroh ID: {}): {}",
                    peer_id, iroh_node_id, e
                );
                return Err(BlixardError::Internal {
                    message: format!("Connection error to peer {}: {}", peer_id, e),
                });
            }
            Err(_) => {
                tracing::error!(
                    "RAFT_CONNECT: Connection timeout to peer {} (Iroh ID: {}) after {:?}",
                    peer_id, iroh_node_id, connection_timeout
                );
                return Err(BlixardError::Timeout {
                    operation: format!("connect to peer {}", peer_id),
                    duration: connection_timeout,
                });
            }
        };

        // Store connection
        let peer_conn = PeerConnection {
            node_id: peer_id,
            iroh_node_id,
            connection: connection.clone(),
            election_stream: None,
            heartbeat_stream: None,
            append_stream: None,
            snapshot_stream: None,
            last_activity: Instant::now(),
        };

        {
            let mut connections = self.connections.write().await;
            connections.insert(peer_id, peer_conn);
            tracing::debug!(
                "RAFT_CONNECT: Stored connection to peer {} in connection cache",
                peer_id
            );
        }

        Ok(PeerConnection {
            node_id: peer_id,
            iroh_node_id,
            connection,
            election_stream: None,
            heartbeat_stream: None,
            append_stream: None,
            snapshot_stream: None,
            last_activity: Instant::now(),
        })
    }

    /// Spawn task to accept incoming connections
    #[allow(dead_code)] // Future: implement automatic connection acceptance
    fn spawn_accept_task(&self) -> JoinHandle<()> {
        let endpoint = self.endpoint.clone();
        let raft_rx_tx = self.raft_rx_tx.clone();
        let node = self.node.clone();
        let messages_received = self.messages_received.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            tracing::info!("Raft transport accept task shutting down");
                            break;
                        }
                    }
                    incoming = endpoint.accept() => {
                        match incoming {
                            Some(incoming) => {
                                let raft_rx_tx = raft_rx_tx.clone();
                                let node = node.clone();
                                let messages_received = messages_received.clone();

                                tokio::spawn(async move {
                                    match incoming.await {
                                        Ok(connection) => {
                                            if let Err(e) = handle_incoming_connection(
                                                connection,
                                                raft_rx_tx,
                                                node,
                                                messages_received,
                                            ).await {
                                                tracing::warn!("Failed to handle incoming Raft connection: {}", e);
                                            }
                                        }
                                        Err(e) => {
                                            tracing::warn!("Failed to accept incoming connection: {}", e);
                                        }
                                    }
                                });
                            }
                            None => {
                                tracing::info!("Endpoint closed, stopping accept task");
                                break;
                            }
                        }
                    }
                }
            }
        })
    }

    /// Spawn task to process message batches
    fn spawn_batch_processor(&self) -> JoinHandle<()> {
        let message_buffer = self.message_buffer.clone();
        let connections = self.connections.clone();
        let node = self.node.clone();
        let messages_sent = self.messages_sent.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();
        let endpoint = self.endpoint.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(10)); // 10ms batching

            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            tracing::info!("Raft transport batch processor shutting down");
                            break;
                        }
                    }
                    _ = interval.tick() => {
                        process_message_batches(
                            &message_buffer,
                            &connections,
                            &node,
                            &messages_sent,
                            &endpoint,
                        ).await;
                    }
                }
            }
        })
    }

    /// Spawn task to check connection health
    fn spawn_health_checker(&self) -> JoinHandle<()> {
        let connections = self.connections.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));

            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            tracing::info!("Raft transport health checker shutting down");
                            break;
                        }
                    }
                    _ = interval.tick() => {
                        check_connection_health(&connections).await;
                    }
                }
            }
        })
    }

    /// Record sent message metrics
    fn record_message_sent(&self, msg_type: MessageType) {
        // Use try_lock to avoid blocking in async context
        if let Ok(mut sent) = self.messages_sent.try_lock() {
            let key = format!("{:?}", msg_type);
            *sent.entry(key).or_insert(0) += 1;
        }
        // If we can't get the lock, skip metrics recording rather than panic

        #[cfg(feature = "observability")]
        RAFT_MESSAGES_SENT.add(
            1,
            &[
                attributes::TRANSPORT_TYPE.string("iroh"),
                attributes::MESSAGE_TYPE.string(format!("{:?}", msg_type)),
            ],
        );
    }

    /// Get transport metrics
    pub async fn get_metrics(&self) -> IrohRaftMetrics {
        let connections = self.connections.read().await;
        let active_connections = connections.len();
        
        let messages_sent = {
            let sent = self.messages_sent.lock().await;
            sent.values().sum()
        };
        
        let messages_received = {
            let received = self.messages_received.lock().await;
            received.values().sum()
        };
        
        IrohRaftMetrics {
            active_connections,
            messages_sent,
            messages_received,
        }
    }

    /// Shutdown the transport
    pub async fn shutdown(&self) {
        tracing::info!("Shutting down Iroh Raft transport");

        // Signal shutdown
        let _ = self.shutdown_tx.send(true);

        // Wait for tasks
        let mut tasks = self.tasks.lock().await;
        for task in tasks.drain(..) {
            let _ = task.await;
        }

        // Close connections
        let mut connections = self.connections.write().await;
        connections.clear();

        tracing::info!("Iroh Raft transport shutdown complete");
    }
}

/// Handle an incoming Raft connection
#[allow(dead_code)] // Future: implement automatic connection handling
async fn handle_incoming_connection(
    connection: iroh::endpoint::Connection,
    raft_rx_tx: mpsc::UnboundedSender<(u64, Message)>,
    node: Arc<SharedNodeState>,
    messages_received: Arc<Mutex<HashMap<String, u64>>>,
) -> BlixardResult<()> {
    let remote_node_id = connection
        .remote_node_id()
        .map_err(|e| BlixardError::Internal {
            message: format!("Failed to get remote node ID: {}", e),
        })?;

    // Find the peer ID for this Iroh node
    let peers = node.get_peers().await;
    let peer_id = peers
        .iter()
        .find(|p| p.node_id == remote_node_id.to_string())
        .and_then(|p| {
            p.node_id.parse::<u64>().map_err(|e| {
                tracing::warn!("Failed to parse peer node ID '{}': {}", p.node_id, e);
                e
            }).ok()
        })
        .ok_or_else(|| {
            BlixardError::ClusterError(format!("Unknown or invalid Iroh node: {}", remote_node_id))
        })?;

    // Accept streams
    loop {
        match connection.accept_uni().await {
            Ok(mut stream) => {
                let raft_rx_tx = raft_rx_tx.clone();
                let messages_received = messages_received.clone();

                tokio::spawn(async move {
                    if let Err(e) =
                        handle_raft_stream(&mut stream, peer_id, raft_rx_tx, messages_received)
                            .await
                    {
                        tracing::warn!("Failed to handle Raft stream from {}: {}", peer_id, e);
                    }
                });
            }
            Err(e) => {
                tracing::debug!("Connection closed from {}: {}", peer_id, e);
                break;
            }
        }
    }

    Ok(())
}

/// Handle a single Raft message stream
#[allow(dead_code)] // Future: implement stream-based message handling
async fn handle_raft_stream(
    stream: &mut RecvStream,
    from: u64,
    raft_rx_tx: mpsc::UnboundedSender<(u64, Message)>,
    messages_received: Arc<Mutex<HashMap<String, u64>>>,
) -> BlixardResult<()> {
    // Read messages from stream
    loop {
        match read_message(stream).await {
            Ok((header, payload)) => {
                if header.msg_type != ProtocolMessageType::Request {
                    continue;
                }

                // Deserialize Raft message
                let message: Message = crate::raft_codec::deserialize_message(&payload)?;

                // Record metrics
                {
                    let mut received = messages_received.lock().await;
                    let key = format!("{:?}", message.msg_type());
                    *received.entry(key.clone()).or_insert(0) += 1;

                    #[cfg(feature = "observability")]
                    RAFT_MESSAGES_RECEIVED.add(
                        1,
                        &[
                            attributes::TRANSPORT_TYPE.string("iroh"),
                            attributes::MESSAGE_TYPE.string(key),
                        ],
                    );
                }

                // Send to Raft manager
                if let Err(e) = raft_rx_tx.send((from, message)) {
                    tracing::error!("Failed to send Raft message to manager: {}", e);
                    break;
                }
            }
            Err(e) => {
                tracing::debug!("Stream ended from {}: {}", from, e);
                break;
            }
        }
    }

    Ok(())
}

/// Process batched messages
async fn process_message_batches(
    message_buffer: &Arc<Mutex<HashMap<u64, VecDeque<BufferedRaftMessage>>>>,
    connections: &Arc<RwLock<HashMap<u64, PeerConnection>>>,
    node: &Arc<SharedNodeState>,
    messages_sent: &Arc<Mutex<HashMap<String, u64>>>,
    endpoint: &Endpoint,
) {
    let mut buffer = message_buffer.lock().await;
    
    // Log buffer status
    let total_messages: usize = buffer.values().map(|q| q.len()).sum();
    if total_messages > 0 {
        tracing::debug!(
            "RAFT_BATCH: Processing {} total buffered messages across {} peers",
            total_messages,
            buffer.len()
        );
    }

    for (peer_id, messages) in buffer.iter_mut() {
        if messages.is_empty() {
            continue;
        }
        
        tracing::debug!(
            "RAFT_BATCH: Processing {} buffered messages for peer {}",
            messages.len(),
            peer_id
        );

        // Try to get connection
        let conn_result = {
            let connections = connections.read().await;
            connections.get(peer_id).map(|c| c.connection.clone())
        };

        let connection = match conn_result {
            Some(conn) => {
                tracing::debug!(
                    "RAFT_BATCH: Using existing connection for peer {}",
                    peer_id
                );
                conn
            }
            None => {
                tracing::debug!(
                    "RAFT_BATCH: No existing connection for peer {}, creating new one",
                    peer_id
                );
                // Try to create connection
                match create_connection_for_peer(*peer_id, node, endpoint).await {
                    Ok(conn) => {
                        let connection = conn.connection.clone();
                        // Store new connection
                        let mut connections = connections.write().await;
                        connections.insert(*peer_id, conn);
                        tracing::debug!(
                            "RAFT_BATCH: Created new connection for peer {}",
                            peer_id
                        );
                        connection
                    }
                    Err(e) => {
                        tracing::warn!(
                            "RAFT_BATCH: Failed to connect to peer {}: {}",
                            peer_id, e
                        );
                        continue;
                    }
                }
            }
        };

        // Group messages by priority
        let mut by_priority: HashMap<RaftMessagePriority, Vec<Message>> = HashMap::new();
        let mut sent_count = 0;

        while let Some(buffered) = messages.pop_front() {
            // Skip old messages
            if buffered.timestamp.elapsed() > Duration::from_secs(30) {
                tracing::debug!(
                    "RAFT_BATCH: Skipping old message to peer {} (age: {:?})",
                    peer_id,
                    buffered.timestamp.elapsed()
                );
                continue;
            }

            by_priority
                .entry(buffered.priority)
                .or_insert_with(Vec::new)
                .push(buffered.message);

            sent_count += 1;
            if sent_count >= 100 {
                // Batch size limit
                tracing::debug!(
                    "RAFT_BATCH: Hit batch size limit of 100 for peer {}",
                    peer_id
                );
                break;
            }
        }

        // Send batches by priority
        for (priority, batch) in by_priority {
            tracing::debug!(
                "RAFT_BATCH: Sending batch of {} {:?} messages to peer {}",
                batch.len(),
                priority,
                peer_id
            );
            if let Err(e) = send_message_batch(&connection, priority, batch, messages_sent).await {
                tracing::warn!(
                    "RAFT_BATCH: Failed to send batch to peer {}: {}",
                    peer_id, e
                );
            } else {
                tracing::debug!(
                    "RAFT_BATCH: Successfully sent batch to peer {}",
                    peer_id
                );
            }
        }
    }
}

/// Send a batch of messages using RPC to the raft service
async fn send_message_batch(
    connection: &Connection,
    _priority: RaftMessagePriority,
    messages: Vec<Message>,
    messages_sent: &Arc<Mutex<HashMap<String, u64>>>,
) -> BlixardResult<()> {
    // Send each message as an RPC call to the raft service
    for message in messages {
        // Serialize the Raft message
        let msg_data = crate::raft_codec::serialize_message(&message)?;
        
        // Create RPC request for the raft service
        let request_id = crate::transport::iroh_protocol::generate_request_id();
        let request = crate::transport::iroh_protocol::RpcRequest {
            service: "raft".to_string(),
            method: "raft.message".to_string(),
            payload: bytes::Bytes::from(msg_data),
        };
        
        // Open bidirectional stream for RPC
        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to open bi stream: {}", e),
            })?;
        
        // Serialize and send the RPC request
        let request_bytes = crate::transport::iroh_protocol::serialize_payload(&request)?;
            
        crate::transport::iroh_protocol::write_message(
            &mut send,
            crate::transport::iroh_protocol::MessageType::Request,
            request_id,
            &request_bytes,
        )
        .await?;
        
        // Close the send stream to signal we're done sending
        send.finish().map_err(|e| BlixardError::Internal {
            message: format!("Failed to finish send stream: {}", e),
        })?;
        
        // Wait for response (just to confirm delivery)
        let _ = crate::transport::iroh_protocol::read_message(&mut recv).await;
        
        // Record metrics
        {
            let mut sent = messages_sent.lock().await;
            let key = format!("{:?}", message.msg_type());
            *sent.entry(key.clone()).or_insert(0) += 1;

            #[cfg(feature = "observability")]
            RAFT_MESSAGES_SENT.add(
                1,
                &[
                    attributes::TRANSPORT_TYPE.string("iroh"),
                    attributes::MESSAGE_TYPE.string(key),
                ],
            );
        }
    }

    Ok(())
}

/// Create a connection for a specific peer
async fn create_connection_for_peer(
    peer_id: u64,
    node: &Arc<SharedNodeState>,
    endpoint: &Endpoint,
) -> BlixardResult<PeerConnection> {
    let peer_info = node
        .get_peer(peer_id)
        .await
        .ok_or_else(|| BlixardError::ClusterError(format!("Unknown peer {}", peer_id)))?;

    let p2p_node_id = peer_info
        .p2p_node_id
        .ok_or_else(|| BlixardError::Internal {
            message: format!("Peer {} has no P2P node ID", peer_id),
        })?;
    let iroh_node_id = p2p_node_id
        .parse::<NodeId>()
        .map_err(|e| BlixardError::Internal {
            message: format!("Invalid Iroh node ID: {}", e),
        })?;

    // Add timeout to connection attempt (10 seconds)
    let connection_timeout = Duration::from_secs(10);
    let connection = match tokio::time::timeout(
        connection_timeout,
        endpoint.connect(iroh_node_id, super::BLIXARD_ALPN)
    ).await {
        Ok(Ok(conn)) => conn,
        Ok(Err(e)) => {
            return Err(BlixardError::Internal {
                message: format!("Failed to connect to peer {}: {}", peer_id, e),
            });
        }
        Err(_) => {
            return Err(BlixardError::Timeout {
                operation: format!("connect to peer {}", peer_id),
                duration: connection_timeout,
            });
        }
    };

    Ok(PeerConnection {
        node_id: peer_id,
        iroh_node_id,
        connection,
        election_stream: None,
        heartbeat_stream: None,
        append_stream: None,
        snapshot_stream: None,
        last_activity: Instant::now(),
    })
}

/// Check health of all connections
async fn check_connection_health(connections: &Arc<RwLock<HashMap<u64, PeerConnection>>>) {
    let mut to_remove = Vec::new();

    {
        let connections = connections.read().await;
        for (peer_id, conn) in connections.iter() {
            if conn.last_activity.elapsed() > Duration::from_secs(60) {
                to_remove.push(*peer_id);
            }
        }
    }

    if !to_remove.is_empty() {
        let mut connections = connections.write().await;
        for peer_id in to_remove {
            connections.remove(&peer_id);
            tracing::info!("Removed stale connection to peer {}", peer_id);
        }
    }
}

/// Extension trait to integrate with existing PeerConnector
impl IrohRaftTransport {
    /// Send a Raft message using this transport (compatible with PeerConnector interface)
    pub async fn send_raft_message(
        &self,
        to: u64,
        message: raft::prelude::Message,
    ) -> BlixardResult<()> {
        self.send_message(to, message).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_priority() {
        let mut vote_msg = Message::default();
        vote_msg.set_msg_type(MessageType::MsgRequestVote);
        assert_eq!(
            RaftMessagePriority::from_raft_message(&vote_msg),
            RaftMessagePriority::Election
        );

        let mut heartbeat_msg = Message::default();
        heartbeat_msg.set_msg_type(MessageType::MsgHeartbeat);
        assert_eq!(
            RaftMessagePriority::from_raft_message(&heartbeat_msg),
            RaftMessagePriority::Heartbeat
        );

        let mut append_msg = Message::default();
        append_msg.set_msg_type(MessageType::MsgAppend);
        assert_eq!(
            RaftMessagePriority::from_raft_message(&append_msg),
            RaftMessagePriority::LogAppend
        );

        let mut snapshot_msg = Message::default();
        snapshot_msg.set_msg_type(MessageType::MsgSnapshot);
        assert_eq!(
            RaftMessagePriority::from_raft_message(&snapshot_msg),
            RaftMessagePriority::Snapshot
        );
    }

    #[test]
    fn test_priority_ordering() {
        assert!(RaftMessagePriority::Election.as_u8() < RaftMessagePriority::Heartbeat.as_u8());
        assert!(RaftMessagePriority::Heartbeat.as_u8() < RaftMessagePriority::LogAppend.as_u8());
        assert!(RaftMessagePriority::LogAppend.as_u8() < RaftMessagePriority::Snapshot.as_u8());
    }
}
