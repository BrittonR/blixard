//! Optimized message routing with minimal allocations
//!
//! This module provides optimized message routing with:
//! - Zero-copy message forwarding where possible
//! - Pre-allocated message buffers for common message types
//! - Reference-based message passing
//! - Efficient connection pooling

use crate::error::{BlixardError, BlixardResult};
use bytes::{Bytes, BytesMut};
use iroh::{Endpoint, NodeAddr, NodeId};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, trace};

/// Message envelope that avoids cloning data
#[derive(Debug)]
pub struct MessageEnvelope<'a> {
    /// Target node ID
    pub target: u64,
    /// Service name (borrowed to avoid allocation)
    pub service: &'a str,
    /// Method name (borrowed to avoid allocation)
    pub method: &'a str,
    /// Message payload (shared reference)
    pub payload: &'a [u8],
    /// Priority for routing
    pub priority: MessagePriority,
    /// Request ID for correlation
    pub request_id: &'a [u8; 16],
}

/// Message priority for routing optimization
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessagePriority {
    /// Critical system messages (elections, heartbeats)
    Critical = 0,
    /// Important user operations
    High = 1,
    /// Normal operations
    Normal = 2,
    /// Background operations
    Low = 3,
}

impl MessagePriority {
    /// Determine priority from service and method
    pub fn from_service_method(service: &str, method: &str) -> Self {
        match (service, method) {
            // Raft messages are critical
            ("raft", _) => Self::Critical,
            // Health checks are high priority
            ("health", _) => Self::High,
            // VM operations are normal priority
            ("vm", _) => Self::Normal,
            // Status queries are normal priority
            ("status", _) => Self::Normal,
            // Everything else is low priority
            _ => Self::Low,
        }
    }
}

/// Pre-allocated message buffer pool
pub struct MessageBufferPool {
    /// Small buffers (< 1KB) for headers and simple messages
    small_buffers: mpsc::UnboundedSender<BytesMut>,
    small_receiver: Arc<RwLock<mpsc::UnboundedReceiver<BytesMut>>>,
    
    /// Medium buffers (1KB - 64KB) for typical messages
    medium_buffers: mpsc::UnboundedSender<BytesMut>,
    medium_receiver: Arc<RwLock<mpsc::UnboundedReceiver<BytesMut>>>,
    
    /// Large buffers (64KB+) for bulk transfers
    large_buffers: mpsc::UnboundedSender<BytesMut>,
    large_receiver: Arc<RwLock<mpsc::UnboundedReceiver<BytesMut>>>,
}

impl MessageBufferPool {
    /// Create a new buffer pool with pre-allocated buffers
    pub fn new() -> Self {
        let (small_tx, small_rx) = mpsc::unbounded_channel();
        let (medium_tx, medium_rx) = mpsc::unbounded_channel();
        let (large_tx, large_rx) = mpsc::unbounded_channel();
        
        // Pre-allocate buffers
        for _ in 0..100 {
            let _ = small_tx.send(BytesMut::with_capacity(1024));
        }
        for _ in 0..50 {
            let _ = medium_tx.send(BytesMut::with_capacity(65536));
        }
        for _ in 0..10 {
            let _ = large_tx.send(BytesMut::with_capacity(1024 * 1024));
        }
        
        Self {
            small_buffers: small_tx,
            small_receiver: Arc::new(RwLock::new(small_rx)),
            medium_buffers: medium_tx,
            medium_receiver: Arc::new(RwLock::new(medium_rx)),
            large_buffers: large_tx,
            large_receiver: Arc::new(RwLock::new(large_rx)),
        }
    }
    
    /// Get a buffer of appropriate size
    pub async fn get_buffer(&self, size_hint: usize) -> BytesMut {
        if size_hint <= 1024 {
            if let Ok(mut rx) = self.small_receiver.try_write() {
                if let Ok(buffer) = rx.try_recv() {
                    return buffer;
                }
            }
            BytesMut::with_capacity(1024)
        } else if size_hint <= 65536 {
            if let Ok(mut rx) = self.medium_receiver.try_write() {
                if let Ok(buffer) = rx.try_recv() {
                    return buffer;
                }
            }
            BytesMut::with_capacity(65536)
        } else {
            if let Ok(mut rx) = self.large_receiver.try_write() {
                if let Ok(buffer) = rx.try_recv() {
                    return buffer;
                }
            }
            BytesMut::with_capacity(size_hint.max(1024 * 1024))
        }
    }
    
