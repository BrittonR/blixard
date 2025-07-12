//! Optimized Raft message processing with minimal cloning
//!
//! This module provides optimized Raft message handling with:
//! - Reference-based message processing where possible
//! - Pre-allocated buffers for message serialization
//! - Efficient batch processing
//! - Zero-copy message forwarding

use crate::error::{BlixardError, BlixardResult};
use bytes::{Bytes, BytesMut};
use raft::prelude::*;
use std::collections::VecDeque;
use tokio::sync::{Mutex, RwLock};
use tracing::trace;

/// Message processing statistics
#[derive(Debug, Default)]
pub struct ProcessingStats {
    pub messages_processed: u64,
    pub bytes_processed: u64,
    pub batches_processed: u64,
    pub zero_copy_forwards: u64,
    pub serialization_reuses: u64,
}

/// Optimized message batch for processing
#[derive(Debug)]
pub struct MessageBatch<'a> {
    /// Messages in this batch (borrowed to avoid cloning)
    pub messages: &'a [Message],
    /// Total batch size in bytes
    pub total_size: usize,
    /// Batch priority
    pub priority: BatchPriority,
    /// Timestamp when batch was created
    pub created_at: std::time::Instant,
}

/// Batch priority for processing order
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BatchPriority {
    /// Election-related messages
    Election = 0,
    /// Heartbeat messages
    Heartbeat = 1,
    /// Normal append entries
    Normal = 2,
    /// Snapshot transfers
    Snapshot = 3,
}

impl BatchPriority {
    fn from_message_type(msg_type: MessageType) -> Self {
        match msg_type {
            MessageType::MsgRequestVote | MessageType::MsgRequestVoteResponse => Self::Election,
            MessageType::MsgHeartbeat | MessageType::MsgHeartbeatResponse => Self::Heartbeat,
            MessageType::MsgSnapshot => Self::Snapshot,
            _ => Self::Normal,
        }
    }
}

/// Pre-allocated buffer for message serialization
pub struct SerializationBuffer {
    /// Buffer for small messages (< 4KB)
    small_buffer: BytesMut,
    /// Buffer for medium messages (4KB - 64KB)
    medium_buffer: BytesMut,
    /// Buffer for large messages (> 64KB)
    large_buffer: BytesMut,
    /// Statistics
    reuse_count: u64,
}

impl SerializationBuffer {
    fn new() -> Self {
        Self {
            small_buffer: BytesMut::with_capacity(4096),
            medium_buffer: BytesMut::with_capacity(65536),
            large_buffer: BytesMut::with_capacity(1024 * 1024),
            reuse_count: 0,
        }
    }
    
    /// Get appropriate buffer for message size
    fn get_buffer(&mut self, size_hint: usize) -> &mut BytesMut {
        self.reuse_count += 1;
        
        if size_hint <= 4096 {
            self.small_buffer.clear();
            if self.small_buffer.capacity() < size_hint {
                self.small_buffer.reserve(size_hint - self.small_buffer.capacity());
            }
            &mut self.small_buffer
        } else if size_hint <= 65536 {
            self.medium_buffer.clear();
            if self.medium_buffer.capacity() < size_hint {
                self.medium_buffer.reserve(size_hint - self.medium_buffer.capacity());
            }
            &mut self.medium_buffer
        } else {
            self.large_buffer.clear();
            if self.large_buffer.capacity() < size_hint {
                self.large_buffer.reserve(size_hint - self.large_buffer.capacity());
            }
            &mut self.large_buffer
        }
    }
    
    fn reuse_stats(&self) -> u64 {
        self.reuse_count
    }
}

/// Message processor with optimized handling
pub struct OptimizedRaftProcessor {
    /// Buffer for serialization reuse
    serialization_buffer: Mutex<SerializationBuffer>,
    /// Processing statistics
    stats: RwLock<ProcessingStats>,
    /// Pending message batches by priority
    election_queue: Mutex<VecDeque<Vec<Message>>>,
    heartbeat_queue: Mutex<VecDeque<Vec<Message>>>,
    normal_queue: Mutex<VecDeque<Vec<Message>>>,
    snapshot_queue: Mutex<VecDeque<Vec<Message>>>,
}

impl OptimizedRaftProcessor {
    /// Create a new optimized processor
    pub fn new() -> Self {
        Self {
            serialization_buffer: Mutex::new(SerializationBuffer::new()),
            stats: RwLock::new(ProcessingStats::default()),
            election_queue: Mutex::new(VecDeque::new()),
            heartbeat_queue: Mutex::new(VecDeque::new()),
            normal_queue: Mutex::new(VecDeque::new()),
            snapshot_queue: Mutex::new(VecDeque::new()),
        }
    }
    
