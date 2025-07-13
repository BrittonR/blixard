//! Simple performance optimizations that don't require additional dependencies
//!
//! This module provides immediately applicable performance optimizations
//! that focus on the most impactful changes with minimal complexity.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Pre-allocated empty attribute slice to avoid allocations
pub static EMPTY_ATTRS: &[opentelemetry::KeyValue] = &[];

/// Fast boolean flags using atomics to avoid locking
#[derive(Debug)]
pub struct AtomicFlags {
    pub is_leader: AtomicBool,
    pub is_initialized: AtomicBool,
    pub is_connected: AtomicBool,
}

impl AtomicFlags {
    pub fn new() -> Self {
        Self {
            is_leader: AtomicBool::new(false),
            is_initialized: AtomicBool::new(false),
            is_connected: AtomicBool::new(false),
        }
    }
    
    #[inline]
    pub fn is_leader(&self) -> bool {
        self.is_leader.load(Ordering::Acquire)
    }
    
    #[inline]
    pub fn set_leader(&self, value: bool) {
        self.is_leader.store(value, Ordering::Release);
    }
    
    #[inline]
    pub fn is_initialized(&self) -> bool {
        self.is_initialized.load(Ordering::Acquire)
    }
    
    #[inline]
    pub fn set_initialized(&self, value: bool) {
        self.is_initialized.store(value, Ordering::Release);
    }
    
    #[inline]
    pub fn is_connected(&self) -> bool {
        self.is_connected.load(Ordering::Acquire)
    }
    
    #[inline]
    pub fn set_connected(&self, value: bool) {
        self.is_connected.store(value, Ordering::Release);
    }
}

/// Atomic counters for metrics to avoid locking
#[derive(Debug)]
pub struct AtomicCounters {
    pub vm_count: AtomicU64,
    pub running_vms: AtomicU64,
    pub cluster_nodes: AtomicU64,
    pub total_vcpus: AtomicU64,
    pub used_vcpus: AtomicU64,
}

impl AtomicCounters {
    pub fn new() -> Self {
        Self {
            vm_count: AtomicU64::new(0),
            running_vms: AtomicU64::new(0),
            cluster_nodes: AtomicU64::new(0),
            total_vcpus: AtomicU64::new(0),
            used_vcpus: AtomicU64::new(0),
        }
    }
    
    #[inline]
    pub fn vm_count(&self) -> u64 {
        self.vm_count.load(Ordering::Acquire)
    }
    
    #[inline]
    pub fn increment_vm_count(&self) -> u64 {
        self.vm_count.fetch_add(1, Ordering::AcqRel)
    }
    
    #[inline]
    pub fn decrement_vm_count(&self) -> u64 {
        self.vm_count.fetch_sub(1, Ordering::AcqRel)
    }
    
    #[inline]
    pub fn running_vms(&self) -> u64 {
        self.running_vms.load(Ordering::Acquire)
    }
    
    #[inline]
    pub fn set_running_vms(&self, count: u64) {
        self.running_vms.store(count, Ordering::Release);
    }
    
    #[inline]
    pub fn cluster_nodes(&self) -> u64 {
        self.cluster_nodes.load(Ordering::Acquire)
    }
    
    #[inline]
    pub fn set_cluster_nodes(&self, count: u64) {
        self.cluster_nodes.store(count, Ordering::Release);
    }
    
    #[inline]
    pub fn total_vcpus(&self) -> u64 {
        self.total_vcpus.load(Ordering::Acquire)
    }
    
    #[inline]
    pub fn set_total_vcpus(&self, count: u64) {
        self.total_vcpus.store(count, Ordering::Release);
    }
    
    #[inline]
    pub fn used_vcpus(&self) -> u64 {
        self.used_vcpus.load(Ordering::Acquire)
    }
    
    #[inline]
    pub fn add_used_vcpus(&self, count: u64) -> u64 {
        self.used_vcpus.fetch_add(count, Ordering::AcqRel)
    }
    
    #[inline]
    pub fn remove_used_vcpus(&self, count: u64) -> u64 {
        self.used_vcpus.fetch_sub(count, Ordering::AcqRel)
    }
}

/// Pre-allocated string constants to avoid allocations in hot paths
pub mod string_constants {
    pub const OPERATION_CREATE: &str = "create";
    pub const OPERATION_START: &str = "start";
    pub const OPERATION_STOP: &str = "stop";
    pub const OPERATION_DELETE: &str = "delete";
    
    pub const SERVICE_HEALTH: &str = "health";
    pub const SERVICE_CLUSTER: &str = "cluster";
    pub const SERVICE_VM: &str = "vm";
    pub const SERVICE_RAFT: &str = "raft";
    
