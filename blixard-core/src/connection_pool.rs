//! Connection pooling for gRPC clients
//!
//! This module provides a connection pool that manages multiple gRPC channels
//! per peer, allowing for better throughput and connection reuse.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};
use tonic::transport::Channel;
use crate::error::{BlixardError, BlixardResult};
use crate::proto::cluster_service_client::ClusterServiceClient;
use crate::config_v2::ConnectionPoolConfig;
use tracing::{debug, info};

/// A pooled connection with metadata
#[derive(Clone)]
struct PooledConnection {
    /// The gRPC client
    client: ClusterServiceClient<Channel>,
    /// When this connection was created
    created_at: Instant,
    /// Last time this connection was used
    last_used: Instant,
    /// Number of times this connection has been used
    use_count: u64,
    /// Whether this connection is currently in use
    in_use: bool,
}

impl PooledConnection {
    fn new(client: ClusterServiceClient<Channel>) -> Self {
        let now = Instant::now();
        Self {
            client,
            created_at: now,
            last_used: now,
            use_count: 0,
            in_use: false,
        }
    }
    
    /// Check if this connection should be evicted
    fn should_evict(&self, max_idle_time: Duration, max_lifetime: Duration) -> bool {
        let now = Instant::now();
        
        // Check if connection is too old
        if now.duration_since(self.created_at) > max_lifetime {
            return true;
        }
        
        // Check if connection has been idle too long
        if !self.in_use && now.duration_since(self.last_used) > max_idle_time {
            return true;
        }
        
        false
    }
}

/// Connection pool for managing multiple gRPC connections per peer
pub struct ConnectionPool {
    /// Configuration
    config: ConnectionPoolConfig,
    
    /// Connections per peer
    connections: Arc<RwLock<HashMap<u64, Vec<PooledConnection>>>>,
    
    /// Semaphore to limit total connections
    connection_limit: Arc<Semaphore>,
    
    /// Function to create new channels
    channel_factory: Arc<dyn Fn(&str) -> BoxFuture<'static, BlixardResult<Channel>> + Send + Sync>,
}

use futures::future::BoxFuture;

impl ConnectionPool {
    /// Create a new connection pool
    pub fn new(
        config: ConnectionPoolConfig,
        channel_factory: impl Fn(&str) -> BoxFuture<'static, BlixardResult<Channel>> + Send + Sync + 'static,
    ) -> Self {
        let total_limit = config.max_connections_per_peer * config.max_peers;
        
        Self {
            config,
            connections: Arc::new(RwLock::new(HashMap::new())),
            connection_limit: Arc::new(Semaphore::new(total_limit)),
            channel_factory: Arc::new(channel_factory),
        }
    }
    
    /// Get a connection to a peer
    pub async fn get_connection(
        &self,
        peer_id: u64,
        endpoint: &str,
    ) -> BlixardResult<ClusterServiceClient<Channel>> {
        // First, try to get an existing connection
        {
            let mut connections = self.connections.write().await;
            if let Some(peer_connections) = connections.get_mut(&peer_id) {
                // Find an available connection
                for conn in peer_connections.iter_mut() {
                    if !conn.in_use && !conn.should_evict(
                        Duration::from_secs(self.config.idle_timeout_secs),
                        Duration::from_secs(self.config.max_lifetime_secs),
                    ) {
                        conn.in_use = true;
                        conn.use_count += 1;
                        conn.last_used = Instant::now();
                        
                        debug!(
                            "Reusing connection to peer {} (use_count: {}, age: {:?})",
                            peer_id,
                            conn.use_count,
                            conn.created_at.elapsed()
                        );
                        
                        // Record connection reuse
                        crate::metrics_otel::record_connection_pool_event(
                            crate::metrics_otel::ConnectionPoolEvent::Reused
                        );
                        
                        return Ok(conn.client.clone());
                    }
                }
                
                // Check if we can create a new connection for this peer
                if peer_connections.len() < self.config.max_connections_per_peer {
                    // Continue to create new connection below
                } else {
                    // All connections are in use, wait for one to be available
                    // For now, we'll just fail fast
                    return Err(BlixardError::Connection {
                        message: format!(
                            "All {} connections to peer {} are in use",
                            self.config.max_connections_per_peer, peer_id
                        ),
                    });
                }
            }
        }
        
        // Try to acquire a permit for a new connection
        let _permit = self.connection_limit
            .try_acquire()
            .map_err(|_| BlixardError::Connection {
                message: "Connection pool limit reached".to_string(),
            })?;
        
        // Create a new connection
        info!("Creating new connection to peer {} at {}", peer_id, endpoint);
        
        let channel = (self.channel_factory)(endpoint).await?;
        let client = ClusterServiceClient::new(channel);
        
        // Add to pool
        {
            let mut connections = self.connections.write().await;
            let peer_connections = connections.entry(peer_id).or_insert_with(Vec::new);
            
            let mut pooled = PooledConnection::new(client.clone());
            pooled.in_use = true;
            pooled.use_count = 1;
            
            peer_connections.push(pooled);
        }
        
        // Record connection creation
        crate::metrics_otel::record_connection_pool_event(
            crate::metrics_otel::ConnectionPoolEvent::Created
        );
        
        Ok(client)
    }
    
