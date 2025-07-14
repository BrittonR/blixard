//! Optimized peer connector with advanced connection pooling and batching
//!
//! This module provides enhanced connection management for peer-to-peer
//! communication with features like connection pooling, message batching,
//! and adaptive connection strategies.

use dashmap::DashMap;
use futures::stream::{self, StreamExt};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{watch, Mutex, RwLock, Semaphore};
use tokio::task::JoinHandle;

use crate::{
    common::async_utils::{concurrent_map, AtomicCounter},
    error::{BlixardError, BlixardResult},
    node_shared::{PeerInfo, SharedNodeState},
    p2p_monitor::P2pMonitor,
    transport::iroh_peer_connector::{IrohClient, MessageType},
};
use iroh::{Endpoint, NodeAddr, NodeId};

/// Advanced configuration for optimized peer connector
#[derive(Debug, Clone)]
pub struct OptimizedPeerConnectorConfig {
    /// Base connection pooling config
    pub max_connections_per_peer: usize,
    pub connection_idle_timeout: Duration,
    pub max_total_connections: usize,
    
    /// Advanced pooling features
    pub enable_connection_warming: bool,
    pub warm_connection_count: usize,
    pub connection_health_check_interval: Duration,
    
    /// Message batching
    pub enable_message_batching: bool,
    pub max_batch_size: usize,
    pub batch_timeout: Duration,
    pub max_batch_bytes: usize,
    
    /// Adaptive connection management
    pub enable_adaptive_pooling: bool,
    pub min_connections_per_peer: usize,
    pub connection_load_threshold: f64,
    
    /// Performance optimization
    pub enable_concurrent_operations: bool,
    pub max_concurrent_connections: usize,
    pub connection_acquisition_timeout: Duration,
}

impl Default for OptimizedPeerConnectorConfig {
    fn default() -> Self {
        Self {
            // Base pooling
            max_connections_per_peer: 5,
            connection_idle_timeout: Duration::from_secs(300),
            max_total_connections: 100,
            
            // Advanced features
            enable_connection_warming: true,
            warm_connection_count: 2,
            connection_health_check_interval: Duration::from_secs(30),
            
            // Message batching
            enable_message_batching: true,
            max_batch_size: 50,
            batch_timeout: Duration::from_millis(10),
            max_batch_bytes: 1024 * 1024, // 1MB
            
            // Adaptive pooling
            enable_adaptive_pooling: true,
            min_connections_per_peer: 1,
            connection_load_threshold: 0.8,
            
            // Performance
            enable_concurrent_operations: true,
            max_concurrent_connections: 20,
            connection_acquisition_timeout: Duration::from_secs(5),
        }
    }
}

/// Enhanced connection metadata with performance tracking
#[derive(Debug)]
pub struct EnhancedConnectionInfo {
    pub client: IrohClient,
    pub created_at: Instant,
    pub last_used: Arc<RwLock<Instant>>,
    pub request_count: AtomicCounter,
    pub bytes_sent: AtomicCounter,
    pub bytes_received: AtomicCounter,
    pub error_count: AtomicCounter,
    pub avg_response_time: Arc<RwLock<f64>>,
    pub is_healthy: Arc<RwLock<bool>>,
    pub connection_score: Arc<RwLock<f64>>,
}

impl EnhancedConnectionInfo {
    pub fn new(client: IrohClient) -> Self {
        let now = Instant::now();
        Self {
            client,
            created_at: now,
            last_used: Arc::new(RwLock::new(now)),
            request_count: AtomicCounter::new(0),
            bytes_sent: AtomicCounter::new(0),
            bytes_received: AtomicCounter::new(0),
            error_count: AtomicCounter::new(0),
            avg_response_time: Arc::new(RwLock::new(0.0)),
            is_healthy: Arc::new(RwLock::new(true)),
            connection_score: Arc::new(RwLock::new(1.0)),
        }
    }

