//! Peer connection management for cluster nodes
//!
//! This module handles establishing and maintaining gRPC connections to peer nodes.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Mutex, watch};
use tokio::task::JoinHandle;
use tonic::transport::Channel;

use crate::error::{BlixardError, BlixardResult};
use crate::proto::cluster_service_client::ClusterServiceClient;
use crate::proto::{HealthCheckRequest};
use crate::node_shared::{SharedNodeState, PeerInfo};
use crate::config_global;
use crate::metrics_otel::{metrics, Timer, attributes};
use crate::tracing_otel;

/// Buffered message for delayed sending
struct BufferedMessage {
    _to: u64,
    message: raft::prelude::Message,
    _attempts: u32,
}

/// Circuit breaker states
#[derive(Debug, Clone, PartialEq)]
enum CircuitState {
    Closed,     // Normal operation
    Open,       // Failing, reject all attempts
    HalfOpen,   // Testing if service has recovered
}

/// Connection state with backoff and circuit breaker
#[derive(Debug, Clone)]
struct ConnectionState {
    last_attempt: Option<Instant>,
    attempt_count: u32,
    backoff_ms: u64,
    circuit_state: CircuitState,
    consecutive_failures: u32,
    last_success: Option<Instant>,
    circuit_opened_at: Option<Instant>,
}

impl ConnectionState {
    fn new() -> Self {
        Self {
            last_attempt: None,
            attempt_count: 0,
            backoff_ms: 100, // Start with 100ms
            circuit_state: CircuitState::Closed,
            consecutive_failures: 0,
            last_success: None,
            circuit_opened_at: None,
        }
    }

    fn should_retry(&self) -> bool {
        match self.circuit_state {
            CircuitState::Open => {
                // Check if we should transition to half-open
                if let Some(opened_at) = self.circuit_opened_at {
                    // After 30 seconds, try half-open
                    if opened_at.elapsed() >= Duration::from_secs(30) {
                        return true;
                    }
                }
                false
            }
            CircuitState::HalfOpen => {
                // Allow one attempt in half-open state
                true
            }
            CircuitState::Closed => {
                // Normal backoff logic
                match self.last_attempt {
                    None => true,
                    Some(last) => {
                        let elapsed = last.elapsed();
                        elapsed >= Duration::from_millis(self.backoff_ms)
                    }
                }
            }
        }
    }

    fn record_failure(&mut self) {
        self.last_attempt = Some(Instant::now());
        self.attempt_count += 1;
        self.consecutive_failures += 1;
        
        // Check if we should open the circuit
        let failure_threshold = config_global::get().cluster.peer.failure_threshold;
        if self.consecutive_failures >= failure_threshold {
            if self.circuit_state != CircuitState::Open {
                tracing::warn!("Opening circuit breaker after {} consecutive failures", 
                    self.consecutive_failures);
                self.circuit_state = CircuitState::Open;
                self.circuit_opened_at = Some(Instant::now());
            }
        }
        
        // Exponential backoff: double the delay up to 30 seconds
        self.backoff_ms = (self.backoff_ms * 2).min(30_000);
    }

    fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.attempt_count = 0;
        self.backoff_ms = 100;
        self.last_attempt = None;
        self.last_success = Some(Instant::now());
        
        if self.circuit_state == CircuitState::HalfOpen {
            tracing::info!("Circuit breaker transitioning from half-open to closed");
        }
        self.circuit_state = CircuitState::Closed;
        self.circuit_opened_at = None;
    }

    fn transition_to_half_open(&mut self) {
        if self.circuit_state == CircuitState::Open {
            tracing::info!("Circuit breaker transitioning from open to half-open");
            self.circuit_state = CircuitState::HalfOpen;
        }
    }
}

/// Manages connections to peer nodes
pub struct PeerConnector {
    node: Arc<SharedNodeState>,
    connections: RwLock<HashMap<u64, ClusterServiceClient<Channel>>>,
    /// Messages buffered while waiting for connection
    message_buffer: Mutex<HashMap<u64, VecDeque<BufferedMessage>>>,
    /// Track connection attempts in progress
    connecting: Mutex<HashMap<u64, bool>>,
    /// Connection state with backoff tracking
    connection_states: RwLock<HashMap<u64, ConnectionState>>,
    /// Track total connection count for pool limiting
    connection_count: Arc<Mutex<usize>>,
    /// Shutdown signal for background tasks
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
    /// JoinHandles for background tasks
    background_tasks: Mutex<Vec<JoinHandle<()>>>,
}