    pub const METHOD_CHECK: &str = "check";
    pub const METHOD_STATUS: &str = "status";
    pub const METHOD_APPEND: &str = "append";
    pub const METHOD_VOTE: &str = "vote";
    
    pub const STATUS_SUCCESS: &str = "success";
    pub const STATUS_FAILED: &str = "failed";
    
    pub const TRANSPORT_IROH: &str = "iroh";
    pub const TRANSPORT_GRPC: &str = "grpc";
}

/// Helper for creating KeyValue attributes without string allocations
pub fn create_node_id_attr(node_id: u64) -> opentelemetry::KeyValue {
    opentelemetry::KeyValue::new("node_id", node_id as i64)
}

/// Helper for creating operation attributes
pub fn create_operation_attr(operation: &'static str) -> opentelemetry::KeyValue {
    opentelemetry::KeyValue::new("operation", operation)
}

/// Helper for creating service attributes
pub fn create_service_attr(service: &'static str) -> opentelemetry::KeyValue {
    opentelemetry::KeyValue::new("service", service)
}

/// Helper for reusing Vec capacity
pub fn reuse_vec_capacity<T>(mut vec: Vec<T>) -> Vec<T> {
    vec.clear();
    vec
}

/// Buffer pool for reusing byte vectors
pub struct SimpleBufferPool {
    small_buffers: std::sync::Mutex<Vec<Vec<u8>>>,
    medium_buffers: std::sync::Mutex<Vec<Vec<u8>>>,
    large_buffers: std::sync::Mutex<Vec<Vec<u8>>>,
}

impl SimpleBufferPool {
    pub fn new() -> Self {
        Self {
            small_buffers: std::sync::Mutex::new(Vec::new()),
            medium_buffers: std::sync::Mutex::new(Vec::new()),
            large_buffers: std::sync::Mutex::new(Vec::new()),
        }
    }
    
    /// Get a buffer of appropriate size
    pub fn get_buffer(&self, size_hint: usize) -> Vec<u8> {
        if size_hint <= 1024 {
            if let Ok(mut buffers) = self.small_buffers.try_lock() {
                if let Some(mut buffer) = buffers.pop() {
                    buffer.clear();
                    if buffer.capacity() >= size_hint {
                        return buffer;
                    }
                }
            }
            Vec::with_capacity(1024.max(size_hint))
        } else if size_hint <= 64 * 1024 {
            if let Ok(mut buffers) = self.medium_buffers.try_lock() {
                if let Some(mut buffer) = buffers.pop() {
                    buffer.clear();
                    if buffer.capacity() >= size_hint {
                        return buffer;
                    }
                }
            }
            Vec::with_capacity((64 * 1024).max(size_hint))
        } else {
            if let Ok(mut buffers) = self.large_buffers.try_lock() {
                if let Some(mut buffer) = buffers.pop() {
                    buffer.clear();
                    if buffer.capacity() >= size_hint {
                        return buffer;
                    }
                }
            }
            Vec::with_capacity(size_hint)
        }
    }
    
    /// Return a buffer to the pool
    pub fn return_buffer(&self, buffer: Vec<u8>) {
        let capacity = buffer.capacity();
        
        if capacity <= 1024 {
            if let Ok(mut buffers) = self.small_buffers.try_lock() {
                if buffers.len() < 10 { // Limit pool size
                    buffers.push(buffer);
                }
            }
        } else if capacity <= 64 * 1024 {
            if let Ok(mut buffers) = self.medium_buffers.try_lock() {
                if buffers.len() < 5 {
                    buffers.push(buffer);
                }
            }
        } else if let Ok(mut buffers) = self.large_buffers.try_lock() {
            if buffers.len() < 2 {
                buffers.push(buffer);
            }
        }
    }
}

/// Global buffer pool instance
static BUFFER_POOL: std::sync::OnceLock<SimpleBufferPool> = std::sync::OnceLock::new();

/// Get the global buffer pool
pub fn buffer_pool() -> &'static SimpleBufferPool {
    BUFFER_POOL.get_or_init(|| SimpleBufferPool::new())
}

/// RAII guard for automatic buffer return
pub struct BufferGuard {
    buffer: Option<Vec<u8>>,
}

impl BufferGuard {
    pub fn new(size_hint: usize) -> Self {
        Self {
            buffer: Some(buffer_pool().get_buffer(size_hint)),
        }
    }
    
    pub fn get_mut(&mut self) -> Option<&mut Vec<u8>> {
        self.buffer.as_mut()
    }
    
    pub fn get(&self) -> Option<&Vec<u8>> {
        self.buffer.as_ref()
    }
    