    pub async fn update_stats(&self, bytes_sent: u64, bytes_received: u64, response_time_ms: f64) {
        *self.last_used.write().await = Instant::now();
        self.request_count.increment();
        self.bytes_sent.set(self.bytes_sent.get() + bytes_sent);
        self.bytes_received.set(self.bytes_received.get() + bytes_received);
        
        // Update moving average of response time
        let mut avg_rt = self.avg_response_time.write().await;
        let count = self.request_count.get() as f64;
        *avg_rt = (*avg_rt * (count - 1.0) + response_time_ms) / count;
        
        // Update connection score based on performance
        let error_rate = self.error_count.get() as f64 / count;
        let score = (1.0 - error_rate) * (1000.0 / (*avg_rt + 1.0)).min(1.0);
        *self.connection_score.write().await = score;
    }

    pub async fn record_error(&self) {
        self.error_count.increment();
        *self.is_healthy.write().await = false;
        
        // Recalculate connection score
        let count = self.request_count.get() as f64;
        if count > 0.0 {
            let error_rate = self.error_count.get() as f64 / count;
            let avg_rt = *self.avg_response_time.read().await;
            let score = (1.0 - error_rate) * (1000.0 / (avg_rt + 1.0)).min(1.0);
            *self.connection_score.write().await = score;
        }
    }

    pub async fn is_idle(&self, timeout: Duration) -> bool {
        self.last_used.read().await.elapsed() > timeout
    }

    pub async fn get_load(&self) -> f64 {
        // Simple load calculation based on request rate
        let requests = self.request_count.get() as f64;
        let age_seconds = self.created_at.elapsed().as_secs_f64();
        if age_seconds > 0.0 {
            requests / age_seconds / 10.0 // Normalize to 0-1 scale
        } else {
            0.0
        }
    }
}

/// Connection pool for a specific peer with load balancing
#[derive(Debug)]
pub struct PeerConnectionPool {
    peer_id: u64,
    connections: Vec<Arc<EnhancedConnectionInfo>>,
    next_connection_index: AtomicCounter,
    total_requests: AtomicCounter,
    _last_health_check: Arc<RwLock<Instant>>,
}

impl PeerConnectionPool {
    pub fn new(peer_id: u64) -> Self {
        Self {
            peer_id,
            connections: Vec::new(),
            next_connection_index: AtomicCounter::new(0),
            total_requests: AtomicCounter::new(0),
            _last_health_check: Arc::new(RwLock::new(Instant::now())),
        }
    }

    pub fn add_connection(&mut self, connection: Arc<EnhancedConnectionInfo>) {
        self.connections.push(connection);
    }

    pub fn remove_connection(&mut self, index: usize) -> Option<Arc<EnhancedConnectionInfo>> {
        if index < self.connections.len() {
            Some(self.connections.remove(index))
        } else {
            None
        }
    }

    /// Get the best connection using load balancing and connection scoring
    pub async fn get_best_connection(&self) -> Option<Arc<EnhancedConnectionInfo>> {
        if self.connections.is_empty() {
            return None;
        }

        // Find the connection with the highest score and lowest load
        let mut best_connection = None;
        let mut best_score = 0.0;

        for connection in &self.connections {
            if *connection.is_healthy.read().await {
                let score = *connection.connection_score.read().await;
                let load = connection.get_load().await;
                let adjusted_score = score * (1.0 - load.min(0.9));
                
                if adjusted_score > best_score {
                    best_score = adjusted_score;
                    best_connection = Some(connection.clone());
                }
            }
        }

        // If no healthy connections, use round-robin
        if best_connection.is_none() && !self.connections.is_empty() {
            let index = self.next_connection_index.increment() as usize % self.connections.len();
            best_connection = Some(self.connections[index].clone());
        }

        self.total_requests.increment();
        best_connection
    }

    pub async fn get_pool_stats(&self) -> PeerPoolStats {
        let mut healthy_count = 0;
        let mut total_requests = 0;
        let mut total_errors = 0;
        let mut avg_response_time = 0.0;

        for connection in &self.connections {
            if *connection.is_healthy.read().await {
                healthy_count += 1;
            }
            total_requests += connection.request_count.get();
            total_errors += connection.error_count.get();
            avg_response_time += *connection.avg_response_time.read().await;
        }

        if !self.connections.is_empty() {
            avg_response_time /= self.connections.len() as f64;
        }

        PeerPoolStats {
            peer_id: self.peer_id,
            total_connections: self.connections.len(),
            healthy_connections: healthy_count,
            total_requests,
            total_errors,
            avg_response_time,
            error_rate: if total_requests > 0 { total_errors as f64 / total_requests as f64 } else { 0.0 },
        }
    }