    /// Process a batch of messages with minimal copying
    pub async fn process_message_batch_ref(&self, batch: &MessageBatch<'_>) -> BlixardResult<Vec<Bytes>> {
        let mut results = Vec::with_capacity(batch.messages.len());
        let mut buffer = self.serialization_buffer.lock().await;
        
        for message in batch.messages {
            // Serialize message using reused buffer
            let serialized = self.serialize_message_optimized(message, &mut buffer)?;
            results.push(serialized);
        }
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.messages_processed += batch.messages.len() as u64;
            stats.bytes_processed += batch.total_size as u64;
            stats.batches_processed += 1;
            stats.serialization_reuses = buffer.reuse_stats();
        }
        
        Ok(results)
    }
    
    /// Serialize message using pre-allocated buffer
    fn serialize_message_optimized(
        &self,
        message: &Message,
        buffer: &mut SerializationBuffer,
    ) -> BlixardResult<Bytes> {
        // Estimate message size for buffer selection
        let estimated_size = self.estimate_message_size(message);
        let buf = buffer.get_buffer(estimated_size);
        
        // Serialize directly into the buffer
        postcard::to_extend(message, buf)
            .map_err(|e| BlixardError::Serialization {
                message: format!("Failed to serialize Raft message: {}", e),
            })?;
        
        Ok(buf.clone().freeze())
    }
    
    /// Estimate message size for buffer selection
    fn estimate_message_size(&self, message: &Message) -> usize {
        // Base message header size
        let mut size = 64;
        
        // Add estimated payload size based on message type
        match message.msg_type() {
            MessageType::MsgAppend => {
                // AppendEntries can be large
                size += message.entries.len() * 256;
            }
            MessageType::MsgSnapshot => {
                // Snapshots are typically large
                size += 1024 * 1024;
            }
            MessageType::MsgRequestVote | MessageType::MsgRequestVoteResponse => {
                // Vote messages are small
                size += 32;
            }
            MessageType::MsgHeartbeat | MessageType::MsgHeartbeatResponse => {
                // Heartbeats are very small
                size += 16;
            }
            _ => {
                // Default estimate
                size += 128;
            }
        }
        
        size
    }
    
    /// Queue messages for batch processing
    pub async fn queue_message(&self, message: Message) -> BlixardResult<()> {
        let priority = BatchPriority::from_message_type(message.msg_type());
        
        match priority {
            BatchPriority::Election => {
                let mut queue = self.election_queue.lock().await;
                if queue.is_empty() || queue.back().map_or(false, |b| b.len() >= 32) {
                    queue.push_back(Vec::with_capacity(32));
                }
                queue.back_mut().expect("queue must have an element after conditional push").push(message);
            }
            BatchPriority::Heartbeat => {
                let mut queue = self.heartbeat_queue.lock().await;
                if queue.is_empty() || queue.back().map_or(false, |b| b.len() >= 64) {
                    queue.push_back(Vec::with_capacity(64));
                }
                queue.back_mut().expect("queue must have an element after conditional push").push(message);
            }
            BatchPriority::Normal => {
                let mut queue = self.normal_queue.lock().await;
                if queue.is_empty() || queue.back().map_or(false, |b| b.len() >= 16) {
                    queue.push_back(Vec::with_capacity(16));
                }
                queue.back_mut().expect("queue must have an element after conditional push").push(message);
            }
            BatchPriority::Snapshot => {
                let mut queue = self.snapshot_queue.lock().await;
                if queue.is_empty() || queue.back().map_or(false, |b| b.len() >= 4) {
                    queue.push_back(Vec::with_capacity(4));
                }
                queue.back_mut().expect("queue must have an element after conditional push").push(message);
            }
        }
        
        Ok(())
    }
    
    /// Process next batch by priority
    pub async fn process_next_batch(&self) -> BlixardResult<Option<Vec<Bytes>>> {
        // Process in priority order
        if let Some(batch) = self.election_queue.lock().await.pop_front() {
            return self.process_batch_owned(batch, BatchPriority::Election).await.map(Some);
        }
        
        if let Some(batch) = self.heartbeat_queue.lock().await.pop_front() {
            return self.process_batch_owned(batch, BatchPriority::Heartbeat).await.map(Some);
        }
        
        if let Some(batch) = self.normal_queue.lock().await.pop_front() {
            return self.process_batch_owned(batch, BatchPriority::Normal).await.map(Some);
        }
        
        if let Some(batch) = self.snapshot_queue.lock().await.pop_front() {
            return self.process_batch_owned(batch, BatchPriority::Snapshot).await.map(Some);
        }
        
        Ok(None)
    }
    
    /// Process an owned batch of messages
    async fn process_batch_owned(&self, messages: Vec<Message>, priority: BatchPriority) -> BlixardResult<Vec<Bytes>> {
        let total_size = messages.iter().map(|m| self.estimate_message_size(m)).sum();
        
        let batch = MessageBatch {
            messages: &messages,
            total_size,
            priority,
            created_at: std::time::Instant::now(),
        };
        
        trace!(
            "Processing {} message batch with {} messages, {} bytes",
            match priority {
                BatchPriority::Election => "election",
                BatchPriority::Heartbeat => "heartbeat", 
                BatchPriority::Normal => "normal",
                BatchPriority::Snapshot => "snapshot",
            },
            messages.len(),
            total_size
        );
        
        self.process_message_batch_ref(&batch).await
    }
    
    /// Get processing statistics
    pub async fn get_stats(&self) -> ProcessingStats {
        self.stats.read().await.clone()
    }
    
    /// Reset statistics
    pub async fn reset_stats(&self) {
        *self.stats.write().await = ProcessingStats::default();
    }
}