    /// Return a buffer to the pool
    pub fn return_buffer(&self, mut buffer: BytesMut) {
        buffer.clear();
        
        let capacity = buffer.capacity();
        if capacity <= 1024 {
            let _ = self.small_buffers.send(buffer);
        } else if capacity <= 65536 {
            let _ = self.medium_buffers.send(buffer);
        } else {
            let _ = self.large_buffers.send(buffer);
        }
    }
}

/// Connection pool with optimized lookup
pub struct OptimizedConnectionPool {
    /// Active connections indexed by node ID
    connections: RwLock<HashMap<u64, Arc<IrohConnection>>>,
    /// Connection establishment in progress
    connecting: RwLock<HashMap<u64, tokio::sync::watch::Receiver<bool>>>,
    /// Endpoint for creating new connections
    endpoint: Endpoint,
    /// Buffer pool for messages
    buffer_pool: Arc<MessageBufferPool>,
}

/// Optimized connection wrapper
pub struct IrohConnection {
    /// Iroh connection
    connection: iroh::endpoint::Connection,
    /// Target node info
    node_addr: NodeAddr,
    /// Connection metadata
    peer_id: u64,
    /// Last activity timestamp
    last_used: std::sync::atomic::AtomicU64,
}

impl IrohConnection {
    /// Send a message without cloning the payload
    pub async fn send_message_ref(&self, envelope: &MessageEnvelope<'_>) -> BlixardResult<Bytes> {
        // Update last used timestamp
        self.last_used.store(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            std::sync::atomic::Ordering::Relaxed,
        );
        
        // Open a stream for this message
        let mut stream = self.connection.open_uni().await
            .map_err(|e| BlixardError::NetworkError(format!("Failed to open stream: {}", e)))?;
        
        // Build message header without allocating
        let header = crate::transport::iroh_protocol::MessageHeader::new(
            crate::transport::iroh_protocol::MessageType::Request,
            envelope.payload.len() as u32,
            *envelope.request_id,
        );
        
        // Write header
        stream.write_all(&header.to_bytes()).await
            .map_err(|e| BlixardError::NetworkError(format!("Failed to write header: {}", e)))?;
        
        // Write service name length and service name
        let service_bytes = envelope.service.as_bytes();
        stream.write_all(&(service_bytes.len() as u16).to_be_bytes()).await
            .map_err(|e| BlixardError::NetworkError(format!("Failed to write service length: {}", e)))?;
        stream.write_all(service_bytes).await
            .map_err(|e| BlixardError::NetworkError(format!("Failed to write service name: {}", e)))?;
        
        // Write method name length and method name
        let method_bytes = envelope.method.as_bytes();
        stream.write_all(&(method_bytes.len() as u16).to_be_bytes()).await
            .map_err(|e| BlixardError::NetworkError(format!("Failed to write method length: {}", e)))?;
        stream.write_all(method_bytes).await
            .map_err(|e| BlixardError::NetworkError(format!("Failed to write method name: {}", e)))?;
        
        // Write payload directly without copying
        stream.write_all(envelope.payload).await
            .map_err(|e| BlixardError::NetworkError(format!("Failed to write payload: {}", e)))?;
        
        // Finish writing
        stream.finish().await
            .map_err(|e| BlixardError::NetworkError(format!("Failed to finish stream: {}", e)))?;
        
        // For now, return empty response - in a real implementation,
        // we'd need to handle bidirectional communication
        Ok(Bytes::new())
    }
}