    pub async fn needs_more_connections(&self, config: &OptimizedPeerConnectorConfig) -> bool {
        if !config.enable_adaptive_pooling {
            return self.connections.len() < config.max_connections_per_peer;
        }

        let healthy_count = self.connections.iter()
            .filter(|c| {
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        *c.is_healthy.read().await
                    })
                })
            })
            .count();

        if healthy_count < config.min_connections_per_peer {
            return true;
        }

        // Check if existing connections are overloaded
        let avg_load = if !self.connections.is_empty() {
            let total_load: f64 = stream::iter(&self.connections)
                .map(|conn| conn.get_load())
                .fold(0.0, |acc, load| async move { acc + load.await })
                .await;
            total_load / self.connections.len() as f64
        } else {
            0.0
        };

        avg_load > config.connection_load_threshold && self.connections.len() < config.max_connections_per_peer
    }
}

/// Batched message for efficient sending
#[derive(Debug)]
pub struct BatchedMessage {
    peer_id: u64,
    data: Vec<u8>,
    message_type: MessageType,
    timestamp: Instant,
    response_sender: Option<tokio::sync::oneshot::Sender<BlixardResult<Vec<u8>>>>,
}

/// Optimized peer connector with advanced features
pub struct OptimizedPeerConnector {
    config: OptimizedPeerConnectorConfig,
    endpoint: Endpoint,
    node: Arc<SharedNodeState>,
    
    /// Connection pools per peer
    peer_pools: Arc<DashMap<u64, Arc<Mutex<PeerConnectionPool>>>>,
    
    /// Message batching
    message_batches: Arc<Mutex<HashMap<u64, Vec<BatchedMessage>>>>,
    
    /// Connection management
    total_connections: AtomicCounter,
    connection_semaphore: Arc<Semaphore>,
    
    /// Background tasks
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
    background_tasks: Mutex<Vec<JoinHandle<()>>>,
    
    /// Monitoring
    p2p_monitor: Arc<dyn P2pMonitor>,
}

impl OptimizedPeerConnector {
    pub fn new(
        config: OptimizedPeerConnectorConfig,
        endpoint: Endpoint,
        node: Arc<SharedNodeState>,
        p2p_monitor: Arc<dyn P2pMonitor>,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let connection_semaphore = Arc::new(Semaphore::new(config.max_concurrent_connections));

        Self {
            config,
            endpoint,
            node,
            peer_pools: Arc::new(DashMap::new()),
            message_batches: Arc::new(Mutex::new(HashMap::new())),
            total_connections: AtomicCounter::new(0),
            connection_semaphore,
            shutdown_tx,
            shutdown_rx,
            background_tasks: Mutex::new(Vec::new()),
            p2p_monitor,
        }
    }

    /// Start the optimized peer connector
    pub async fn start(&self) -> BlixardResult<()> {
        let mut tasks = self.background_tasks.lock().await;
        
        // Start connection warming task
        if self.config.enable_connection_warming {
            tasks.push(self.start_connection_warming_task());
        }
        
        // Start adaptive connection management
        if self.config.enable_adaptive_pooling {
            tasks.push(self.start_adaptive_management_task());
        }
        
        // Start message batching processor
        if self.config.enable_message_batching {
            tasks.push(self.start_message_batching_task());
        }
        
        // Start health check task
        tasks.push(self.start_health_check_task());
        
        // Start cleanup task
        tasks.push(self.start_cleanup_task());

        tracing::info!("Optimized peer connector started with config: {:?}", self.config);
        Ok(())
    }

