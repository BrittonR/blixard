//! Iroh-based peer connection management for cluster nodes
//!
//! This module provides P2P connection management using Iroh transport,
//! maintaining feature parity with the gRPC PeerConnector.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Mutex, watch};
use tokio::task::JoinHandle;
use dashmap::DashMap;

use crate::{
    error::{BlixardError, BlixardResult},
    node_shared::{SharedNodeState, PeerInfo},
    metrics_otel::{metrics, Timer, attributes},
    config_global,
};
use iroh::{Endpoint, NodeAddr, NodeId};

/// Circuit breaker states
#[derive(Debug, Clone, PartialEq)]
enum CircuitState {
    Closed,     // Normal operation
    Open,       // Failing, reject all attempts
    HalfOpen,   // Testing if service has recovered
}

/// Circuit breaker for connection management
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    state: CircuitState,
    consecutive_failures: u32,
    last_failure: Option<Instant>,
    last_success: Option<Instant>,
    opened_at: Option<Instant>,
    failure_threshold: u32,
    reset_timeout: Duration,
}

impl CircuitBreaker {
    fn new(failure_threshold: u32, reset_timeout: Duration) -> Self {
        Self {
            state: CircuitState::Closed,
            consecutive_failures: 0,
            last_failure: None,
            last_success: None,
            opened_at: None,
            failure_threshold,
            reset_timeout,
        }
    }
    
    fn should_allow_request(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                if let Some(opened_at) = self.opened_at {
                    if opened_at.elapsed() >= self.reset_timeout {
                        self.state = CircuitState::HalfOpen;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }
    
    fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.last_success = Some(Instant::now());
        
        if self.state == CircuitState::HalfOpen {
            tracing::info!("Circuit breaker transitioning to closed");
        }
        
        self.state = CircuitState::Closed;
        self.opened_at = None;
    }
    
    fn record_failure(&mut self) {
        self.consecutive_failures += 1;
        self.last_failure = Some(Instant::now());
        
        if self.consecutive_failures >= self.failure_threshold {
            if self.state != CircuitState::Open {
                tracing::warn!("Circuit breaker opening after {} failures", self.consecutive_failures);
                self.state = CircuitState::Open;
                self.opened_at = Some(Instant::now());
            }
        }
        
        if self.state == CircuitState::HalfOpen {
            self.state = CircuitState::Open;
            self.opened_at = Some(Instant::now());
        }
    }
}

/// Connection metadata
#[derive(Debug, Clone)]
struct ConnectionInfo {
    node_addr: NodeAddr,
    established_at: Instant,
    last_used: Instant,
    bytes_sent: u64,
    bytes_received: u64,
}

/// Buffered message for delayed sending
#[derive(Debug, Clone)]
struct BufferedMessage {
    to: u64,
    data: Vec<u8>,
    message_type: MessageType,
    timestamp: Instant,
    attempts: u32,
}

#[derive(Debug, Clone)]
enum MessageType {
    RaftMessage,
    StatusQuery,
    HealthCheck,
    VmOperation,
}

/// Client connection wrapper with metadata
#[derive(Clone)]
pub struct IrohClient {
    endpoint: Endpoint,
    node_addr: NodeAddr,
    connection_info: Arc<RwLock<ConnectionInfo>>,
    rpc_client: Arc<crate::transport::iroh_service::IrohRpcClient>,
}

impl IrohClient {
    async fn new(endpoint: Endpoint, node_addr: NodeAddr) -> BlixardResult<Self> {
        let connection_info = Arc::new(RwLock::new(ConnectionInfo {
            node_addr: node_addr.clone(),
            established_at: Instant::now(),
            last_used: Instant::now(),
            bytes_sent: 0,
            bytes_received: 0,
        }));
        
        // Create our custom RPC client
        let rpc_client = Arc::new(crate::transport::iroh_service::IrohRpcClient::new(endpoint.clone()));
        
        Ok(Self {
            endpoint,
            node_addr,
            connection_info,
            rpc_client,
        })
    }
    