impl OptimizedConnectionPool {
    /// Create a new optimized connection pool
    pub fn new(endpoint: Endpoint) -> Self {
        Self {
            connections: RwLock::new(HashMap::with_capacity(64)),
            connecting: RwLock::new(HashMap::new()),
            endpoint,
            buffer_pool: Arc::new(MessageBufferPool::new()),
        }
    }
    
    /// Get or create a connection to the target node
    pub async fn get_connection(&self, target: u64, node_addr: NodeAddr) -> BlixardResult<Arc<IrohConnection>> {
        // Fast path: check if connection already exists
        {
            let connections = self.connections.read().await;
            if let Some(conn) = connections.get(&target) {
                return Ok(conn.clone());
            }
        }
        
        // Check if connection is being established
        {
            let connecting = self.connecting.read().await;
            if let Some(mut rx) = connecting.get(&target).cloned() {
                drop(connecting);
                // Wait for connection to be established
                let _ = rx.changed().await;
                // Try again to get the connection
                let connections = self.connections.read().await;
                if let Some(conn) = connections.get(&target) {
                    return Ok(conn.clone());
                }
            }
        }
        
        // Establish new connection
        self.establish_connection(target, node_addr).await
    }
    
    /// Establish a new connection
    async fn establish_connection(&self, target: u64, node_addr: NodeAddr) -> BlixardResult<Arc<IrohConnection>> {
        let (tx, rx) = tokio::sync::watch::channel(false);
        
        // Mark as connecting
        {
            let mut connecting = self.connecting.write().await;
            connecting.insert(target, rx);
        }
        
        // Connect to the peer
        let connection = self.endpoint.connect(node_addr.clone(), b"blixard-p2p").await
            .map_err(|e| BlixardError::NetworkError(format!("Failed to connect to {}: {}", target, e)))?;
        
        // Create connection wrapper
        let iroh_conn = Arc::new(IrohConnection {
            connection,
            node_addr,
            peer_id: target,
            last_used: std::sync::atomic::AtomicU64::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            ),
        });
        
        // Store the connection
        {
            let mut connections = self.connections.write().await;
            connections.insert(target, iroh_conn.clone());
        }
        
        // Remove from connecting and notify waiters
        {
            let mut connecting = self.connecting.write().await;
            connecting.remove(&target);
        }
        let _ = tx.send(true);
        
        Ok(iroh_conn)
    }
    
    /// Send a message using zero-copy where possible
    pub async fn send_message_optimized(
        &self,
        envelope: &MessageEnvelope<'_>,
        node_addr: NodeAddr,
    ) -> BlixardResult<Bytes> {
        let connection = self.get_connection(envelope.target, node_addr).await?;
        connection.send_message_ref(envelope).await
    }
    
    /// Remove stale connections
    pub async fn cleanup_stale_connections(&self, max_idle_secs: u64) {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let mut to_remove = Vec::new();
        
        {
            let connections = self.connections.read().await;
            for (node_id, conn) in connections.iter() {
                let last_used = conn.last_used.load(std::sync::atomic::Ordering::Relaxed);
                if current_time.saturating_sub(last_used) > max_idle_secs {
                    to_remove.push(*node_id);
                }
            }
        }
        
        if !to_remove.is_empty() {
            let mut connections = self.connections.write().await;
            for node_id in to_remove {
                connections.remove(&node_id);
                debug!("Removed stale connection to node {}", node_id);
            }
        }
    }
}

/// Message router with priority queues
pub struct OptimizedMessageRouter {
    /// Connection pool
    pool: Arc<OptimizedConnectionPool>,
    /// Priority queues for outbound messages
    high_priority_queue: mpsc::UnboundedSender<QueuedMessage>,
    normal_priority_queue: mpsc::UnboundedSender<QueuedMessage>,
    low_priority_queue: mpsc::UnboundedSender<QueuedMessage>,
}

/// Queued message with routing information
struct QueuedMessage {
    envelope: MessageEnvelopeOwned,
    node_addr: NodeAddr,
    retries: u32,
}

/// Owned version of MessageEnvelope for queuing
#[derive(Debug, Clone)]
struct MessageEnvelopeOwned {
    target: u64,
    service: String,
    method: String,
    payload: Bytes,
    priority: MessagePriority,
    request_id: [u8; 16],
}