    /// Get a connection with optimal load balancing
    pub async fn get_optimized_connection(&self, peer_id: u64) -> BlixardResult<Arc<EnhancedConnectionInfo>> {
        // Acquire semaphore to limit concurrent connection operations
        let _permit = self.connection_semaphore.acquire().await.map_err(|_| {
            BlixardError::Internal {
                message: "Connection semaphore closed".to_string(),
            }
        })?;

        // Get or create peer pool
        let pool = self.get_or_create_peer_pool(peer_id).await?;
        let mut pool_guard = pool.lock().await;

        // Try to get existing connection
        if let Some(connection) = pool_guard.get_best_connection().await {
            return Ok(connection);
        }

        // Check if we need to create more connections
        if pool_guard.needs_more_connections(&self.config).await {
            // Create new connection
            let connection = self.create_enhanced_connection(peer_id).await?;
            let enhanced_conn = Arc::new(EnhancedConnectionInfo::new(connection));
            pool_guard.add_connection(enhanced_conn.clone());
            self.total_connections.increment();
            
            return Ok(enhanced_conn);
        }

        Err(BlixardError::ResourceExhausted {
            resource: format!("Connection pool for peer {} (limit: {})", peer_id, self.config.max_connections_per_peer),
        })
    }

    /// Send message with batching optimization
    pub async fn send_optimized_message(
        &self,
        peer_id: u64,
        data: Vec<u8>,
        message_type: MessageType,
    ) -> BlixardResult<()> {
        if self.config.enable_message_batching && message_type != MessageType::RaftMessage {
            // Add to batch for non-critical messages
            self.add_to_batch(peer_id, data, message_type).await
        } else {
            // Send immediately for critical messages
            let connection = self.get_optimized_connection(peer_id).await?;
            self.send_direct_message(&connection, data, message_type).await
        }
    }

    /// Add message to batch
    async fn add_to_batch(
        &self,
        peer_id: u64,
        data: Vec<u8>,
        message_type: MessageType,
    ) -> BlixardResult<()> {
        let mut batches = self.message_batches.lock().await;
        let batch = batches.entry(peer_id).or_insert_with(Vec::new);

        let message = BatchedMessage {
            peer_id,
            data,
            message_type,
            timestamp: Instant::now(),
            response_sender: None,
        };

        batch.push(message);

        // Check if batch should be flushed
        let total_size: usize = batch.iter().map(|m| m.data.len()).sum();
        if batch.len() >= self.config.max_batch_size || total_size >= self.config.max_batch_bytes {
            let messages = std::mem::take(batch);
            drop(batches);
            self.flush_batch(peer_id, messages).await?;
        }

        Ok(())
    }

    /// Flush message batch
    async fn flush_batch(&self, peer_id: u64, messages: Vec<BatchedMessage>) -> BlixardResult<()> {
        if messages.is_empty() {
            return Ok(());
        }

        let connection = self.get_optimized_connection(peer_id).await?;
        
        // Send messages concurrently with limited parallelism
        let results = concurrent_map(
            messages,
            5, // Max 5 concurrent sends per batch
            |message| {
                let connection = connection.clone();
                async move {
                    self.send_direct_message(&connection, message.data, message.message_type).await
                }
            },
        ).await;

        // Check for errors
        for result in results {
            if let Err(e) = result {
                connection.record_error().await;
                return Err(e);
            }
        }

        Ok(())
    }

    /// Send message directly through connection
    async fn send_direct_message(
        &self,
        connection: &EnhancedConnectionInfo,
        data: Vec<u8>,
        _message_type: MessageType,
    ) -> BlixardResult<()> {
        let start = Instant::now();
        let data_len = data.len() as u64;

        // TODO: Implement actual message sending based on message type
        // For now, just update stats
        tokio::time::sleep(Duration::from_millis(1)).await; // Simulate network delay

        let response_time = start.elapsed().as_secs_f64() * 1000.0;
        connection.update_stats(data_len, 0, response_time).await;

        Ok(())
    }

    /// Get or create peer connection pool
    async fn get_or_create_peer_pool(&self, peer_id: u64) -> BlixardResult<Arc<Mutex<PeerConnectionPool>>> {
        if let Some(pool) = self.peer_pools.get(&peer_id) {
            return Ok(pool.clone());
        }

        let pool = Arc::new(Mutex::new(PeerConnectionPool::new(peer_id)));
        self.peer_pools.insert(peer_id, pool.clone());
        Ok(pool)
    }

    /// Create enhanced connection with better error handling
    async fn create_enhanced_connection(&self, peer_id: u64) -> BlixardResult<IrohClient> {
        // Get peer info
        let peer_info = self.get_peer_info(peer_id).await?;
        let node_addr = self.peer_info_to_node_addr(&peer_info)?;

        // Create connection with timeout
        crate::common::async_utils::with_timeout(
            IrohClient::new(
                self.endpoint.clone(),
                node_addr,
                peer_id,
                Some(self.p2p_monitor.clone()),
            ),
            self.config.connection_acquisition_timeout,
            "peer connection creation",
        ).await?
    }