    /// Get the RPC client
    pub fn rpc_client(&self) -> &Arc<crate::transport::iroh_service::IrohRpcClient> {
        &self.rpc_client
    }
    
    /// Update last used time
    pub async fn touch(&self) {
        let mut info = self.connection_info.write().await;
        info.last_used = Instant::now();
    }
    
    /// Update connection statistics
    pub async fn update_stats(&self, bytes_sent: u64, bytes_received: u64) {
        let mut info = self.connection_info.write().await;
        info.bytes_sent += bytes_sent;
        info.bytes_received += bytes_received;
    }
    
    /// Get the node address
    pub fn node_addr(&self) -> &NodeAddr {
        &self.node_addr
    }
    
    /// Get the endpoint
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }
}

/// Manages P2P connections to peer nodes using Iroh
pub struct IrohPeerConnector {
    endpoint: Endpoint,
    node: Arc<SharedNodeState>,
    connections: Arc<DashMap<u64, IrohClient>>,
    peers: Arc<RwLock<HashMap<u64, PeerInfo>>>,
    circuit_breakers: Arc<DashMap<u64, CircuitBreaker>>,
    message_buffer: Arc<Mutex<HashMap<u64, VecDeque<BufferedMessage>>>>,
    connecting: Arc<Mutex<HashMap<u64, bool>>>,
    connection_count: Arc<Mutex<usize>>,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
    background_tasks: Mutex<Vec<JoinHandle<()>>>,
}