impl PeerConnector {
    pub fn new(node: Arc<SharedNodeState>) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            node,
            connections: RwLock::new(HashMap::new()),
            message_buffer: Mutex::new(HashMap::new()),
            connecting: Mutex::new(HashMap::new()),
            connection_states: RwLock::new(HashMap::new()),
            connection_count: Arc::new(Mutex::new(0)),
            shutdown_tx,
            shutdown_rx,
            background_tasks: Mutex::new(Vec::new()),
        }
    }
    
    /// Establish connection to a peer
    #[tracing::instrument(level = "debug", skip(self), fields(peer_id = %peer.id))]
    pub async fn connect_to_peer(&self, peer: &PeerInfo) -> BlixardResult<()> {
        let metrics = metrics();
        let peer_attrs = vec![attributes::peer_id(peer.id)];
        let _timer = Timer::with_attributes(
            metrics.grpc_request_duration.clone(),
            peer_attrs.clone()
        );
        metrics.peer_reconnect_attempts.add(1, &peer_attrs);
        
        // Check if we already have a connection
        {
            let connections = self.connections.read().await;
            if connections.contains_key(&peer.id) {
                tracing::debug!("Already connected to peer {}", peer.id);
                return Ok(());
            }
        }
        
        // Check connection pool limit
        {
            let count = self.connection_count.lock().await;
            let max_connections = config_global::get().cluster.peer.max_connections;
            if *count >= max_connections {
                tracing::warn!("Connection pool limit reached ({}/{}), cannot connect to peer {}", 
                    count, max_connections, peer.id);
                return Err(BlixardError::ClusterError(
                    format!("Connection pool limit reached ({})", max_connections)
                ));
            }
        }
        
        // Check circuit breaker and backoff before attempting
        {
            let mut states = self.connection_states.write().await;
            let state = states.entry(peer.id).or_insert_with(ConnectionState::new);
            
            if !state.should_retry() {
                match state.circuit_state {
                    CircuitState::Open => {
                        tracing::debug!("Circuit breaker OPEN for peer {} (failures: {})", 
                            peer.id, state.consecutive_failures);
                        return Err(BlixardError::ClusterError(
                            format!("Circuit breaker open for peer {}", peer.id)
                        ));
                    }
                    _ => {
                        tracing::debug!("Skipping connection to peer {} due to backoff (attempt #{}, wait {}ms)", 
                            peer.id, state.attempt_count, state.backoff_ms);
                        return Err(BlixardError::ClusterError(
                            format!("Connection to peer {} in backoff period", peer.id)
                        ));
                    }
                }
            }
            
            // Transition to half-open if appropriate
            if state.circuit_state == CircuitState::Open && state.should_retry() {
                state.transition_to_half_open();
            }
        }

        // Mark that we're connecting to prevent duplicate attempts
        {
            let mut connecting = self.connecting.lock().await;
            if connecting.get(&peer.id).copied().unwrap_or(false) {
                tracing::debug!("Already connecting to peer {}", peer.id);
                return Ok(());
            }
            connecting.insert(peer.id, true);
        }
        
        let endpoint = format!("http://{}", peer.address);
        
        let result = match Channel::from_shared(endpoint.clone()) {
            Ok(endpoint) => {
                match endpoint.connect().await {
                    Ok(channel) => {
                        let client = ClusterServiceClient::new(channel);
                        
                        // Store the connection
                        let mut connections = self.connections.write().await;
                        connections.insert(peer.id, client.clone());
                        
                        // Increment connection count
                        {
                            let mut count = self.connection_count.lock().await;
                            *count += 1;
                            tracing::debug!("[PEER-CONN] Connection count: {}/{}", count, config_global::get().cluster.peer.max_connections);
                        }
                        
                        // Update peer connection status
                        self.node.update_peer_connection(peer.id, true).await?;
                        
                        // Update metrics
                        let active_connections = *self.connection_count.lock().await as i64;
                        metrics.peer_connections_active.add(active_connections, &[]);
                        
                        tracing::info!("Connected to peer {} at {}", peer.id, peer.address);
                        
                        // Record successful connection
                        {
                            let mut states = self.connection_states.write().await;
                            if let Some(state) = states.get_mut(&peer.id) {
                                state.record_success();
                            }
                        }
                        
                        // Send any buffered messages
                        self.send_buffered_messages(peer.id, client).await;
                        
                        Ok(())
                    }
                    Err(e) => {
                        tracing::warn!("Failed to connect to peer {} at {}: {}", peer.id, peer.address, e);
                        self.node.update_peer_connection(peer.id, false).await?;
                        
                        // Record failed attempt
                        {
                            let mut states = self.connection_states.write().await;
                            if let Some(state) = states.get_mut(&peer.id) {
                                state.record_failure();
                            }
                        }
                        
                        Err(BlixardError::ClusterError(format!(
                            "Failed to connect to peer {}: {}", peer.id, e
                        )))
                    }
                }
            }
            Err(e) => {
                // Record failed attempt
                {
                    let mut states = self.connection_states.write().await;
                    if let Some(state) = states.get_mut(&peer.id) {
                        state.record_failure();
                    }
                }
                
                Err(BlixardError::ClusterError(format!(
                    "Invalid peer address {}: {}", peer.address, e
                )))
            }
        };
        
        // Clear connecting flag
        {
            let mut connecting = self.connecting.lock().await;
            connecting.remove(&peer.id);
        }
        
        result
    }
    
    /// Disconnect from a peer
    pub async fn disconnect_from_peer(&self, peer_id: u64) -> BlixardResult<()> {
        let mut connections = self.connections.write().await;
        if connections.remove(&peer_id).is_some() {
            // Decrement connection count
            let mut count = self.connection_count.lock().await;
            if *count > 0 {
                *count -= 1;
                tracing::debug!("[PEER-CONN] Connection count: {}/{}", count, config_global::get().cluster.peer.max_connections);
            }
        }
        
        self.node.update_peer_connection(peer_id, false).await?;
        
        tracing::info!("Disconnected from peer {}", peer_id);
        Ok(())
    }
    
    /// Get connection to a peer
    pub async fn get_connection(&self, peer_id: u64) -> Option<ClusterServiceClient<Channel>> {
        let connections = self.connections.read().await;
        connections.get(&peer_id).cloned()
    }
    
    /// Send buffered messages after connection is established
    async fn send_buffered_messages(&self, peer_id: u64, mut client: ClusterServiceClient<Channel>) {
        use crate::proto::RaftMessageRequest;
        
        let mut buffer = self.message_buffer.lock().await;
        if let Some(messages) = buffer.remove(&peer_id) {
            tracing::info!("[PEER-CONN] Sending {} buffered messages to node {}", messages.len(), peer_id);
            
            for buffered in messages {
                match crate::raft_codec::serialize_message(&buffered.message) {
                    Ok(raft_data) => {
                        let mut request = tonic::Request::new(RaftMessageRequest { raft_data });
                        tracing_otel::inject_context(&mut request);
                        
                        if let Err(e) = client.send_raft_message(request).await {
                            tracing::warn!("[PEER-CONN] Failed to send buffered message to {}: {}", peer_id, e);
                        } else {
                            tracing::debug!("[PEER-CONN] Successfully sent buffered message to {}, type: {:?}", 
                                peer_id, buffered.message.msg_type());
                        }
                    }
                    Err(e) => {
                        tracing::error!("[PEER-CONN] Failed to serialize buffered message: {}", e);
                    }
                }
            }
        }
    }
    
    /// Send Raft message to a peer
    pub async fn send_raft_message(self: &Arc<Self>, to: u64, message: raft::prelude::Message) -> BlixardResult<()> {
        use crate::proto::RaftMessageRequest;
        
        tracing::info!("[PEER-CONN] Sending Raft message to node {}, type: {:?}", to, message.msg_type());
        
        // Check if we have a connection
        if let Some(mut client) = self.get_connection(to).await {
            // Serialize the message
            let raft_data = crate::raft_codec::serialize_message(&message)?;
            let mut request = tonic::Request::new(RaftMessageRequest { raft_data });
            tracing_otel::inject_context(&mut request);
            
            match client.send_raft_message(request).await {
                Ok(response) => {
                    let resp = response.into_inner();
                    if resp.success {
                        Ok(())
                    } else {
                        Err(BlixardError::ClusterError(format!(
                            "Failed to send Raft message to {}: {}", to, resp.error
                        )))
                    }
                }
                Err(e) => {
                    // Connection failed, disconnect and buffer the message
                    {
                        let mut connections = self.connections.write().await;
                        if connections.remove(&to).is_some() {
                            // Decrement connection count
                            let mut count = self.connection_count.lock().await;
                            if *count > 0 {
                                *count -= 1;
                            }
                        }
                    }
                    
                    self.node.update_peer_connection(to, false).await?;
                    self.buffer_message(to, message).await;
                    
                    // Try to reconnect asynchronously
                    if let Some(peer) = self.node.get_peer(to).await {
                        let self_clone = self.clone();
                        let peer_clone = peer.clone();
                        tokio::spawn(async move {
                            let _ = self_clone.connect_to_peer(&peer_clone).await;
                        });
                    }
                    
                    Err(BlixardError::ClusterError(format!(
                        "Failed to send Raft message to {}: {}", to, e
                    )))
                }
            }
        } else {
            // No connection, buffer the message
            tracing::info!("[PEER-CONN] No connection to node {}, buffering message", to);
            self.buffer_message(to, message).await;
            
            // Try to connect asynchronously
            if let Some(peer) = self.node.get_peer(to).await {
                tracing::info!("[PEER-CONN] Found peer info for node {}: {}, attempting connection", to, peer.address);
                
                // Clone self for async task
                let peer_connector = self.clone();
                let peer_info = peer.clone();
                
                // Spawn connection attempt
                tokio::spawn(async move {
                    if let Err(e) = peer_connector.connect_to_peer(&peer_info).await {
                        tracing::warn!("[PEER-CONN] Failed to connect to peer {}: {}", peer_info.id, e);
                    }
                });
                
                Ok(())  // Message is buffered, will be sent when connection is established
            } else {
                Err(BlixardError::ClusterError(format!(
                    "Unknown peer {}", to
                )))
            }
        }
    }
    
    /// Buffer a message for later delivery
    async fn buffer_message(&self, to: u64, message: raft::prelude::Message) {
        let mut buffer = self.message_buffer.lock().await;
        let queue = buffer.entry(to).or_insert_with(VecDeque::new);
        
        // Limit buffer size to prevent memory issues
        let max_buffered_messages = config_global::get().cluster.peer.max_buffered_messages;
        if queue.len() >= max_buffered_messages {
            tracing::warn!("[PEER-CONN] Message buffer full for node {}, dropping oldest message", to);
            queue.pop_front();
        }
        
        queue.push_back(BufferedMessage {
            _to: to,
            message,
            _attempts: 0,
        });
        
        tracing::debug!("[PEER-CONN] Buffered message for node {}, buffer size: {}", to, queue.len());
    }
    
    /// Check health of a specific connection
    pub async fn check_connection_health(&self, peer_id: u64) -> bool {
        if let Some(mut client) = self.get_connection(peer_id).await {
            // Send health check request
            let mut request = tonic::Request::new(HealthCheckRequest {});
            tracing_otel::inject_context(&mut request);
            match client.health_check(request).await {
                Ok(response) => {
                    let resp = response.into_inner();
                    if resp.healthy {
                        true
                    } else {
                        tracing::warn!("[PEER-CONN] Peer {} reported unhealthy: {}", peer_id, resp.message);
                        false
                    }
                }
                Err(e) => {
                    tracing::warn!("[PEER-CONN] Health check failed for peer {}: {}", peer_id, e);
                    false
                }
            }
        } else {
            false
        }
    }
    
    /// Start background task to maintain connections
    pub async fn start_connection_maintenance(self: Arc<Self>) {
        // Connection establishment task
        let self_clone = self.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();
        let connection_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            
            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            tracing::info!("[PEER-CONN] Connection maintenance task shutting down");
                            break;
                        }
                    }
                    _ = interval.tick() => {
                        // Get all peers
                        let peers = self_clone.node.get_peers().await;
                        
                        // Check each peer connection
                        for peer in peers {
                            if !peer.is_connected {
                                // Try to connect
                                let _ = self_clone.connect_to_peer(&peer).await;
                            }
                        }
                    }
                }
            }
        });
        
        // Health check task
        let self_clone = self.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();
        let health_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            
            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            tracing::info!("[PEER-CONN] Health check task shutting down");
                            break;
                        }
                    }
                    _ = interval.tick() => {
                        // Get all connected peers
                        let peers = self_clone.node.get_peers().await;
                        
                        for peer in peers {
                            if peer.is_connected {
                                // Check connection health
                                if !self_clone.check_connection_health(peer.id).await {
                                    // Connection is unhealthy, disconnect and let maintenance reconnect
                                    tracing::info!("[PEER-CONN] Disconnecting unhealthy peer {}", peer.id);
                                    let _ = self_clone.disconnect_from_peer(peer.id).await;
                                }
                            }
                        }
                    }
                }
            }
        });
        
        // Store task handles
        let mut tasks = self.background_tasks.lock().await;
        tasks.push(connection_task);
        tasks.push(health_task);
    }
    
    /// Shutdown the peer connector and all background tasks
    pub async fn shutdown(&self) {
        tracing::info!("[PEER-CONN] Shutting down peer connector");
        
        // Signal all tasks to stop
        let _ = self.shutdown_tx.send(true);
        
        // Wait for all background tasks to complete
        let mut tasks = self.background_tasks.lock().await;
        for task in tasks.drain(..) {
            // Ignore errors from task abortion
            let _ = task.await;
        }
        
        // Disconnect all peers
        let connections = self.connections.read().await;
        let peer_ids: Vec<u64> = connections.keys().copied().collect();
        drop(connections);
        
        for peer_id in peer_ids {
            let _ = self.disconnect_from_peer(peer_id).await;
        }
        
        tracing::info!("[PEER-CONN] Peer connector shutdown complete");
    }
}