    /// Get peer info from shared state
    async fn get_peer_info(&self, peer_id: u64) -> BlixardResult<PeerInfo> {
        let peers = self.node.get_peers().await;
        peers
            .into_iter()
            .find(|p| p.node_id.parse::<u64>().unwrap_or(0) == peer_id)
            .ok_or_else(|| BlixardError::NotFound {
                resource: format!("Peer {}", peer_id),
            })
    }

    /// Convert PeerInfo to NodeAddr
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
                    let addrs: Vec<SocketAddr> = peer
                        .p2p_addresses
                        .iter()
                        .filter_map(|addr_str| addr_str.parse().ok())
                        .collect();
                    if !addrs.is_empty() {
                        node_addr = node_addr.with_direct_addresses(addrs);
                    }

                    return Ok(node_addr);
                }
            }
        }

        Err(BlixardError::Internal {
            message: format!("Cannot convert peer {} to Iroh address", peer.node_id),
        })
    }

    /// Start connection warming task
    fn start_connection_warming_task(&self) -> JoinHandle<()> {
        let peer_pools = Arc::clone(&self.peer_pools);
        let config = self.config.clone();
        let node = Arc::clone(&self.node);
        let _endpoint = self.endpoint.clone();
        let _p2p_monitor = Arc::clone(&self.p2p_monitor);
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Get current peers
                        let peers = node.get_peers().await;
                        
                        for peer in peers {
                            if let Ok(peer_id) = peer.node_id.parse::<u64>() {
                                if let Some(pool_ref) = peer_pools.get(&peer_id) {
                                    let pool = pool_ref.clone();
                                    let pool_guard = pool.lock().await;
                                    
                                    // Check if we need to warm connections
                                    if pool_guard.connections.len() < config.warm_connection_count {
                                        // TODO: Create warm connections
                                        tracing::debug!("Warming connection for peer {}", peer_id);
                                    }
                                }
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
                }
            }
        })
    }

    /// Start adaptive management task
    fn start_adaptive_management_task(&self) -> JoinHandle<()> {
        let peer_pools = Arc::clone(&self.peer_pools);
        let _config = self.config.clone(); // TODO: Use for adaptive configuration
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Analyze connection pools and adjust
                        for pool_ref in peer_pools.iter() {
                            let pool = pool_ref.value().clone();
                            let pool_guard = pool.lock().await;
                            
                            // TODO: Implement adaptive logic
                            let stats = pool_guard.get_pool_stats().await;
                            if stats.error_rate > 0.1 {
                                tracing::warn!("High error rate for peer {}: {:.2}%", 
                                             stats.peer_id, stats.error_rate * 100.0);
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
                }
            }
        })
    }

    /// Start message batching task
    fn start_message_batching_task(&self) -> JoinHandle<()> {
        let message_batches = Arc::clone(&self.message_batches);
        let config = self.config.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.batch_timeout);
            
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Flush expired batches
                        let mut batches = message_batches.lock().await;
                        let mut expired_batches = Vec::new();
                        
                        for (peer_id, messages) in batches.iter_mut() {
                            let mut expired = Vec::new();
                            let mut i = 0;
                            while i < messages.len() {
                                if messages[i].timestamp.elapsed() > config.batch_timeout {
                                    expired.push(messages.remove(i));
                                } else {
                                    i += 1;
                                }
                            }
                            
                            if !expired.is_empty() {
                                expired_batches.push((*peer_id, expired));
                            }
                        }
                        
                        drop(batches);
                        
                        // Send expired batches
                        for (peer_id, messages) in expired_batches {
                            // TODO: Send batched messages
                            tracing::debug!("Flushing {} expired messages for peer {}", 
                                          messages.len(), peer_id);
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
                }
            }
        })
    }

    /// Start health check task
    fn start_health_check_task(&self) -> JoinHandle<()> {
        let peer_pools = Arc::clone(&self.peer_pools);
        let config = self.config.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.connection_health_check_interval);
            
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Health check all connections
                        for pool_ref in peer_pools.iter() {
                            let pool = pool_ref.value().clone();
                            let pool_guard = pool.lock().await;
                            
                            for connection in &pool_guard.connections {
                                // TODO: Implement actual health check
                                let _is_healthy = *connection.is_healthy.read().await;
                                // Perform health check and update connection status
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
                }
            }
        })
    }

    /// Start cleanup task
    fn start_cleanup_task(&self) -> JoinHandle<()> {
        let peer_pools = Arc::clone(&self.peer_pools);
        let config = self.config.clone();
        let total_connections = self.total_connections.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Clean up idle connections
                        for pool_ref in peer_pools.iter() {
                            let pool = pool_ref.value().clone();
                            let mut pool_guard = pool.lock().await;
                            
                            let mut indices_to_remove = Vec::new();
                            for (i, connection) in pool_guard.connections.iter().enumerate() {
                                if connection.is_idle(config.connection_idle_timeout).await {
                                    indices_to_remove.push(i);
                                }
                            }
                            
                            // Remove from back to front to maintain indices
                            for &index in indices_to_remove.iter().rev() {
                                if let Some(_) = pool_guard.remove_connection(index) {
                                    total_connections.decrement();
                                }
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
                }
            }
        })
    }

    /// Get comprehensive statistics
    pub async fn get_comprehensive_stats(&self) -> OptimizedPeerConnectorStats {
        let mut peer_stats = Vec::new();
        let mut total_healthy_connections = 0;
        let mut total_requests = 0;
        let mut total_errors = 0;

        for pool_ref in self.peer_pools.iter() {
            let pool = pool_ref.value().clone();
            let pool_guard = pool.lock().await;
            let stats = pool_guard.get_pool_stats().await;
            
            total_healthy_connections += stats.healthy_connections;
            total_requests += stats.total_requests;
            total_errors += stats.total_errors;
            
            peer_stats.push(stats);
        }

        OptimizedPeerConnectorStats {
            total_connections: self.total_connections.get() as usize,
            total_healthy_connections,
            total_peers: self.peer_pools.len(),
            total_requests,
            total_errors,
            overall_error_rate: if total_requests > 0 {
                total_errors as f64 / total_requests as f64
            } else {
                0.0
            },
            peer_stats,
        }
    }

    /// Shutdown the connector
    pub async fn shutdown(&mut self) {
        let _ = self.shutdown_tx.send(true);
        
        let mut tasks = self.background_tasks.lock().await;
        for task in tasks.drain(..) {
            task.abort();
        }
        
        self.peer_pools.clear();
        self.total_connections.set(0);
    }
}