impl IrohPeerConnector {
    /// Create a new Iroh peer connector
    pub fn new(endpoint: Endpoint, node: Arc<SharedNodeState>) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        
        Self {
            endpoint,
            node,
            connections: Arc::new(DashMap::new()),
            peers: Arc::new(RwLock::new(HashMap::new())),
            circuit_breakers: Arc::new(DashMap::new()),
            message_buffer: Arc::new(Mutex::new(HashMap::new())),
            connecting: Arc::new(Mutex::new(HashMap::new())),
            connection_count: Arc::new(Mutex::new(0)),
            shutdown_tx,
            shutdown_rx,
            background_tasks: Mutex::new(Vec::new()),
        }
    }
    
    /// Start background tasks for connection management
    pub async fn start(&self) -> BlixardResult<()> {
        // Start health check task
        let health_task = self.start_health_check_task();
        
        // Start message buffer processing task
        let buffer_task = self.start_buffer_processing_task();
        
        // Start connection cleanup task
        let cleanup_task = self.start_connection_cleanup_task();
        
        // Store task handles
        let mut tasks = self.background_tasks.lock().await;
        tasks.push(health_task);
        tasks.push(buffer_task);
        tasks.push(cleanup_task);
        
        Ok(())
    }
    
    /// Connect to a peer using their node ID and address info
    pub async fn connect_to_peer(&self, peer_id: u64) -> BlixardResult<IrohClient> {
        let metrics = metrics();
        let peer_attrs = vec![attributes::peer_id(peer_id)];
        let _timer = Timer::with_attributes(
            metrics.grpc_request_duration.clone(),
            peer_attrs.clone()
        );
        metrics.peer_reconnect_attempts.add(1, &peer_attrs);
        
        // Check if already connected
        if let Some(client) = self.connections.get(&peer_id) {
            tracing::debug!("Already connected to peer {}", peer_id);
            return Ok(client.clone());
        }
        
        // Check connection pool limit
        {
            let count = self.connection_count.lock().await;
            let max_connections = config_global::get().cluster.peer.max_connections;
            if *count >= max_connections {
                return Err(BlixardError::ClusterError(
                    format!("Connection pool limit reached ({})", max_connections)
                ));
            }
        }
        
        // Check circuit breaker
        let mut breaker = self.circuit_breakers
            .entry(peer_id)
            .or_insert_with(|| CircuitBreaker::new(
                config_global::get().cluster.peer.failure_threshold,
                Duration::from_secs(30),
            ));
        
        if !breaker.should_allow_request() {
            return Err(BlixardError::ClusterError(
                format!("Circuit breaker open for peer {}", peer_id)
            ));
        }
        
        // Mark as connecting
        {
            let mut connecting = self.connecting.lock().await;
            if connecting.get(&peer_id).copied().unwrap_or(false) {
                return Err(BlixardError::ClusterError(
                    format!("Already connecting to peer {}", peer_id)
                ));
            }
            connecting.insert(peer_id, true);
        }
        
        // Get peer info and convert to NodeAddr
        let peer_info = self.get_peer_info(peer_id).await?;
        let node_addr = self.peer_info_to_node_addr(&peer_info)?;
        
        // Attempt connection
        match IrohClient::new(self.endpoint.clone(), node_addr).await {
            Ok(client) => {
                // Store connection
                self.connections.insert(peer_id, client.clone());
                
                // Update connection count
                {
                    let mut count = self.connection_count.lock().await;
                    *count += 1;
                }
                
                // Update peer status
                self.node.update_peer_connection(peer_id, true).await?;
                
                // Record success
                breaker.record_success();
                
                // Remove from connecting
                {
                    let mut connecting = self.connecting.lock().await;
                    connecting.remove(&peer_id);
                }
                
                // Process buffered messages
                self.send_buffered_messages(peer_id).await;
                
                tracing::info!("Connected to peer {} via Iroh", peer_id);
                Ok(client)
            }
            Err(e) => {
                // Record failure
                breaker.record_failure();
                
                // Update peer status
                self.node.update_peer_connection(peer_id, false).await?;
                
                // Remove from connecting
                {
                    let mut connecting = self.connecting.lock().await;
                    connecting.remove(&peer_id);
                }
                
                Err(e)
            }
        }
    }
    
    /// Get peer info from node state
    async fn get_peer_info(&self, peer_id: u64) -> BlixardResult<PeerInfo> {
        let peers = self.node.get_peers().await;
        peers.into_iter()
            .find(|p| p.id == peer_id)
            .ok_or_else(|| BlixardError::NotFound {
                resource: format!("Peer {}", peer_id),
            })
    }
    
    /// Convert PeerInfo to Iroh NodeAddr
    fn peer_info_to_node_addr(&self, peer: &PeerInfo) -> BlixardResult<NodeAddr> {
        // Parse node ID from P2P info if available
        if let Some(ref p2p_node_id) = peer.p2p_node_id {
            if !p2p_node_id.is_empty() {
                if let Ok(node_id) = p2p_node_id.parse::<NodeId>() {
                    let mut node_addr = NodeAddr::new(node_id);
                    
                    // Add relay URL if available
                    if let Some(ref p2p_relay_url) = peer.p2p_relay_url {
                        if !p2p_relay_url.is_empty() {
                            if let Ok(relay_url) = p2p_relay_url.parse() {
                                node_addr = node_addr.with_relay_url(relay_url);
                            }
                        }
                    }
                
                // Add direct addresses
                for addr_str in &peer.p2p_addresses {
                    if let Ok(addr) = addr_str.parse() {
                        node_addr = node_addr.with_direct_addresses([addr]);
                    }
                }
                    
                    return Ok(node_addr);
                }
            }
        }
        
        Err(BlixardError::Internal {
            message: format!("Cannot convert peer {} to Iroh address", peer.id),
        })
    }
    
    /// Buffer a message for later delivery
    pub async fn buffer_message(
        &self,
        peer_id: u64,
        data: Vec<u8>,
        message_type: MessageType,
    ) -> BlixardResult<()> {
        let mut buffer = self.message_buffer.lock().await;
        let queue = buffer.entry(peer_id).or_insert_with(VecDeque::new);
        
        // Limit buffer size
        if queue.len() >= 1000 {
            queue.pop_front();
        }
        
        queue.push_back(BufferedMessage {
            to: peer_id,
            data,
            message_type,
            timestamp: Instant::now(),
            attempts: 0,
        });
        
        Ok(())
    }
    
    /// Send buffered messages to a peer
    async fn send_buffered_messages(&self, peer_id: u64) {
        let messages = {
            let mut buffer = self.message_buffer.lock().await;
            buffer.remove(&peer_id).unwrap_or_default()
        };
        
        if messages.is_empty() {
            return;
        }
        
        tracing::debug!("Sending {} buffered messages to peer {}", messages.len(), peer_id);
        
        if let Some(client) = self.connections.get(&peer_id) {
            for msg in messages {
                // TODO: Actually send the message based on type
                let _ = client.update_stats(msg.data.len() as u64, 0).await;
            }
        }
    }
    
    /// Disconnect from a peer
    pub async fn disconnect_peer(&self, peer_id: u64) -> BlixardResult<()> {
        // Remove connection
        if let Some(_) = self.connections.remove(&peer_id) {
            // Update connection count
            let mut count = self.connection_count.lock().await;
            *count = count.saturating_sub(1);
            
            // Update peer status
            self.node.update_peer_connection(peer_id, false).await?;
            
            tracing::info!("Disconnected from peer {}", peer_id);
        }
        
        Ok(())
    }
    
    /// Get an active connection to a peer
    pub async fn get_connection(&self, peer_id: u64) -> BlixardResult<IrohClient> {
        if let Some(client) = self.connections.get(&peer_id) {
            Ok(client.clone())
        } else {
            self.connect_to_peer(peer_id).await
        }
    }
    
    /// Start health check background task
    fn start_health_check_task(&self) -> JoinHandle<()> {
        let connections = self.connections.clone();
        let node = self.node.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Check health of all connections
                        for entry in connections.iter() {
                            let peer_id = *entry.key();
                            let client = entry.value();
                            
                            // TODO: Implement actual health check
                            tracing::debug!("Health check for peer {}", peer_id);
                            
                            // For now, just update last used time
                            client.touch().await;
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            tracing::info!("Health check task shutting down");
                            break;
                        }
                    }
                }
            }
        })
    }
    
    /// Start buffer processing background task
    fn start_buffer_processing_task(&self) -> JoinHandle<()> {
        let buffer = self.message_buffer.clone();
        let connections = self.connections.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Process buffered messages
                        let mut buffer_lock = buffer.lock().await;
                        let mut expired_messages = Vec::new();
                        
                        for (peer_id, messages) in buffer_lock.iter_mut() {
                            messages.retain(|msg| {
                                // Remove messages older than 5 minutes
                                msg.timestamp.elapsed() < Duration::from_secs(300)
                            });
                            
                            if messages.is_empty() {
                                expired_messages.push(*peer_id);
                            }
                        }
                        
                        // Clean up empty queues
                        for peer_id in expired_messages {
                            buffer_lock.remove(&peer_id);
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            tracing::info!("Buffer processing task shutting down");
                            break;
                        }
                    }
                }
            }
        })
    }
    
    /// Start connection cleanup background task
    fn start_connection_cleanup_task(&self) -> JoinHandle<()> {
        let connections = self.connections.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Clean up idle connections
                        let mut to_remove = Vec::new();
                        
                        for entry in connections.iter() {
                            let peer_id = *entry.key();
                            let client = entry.value();
                            
                            // Check if connection is idle
                            let info = client.connection_info.read().await;
                            if info.last_used.elapsed() > Duration::from_secs(300) {
                                to_remove.push(peer_id);
                            }
                        }
                        
                        // Remove idle connections
                        for peer_id in to_remove {
                            connections.remove(&peer_id);
                            tracing::debug!("Removed idle connection to peer {}", peer_id);
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            tracing::info!("Connection cleanup task shutting down");
                            break;
                        }
                    }
                }
            }
        })
    }
    
    /// Shutdown the peer connector
    pub async fn shutdown(&self) -> BlixardResult<()> {
        // Signal shutdown
        let _ = self.shutdown_tx.send(true);
        
        // Wait for background tasks
        let mut tasks = self.background_tasks.lock().await;
        for task in tasks.drain(..) {
            let _ = task.await;
        }
        
        // Clear connections
        self.connections.clear();
        
        Ok(())
    }
}