impl From<&MessageEnvelope<'_>> for MessageEnvelopeOwned {
    fn from(envelope: &MessageEnvelope<'_>) -> Self {
        Self {
            target: envelope.target,
            service: envelope.service.to_string(),
            method: envelope.method.to_string(),
            payload: Bytes::copy_from_slice(envelope.payload),
            priority: envelope.priority,
            request_id: *envelope.request_id,
        }
    }
}

impl OptimizedMessageRouter {
    /// Create a new message router
    pub fn new(endpoint: Endpoint) -> Self {
        let pool = Arc::new(OptimizedConnectionPool::new(endpoint));
        
        let (high_tx, high_rx) = mpsc::unbounded_channel();
        let (normal_tx, normal_rx) = mpsc::unbounded_channel();
        let (low_tx, low_rx) = mpsc::unbounded_channel();
        
        // Start message processing tasks
        Self::start_message_processor(pool.clone(), high_rx, "high");
        Self::start_message_processor(pool.clone(), normal_rx, "normal");
        Self::start_message_processor(pool.clone(), low_rx, "low");
        
        Self {
            pool,
            high_priority_queue: high_tx,
            normal_priority_queue: normal_tx,
            low_priority_queue: low_tx,
        }
    }
    
    /// Start a message processing task
    fn start_message_processor(
        pool: Arc<OptimizedConnectionPool>,
        mut rx: mpsc::UnboundedReceiver<QueuedMessage>,
        queue_name: &'static str,
    ) {
        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                let envelope = MessageEnvelope {
                    target: message.envelope.target,
                    service: &message.envelope.service,
                    method: &message.envelope.method,
                    payload: &message.envelope.payload,
                    priority: message.envelope.priority,
                    request_id: &message.envelope.request_id,
                };
                
                match pool.send_message_optimized(&envelope, message.node_addr).await {
                    Ok(_) => {
                        trace!("Message sent successfully via {} queue", queue_name);
                    }
                    Err(e) => {
                        error!("Failed to send message via {} queue: {}", queue_name, e);
                        // TODO: Implement retry logic
                    }
                }
            }
        });
    }
    
    /// Route a message based on priority
    pub async fn route_message(
        &self,
        envelope: &MessageEnvelope<'_>,
        node_addr: NodeAddr,
    ) -> BlixardResult<()> {
        let queued = QueuedMessage {
            envelope: envelope.into(),
            node_addr,
            retries: 0,
        };
        
        let result = match envelope.priority {
            MessagePriority::Critical | MessagePriority::High => {
                self.high_priority_queue.send(queued)
            }
            MessagePriority::Normal => {
                self.normal_priority_queue.send(queued)
            }
            MessagePriority::Low => {
                self.low_priority_queue.send(queued)
            }
        };
        
        result.map_err(|_| BlixardError::Internal {
            message: "Failed to queue message for routing".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_message_priority_from_service() {
        assert_eq!(
            MessagePriority::from_service_method("raft", "append"),
            MessagePriority::Critical
        );
        assert_eq!(
            MessagePriority::from_service_method("health", "check"),
            MessagePriority::High
        );
        assert_eq!(
            MessagePriority::from_service_method("vm", "create"),
            MessagePriority::Normal
        );
        assert_eq!(
            MessagePriority::from_service_method("unknown", "method"),
            MessagePriority::Low
        );
    }
    
    #[tokio::test]
    async fn test_buffer_pool() {
        let pool = MessageBufferPool::new();
        
        // Get buffers of different sizes
        let small = pool.get_buffer(512).await;
        let medium = pool.get_buffer(32768).await;
        let large = pool.get_buffer(512000).await;
        
        assert!(small.capacity() >= 512);
        assert!(medium.capacity() >= 32768);
        assert!(large.capacity() >= 512000);
        
        // Return buffers
        pool.return_buffer(small);
        pool.return_buffer(medium);
        pool.return_buffer(large);
    }
}