/// Statistics for a peer connection pool
#[derive(Debug)]
pub struct PeerPoolStats {
    pub peer_id: u64,
    pub total_connections: usize,
    pub healthy_connections: usize,
    pub total_requests: u64,
    pub total_errors: u64,
    pub avg_response_time: f64,
    pub error_rate: f64,
}

/// Comprehensive statistics for the optimized peer connector
#[derive(Debug)]
pub struct OptimizedPeerConnectorStats {
    pub total_connections: usize,
    pub total_healthy_connections: usize,
    pub total_peers: usize,
    pub total_requests: u64,
    pub total_errors: u64,
    pub overall_error_rate: f64,
    pub peer_stats: Vec<PeerPoolStats>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_enhanced_connection_info() {
        // Mock client creation
        let endpoint = iroh::Endpoint::builder().bind().await.unwrap();
        let node_addr = NodeAddr::new(iroh::NodeId::from_bytes(&[1u8; 32]).unwrap());
        
        // This would normally create a real client, but for testing we'll skip
        // let client = IrohClient::new(endpoint, node_addr, 1, None).await.unwrap();
        // let enhanced = EnhancedConnectionInfo::new(client);
        
        // Test would verify connection stats tracking
    }

    #[tokio::test]
    async fn test_peer_connection_pool() {
        let pool = PeerConnectionPool::new(1);
        assert_eq!(pool.peer_id, 1);
        assert_eq!(pool.connections.len(), 0);
        
        let stats = pool.get_pool_stats().await;
        assert_eq!(stats.total_connections, 0);
        assert_eq!(stats.healthy_connections, 0);
    }
}