    /// Get mutable buffer reference, returning error if buffer is not available
    pub fn try_get_mut(&mut self) -> crate::error::BlixardResult<&mut Vec<u8>> {
        self.buffer.as_mut().ok_or_else(|| crate::error::BlixardError::Internal {
            message: "Buffer is not available (possibly after drop)".to_string(),
        })
    }
    
    /// Get buffer reference, returning error if buffer is not available
    pub fn try_get(&self) -> crate::error::BlixardResult<&Vec<u8>> {
        self.buffer.as_ref().ok_or_else(|| crate::error::BlixardError::Internal {
            message: "Buffer is not available (possibly after drop)".to_string(),
        })
    }
}

impl Drop for BufferGuard {
    fn drop(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            buffer_pool().return_buffer(buffer);
        }
    }
}

/// Performance optimization helpers for common patterns
pub mod helpers {
    use super::*;
    
    /// Fast VM operation recording without string allocations
    pub fn record_vm_operation_fast(
        operation: &'static str,
        success: bool,
        metrics: &crate::metrics_otel::Metrics,
    ) {
        let attrs = if success {
            [create_operation_attr(operation)]
        } else {
            [
                create_operation_attr(operation),
                opentelemetry::KeyValue::new("error", true),
            ]
        };
        
        match operation {
            string_constants::OPERATION_CREATE => {
                metrics.vm_create_total.add(1, &attrs);
                if !success {
                    metrics.vm_create_failed.add(1, &attrs);
                }
            }
            string_constants::OPERATION_START => {
                metrics.vm_start_total.add(1, &attrs);
                if !success {
                    metrics.vm_start_failed.add(1, &attrs);
                }
            }
            string_constants::OPERATION_STOP => {
                metrics.vm_stop_total.add(1, &attrs);
                if !success {
                    metrics.vm_stop_failed.add(1, &attrs);
                }
            }
            string_constants::OPERATION_DELETE => {
                metrics.vm_delete_total.add(1, &attrs);
                if !success {
                    metrics.vm_delete_failed.add(1, &attrs);
                }
            }
            _ => {}
        }
    }
    
    /// Fast resource metrics update without allocations
    pub fn update_resource_metrics_fast(
        node_id: u64,
        vcpus: u64,
        memory_mb: u64,
        disk_gb: u64,
        metrics: &crate::metrics_otel::Metrics,
    ) {
        let attrs = [create_node_id_attr(node_id)];
        
        metrics.node_vcpus_available.add(vcpus as i64, &attrs);
        metrics.node_memory_mb_available.add(memory_mb as i64, &attrs);
        metrics.node_disk_gb_available.add(disk_gb as i64, &attrs);
    }
    
    /// Fast cluster metrics update without allocations
    pub fn update_cluster_metrics_fast(
        total_nodes: u64,
        total_vcpus: u64,
        used_vcpus: u64,
        total_memory_mb: u64,
        used_memory_mb: u64,
        metrics: &crate::metrics_otel::Metrics,
    ) {
        // Use empty attributes to avoid any allocations
        metrics.cluster_nodes_total.add(total_nodes as i64, EMPTY_ATTRS);
        metrics.cluster_vcpus_total.add(total_vcpus as i64, EMPTY_ATTRS);
        metrics.cluster_vcpus_used.add(used_vcpus as i64, EMPTY_ATTRS);
        metrics.cluster_memory_mb_total.add(total_memory_mb as i64, EMPTY_ATTRS);
        metrics.cluster_memory_mb_used.add(used_memory_mb as i64, EMPTY_ATTRS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_atomic_flags() {
        let flags = AtomicFlags::new();
        
        assert!(!flags.is_leader());
        flags.set_leader(true);
        assert!(flags.is_leader());
        
        assert!(!flags.is_initialized());
        flags.set_initialized(true);
        assert!(flags.is_initialized());
    }
    
    #[test]
    fn test_atomic_counters() {
        let counters = AtomicCounters::new();
        
        assert_eq!(counters.vm_count(), 0);
        counters.increment_vm_count();
        assert_eq!(counters.vm_count(), 1);
        counters.decrement_vm_count();
        assert_eq!(counters.vm_count(), 0);
    }
    
    #[test]
    fn test_buffer_pool() {
        let pool = SimpleBufferPool::new();
        
        let buffer1 = pool.get_buffer(512);
        assert!(buffer1.capacity() >= 512);
        
        pool.return_buffer(buffer1);
        
        let buffer2 = pool.get_buffer(256);
        // Should reuse the returned buffer
        assert!(buffer2.capacity() >= 256);
    }
    
    #[test]
    fn test_buffer_guard() {
        let mut guard = BufferGuard::new(1024);
        guard.get_mut().extend_from_slice(b"test data");
        assert_eq!(guard.get().len(), 9);
        // Buffer automatically returned on drop
    }
}