/// Zero-copy message forwarder for proxying
pub struct MessageForwarder {
    /// Statistics
    stats: RwLock<ProcessingStats>,
}

impl MessageForwarder {
    pub fn new() -> Self {
        Self {
            stats: RwLock::new(ProcessingStats::default()),
        }
    }
    
    /// Forward a message without deserializing/re-serializing
    pub async fn forward_message_zero_copy(
        &self,
        serialized_message: &[u8],
        target_nodes: &[u64],
    ) -> BlixardResult<()> {
        // In a real implementation, this would forward the raw bytes
        // to the target nodes without deserializing and re-serializing
        
        for &target in target_nodes {
            trace!("Forwarding {} bytes to node {}", serialized_message.len(), target);
            // TODO: Implement actual forwarding via transport layer
        }
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.zero_copy_forwards += target_nodes.len() as u64;
            stats.bytes_processed += serialized_message.len() as u64 * target_nodes.len() as u64;
        }
        
        Ok(())
    }
    
    /// Check if a message can be forwarded without processing
    pub fn can_forward_directly(message_type: MessageType) -> bool {
        match message_type {
            // These can typically be forwarded directly
            MessageType::MsgHeartbeat |
            MessageType::MsgHeartbeatResponse |
            MessageType::MsgRequestVote |
            MessageType::MsgRequestVoteResponse => true,
            
            // These might need processing
            MessageType::MsgAppend |
            MessageType::MsgAppendResponse |
            MessageType::MsgSnapshot => false,
            
            _ => false,
        }
    }
}

/// Message deduplication cache using HashMap (LRU removed for compilation)
pub struct MessageCache {
    /// Recently seen message hashes  
    seen_messages: RwLock<std::collections::HashMap<[u8; 16], std::time::Instant>>,
    /// Cache hit statistics
    cache_hits: std::sync::atomic::AtomicU64,
    /// Cache miss statistics  
    cache_misses: std::sync::atomic::AtomicU64,
    /// Max capacity
    max_capacity: usize,
}

impl MessageCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            seen_messages: RwLock::new(std::collections::HashMap::with_capacity(capacity)),
            cache_hits: std::sync::atomic::AtomicU64::new(0),
            cache_misses: std::sync::atomic::AtomicU64::new(0),
            max_capacity: capacity,
        }
    }
    
    /// Check if a message has been seen recently
    pub async fn is_duplicate(&self, message_hash: &[u8; 16]) -> bool {
        let mut cache = self.seen_messages.write().await;
        
        if cache.contains_key(message_hash) {
            self.cache_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            true
        } else {
            // Simple eviction if over capacity
            if cache.len() >= self.max_capacity {
                cache.clear();
            }
            cache.insert(*message_hash, std::time::Instant::now());
            self.cache_misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            false
        }
    }
    
    /// Get cache statistics
    pub fn get_cache_stats(&self) -> (u64, u64) {
        (
            self.cache_hits.load(std::sync::atomic::Ordering::Relaxed),
            self.cache_misses.load(std::sync::atomic::Ordering::Relaxed),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_batch_priority_ordering() {
        assert!(BatchPriority::Election < BatchPriority::Heartbeat);
        assert!(BatchPriority::Heartbeat < BatchPriority::Normal);
        assert!(BatchPriority::Normal < BatchPriority::Snapshot);
    }
    
    #[tokio::test]
    async fn test_serialization_buffer_reuse() {
        let mut buffer = SerializationBuffer::new();
        
        // Use small buffer multiple times
        let _buf1 = buffer.get_buffer(1024);
        let _buf2 = buffer.get_buffer(2048);
        let _buf3 = buffer.get_buffer(512);
        
        assert_eq!(buffer.reuse_stats(), 3);
    }
    
    #[tokio::test]
    async fn test_message_deduplication() {
        let cache = MessageCache::new(100);
        let hash = [1u8; 16];
        
        // First time should not be duplicate
        assert!(!cache.is_duplicate(&hash).await);
        
        // Second time should be duplicate
        assert!(cache.is_duplicate(&hash).await);
        
        let (hits, misses) = cache.get_cache_stats();
        assert_eq!(hits, 1);
        assert_eq!(misses, 1);
    }
}