    /// Return a connection to the pool
    pub async fn return_connection(&self, peer_id: u64, _client: ClusterServiceClient<Channel>) {
        let mut connections = self.connections.write().await;
        
        if let Some(peer_connections) = connections.get_mut(&peer_id) {
            // Find the first in-use connection and mark it as not in use
            // Note: In a real implementation, we'd need a better way to identify connections
            for conn in peer_connections.iter_mut() {
                if conn.in_use {
                    conn.in_use = false;
                    conn.last_used = Instant::now();
                    
                    debug!(
                        "Returned connection to peer {} (use_count: {})",
                        peer_id, conn.use_count
                    );
                    
                    break;
                }
            }
        }
    }
    
    /// Remove a connection from the pool (e.g., due to errors)
    pub async fn remove_connection(&self, peer_id: u64, _client: ClusterServiceClient<Channel>) {
        let mut connections = self.connections.write().await;
        
        if let Some(peer_connections) = connections.get_mut(&peer_id) {
            // Remove the first in-use connection
            // Note: In a real implementation, we'd need a better way to identify connections
            let before_len = peer_connections.len();
            peer_connections.retain(|conn| !conn.in_use);
            
            let removed = before_len - peer_connections.len();
            if removed > 0 {
                debug!(
                    "Removed {} connection(s) to peer {} ({} remaining)",
                    removed,
                    peer_id,
                    peer_connections.len()
                );
            }
            
            // Clean up empty entries
            if peer_connections.is_empty() {
                connections.remove(&peer_id);
            }
        }
    }
    
    /// Clean up idle and expired connections
    pub async fn cleanup_connections(&self) {
        let mut connections = self.connections.write().await;
        let idle_timeout = Duration::from_secs(self.config.idle_timeout_secs);
        let max_lifetime = Duration::from_secs(self.config.max_lifetime_secs);
        
        let mut total_removed = 0;
        
        connections.retain(|peer_id, peer_connections| {
            let before_count = peer_connections.len();
            
            peer_connections.retain(|conn| {
                !conn.should_evict(idle_timeout, max_lifetime)
            });
            
            let removed = before_count - peer_connections.len();
            if removed > 0 {
                debug!(
                    "Cleaned up {} idle/expired connections to peer {}",
                    removed, peer_id
                );
                total_removed += removed;
                
                // Record eviction metrics
                for _ in 0..removed {
                    crate::metrics_otel::record_connection_pool_event(
                        crate::metrics_otel::ConnectionPoolEvent::Evicted
                    );
                }
            }
            
            !peer_connections.is_empty()
        });
        
        if total_removed > 0 {
            info!("Cleaned up {} total connections", total_removed);
        }
    }
    
    /// Get statistics about the connection pool
    pub async fn get_stats(&self) -> ConnectionPoolStats {
        let connections = self.connections.read().await;
        
        let mut total_connections = 0;
        let mut active_connections = 0;
        let mut peer_count = 0;
        
        for (_peer_id, peer_connections) in connections.iter() {
            peer_count += 1;
            total_connections += peer_connections.len();
            active_connections += peer_connections.iter().filter(|c| c.in_use).count();
        }
        
        ConnectionPoolStats {
            total_connections,
            active_connections,
            idle_connections: total_connections - active_connections,
            peer_count,
            max_connections: self.config.max_connections_per_peer * self.config.max_peers,
        }
    }
}

/// Statistics about the connection pool
#[derive(Debug, Clone)]
pub struct ConnectionPoolStats {
    /// Total number of connections in the pool
    pub total_connections: usize,
    /// Number of connections currently in use
    pub active_connections: usize,
    /// Number of idle connections
    pub idle_connections: usize,
    /// Number of unique peers
    pub peer_count: usize,
    /// Maximum allowed connections
    pub max_connections: usize,
}

/// A guard that returns the connection to the pool when dropped
pub struct PooledConnectionGuard {
    peer_id: u64,
    client: Option<ClusterServiceClient<Channel>>,
    pool: Arc<ConnectionPool>,
}

impl PooledConnectionGuard {
    pub fn new(
        peer_id: u64,
        client: ClusterServiceClient<Channel>,
        pool: Arc<ConnectionPool>,
    ) -> Self {
        Self {
            peer_id,
            client: Some(client),
            pool,
        }
    }
    
    /// Get the client
    pub fn client(&mut self) -> &mut ClusterServiceClient<Channel> {
        self.client.as_mut().expect("Client already taken")
    }
}

impl Drop for PooledConnectionGuard {
    fn drop(&mut self) {
        if let Some(client) = self.client.take() {
            let pool = self.pool.clone();
            let peer_id = self.peer_id;
            
            // Return connection asynchronously
            tokio::spawn(async move {
                pool.return_connection(peer_id, client).await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_config() -> ConnectionPoolConfig {
        ConnectionPoolConfig {
            max_connections_per_peer: 3,
            max_peers: 10,
            idle_timeout_secs: 300,
            max_lifetime_secs: 3600,
            cleanup_interval_secs: 60,
        }
    }
    
    async fn create_mock_channel(_endpoint: &str) -> BlixardResult<Channel> {
        // In real tests, this would create a mock channel
        Err(BlixardError::NotImplemented {
            feature: "Mock channel creation".to_string(),
        })
    }
    
    #[tokio::test]
    async fn test_connection_pool_creation() {
        let config = create_test_config();
        let pool = ConnectionPool::new(config, |endpoint| {
            let endpoint = endpoint.to_string();
            Box::pin(async move {
                create_mock_channel(&endpoint).await
            })
        });
        
        let stats = pool.get_stats().await;
        assert_eq!(stats.total_connections, 0);
        assert_eq!(stats.active_connections, 0);
        assert_eq!(stats.peer_count, 0);
    }
}