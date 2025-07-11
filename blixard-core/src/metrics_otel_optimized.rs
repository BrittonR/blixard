//! Optimized OpenTelemetry metrics with reduced hot path allocations
//!
//! This module provides optimized metric recording with:
//! - Pre-allocated static labels to avoid string allocations
//! - Cached attribute sets for common operations
//! - Lazy initialization of rarely used metrics
//! - Efficient attribute reuse patterns

use opentelemetry::KeyValue;
use std::{cell::RefCell, sync::Arc};

/// Static attribute values to avoid repeated allocations
pub mod static_attrs {
    use opentelemetry::KeyValue;
    use std::sync::OnceLock;
    use once_cell::sync::Lazy;
    
    // Pre-create common operation attributes
    pub static OPERATION_CREATE: OnceLock<KeyValue> = OnceLock::new();
    pub static OPERATION_START: OnceLock<KeyValue> = OnceLock::new();
    pub static OPERATION_STOP: OnceLock<KeyValue> = OnceLock::new();
    pub static OPERATION_DELETE: OnceLock<KeyValue> = OnceLock::new();
    
    // Pre-create common status attributes
    pub static STATUS_SUCCESS: Lazy<KeyValue> = Lazy::new(|| KeyValue::new("status", "success"));
    pub static STATUS_FAILED: Lazy<KeyValue> = Lazy::new(|| KeyValue::new("status", "failed"));
    
    // Pre-create common error attributes
    pub static ERROR_TRUE: Lazy<KeyValue> = Lazy::new(|| KeyValue::new("error", true));
    pub static ERROR_FALSE: Lazy<KeyValue> = Lazy::new(|| KeyValue::new("error", false));
    
    // Pre-create common service attributes
    pub static SERVICE_HEALTH: Lazy<KeyValue> = Lazy::new(|| KeyValue::new("service", "health"));
    pub static SERVICE_CLUSTER: Lazy<KeyValue> = Lazy::new(|| KeyValue::new("service", "cluster"));
    pub static SERVICE_VM: Lazy<KeyValue> = Lazy::new(|| KeyValue::new("service", "vm"));
    
    // Pre-create common transport types
    pub static TRANSPORT_IROH: Lazy<KeyValue> = Lazy::new(|| KeyValue::new("transport", "iroh"));
    pub static TRANSPORT_GRPC: Lazy<KeyValue> = Lazy::new(|| KeyValue::new("transport", "grpc"));
    
    // Pre-create common verification types
    pub static VERIFY_NAR_HASH: Lazy<KeyValue> = Lazy::new(|| KeyValue::new("verification_type", "nar_hash"));
    pub static VERIFY_CHUNK_HASH: Lazy<KeyValue> = Lazy::new(|| KeyValue::new("verification_type", "chunk_hash"));
    
    // Pre-create common cache types
    pub static CACHE_CHUNK: Lazy<KeyValue> = Lazy::new(|| KeyValue::new("cache_type", "chunk"));
    pub static CACHE_IMAGE: Lazy<KeyValue> = Lazy::new(|| KeyValue::new("cache_type", "image"));
    
    // Pre-create common placement strategies
    pub static STRATEGY_MOST_AVAILABLE: Lazy<KeyValue> = Lazy::new(|| KeyValue::new("strategy", "MostAvailable"));
    pub static STRATEGY_LEAST_AVAILABLE: Lazy<KeyValue> = Lazy::new(|| KeyValue::new("strategy", "LeastAvailable"));
    pub static STRATEGY_ROUND_ROBIN: Lazy<KeyValue> = Lazy::new(|| KeyValue::new("strategy", "RoundRobin"));
    pub static STRATEGY_MANUAL: Lazy<KeyValue> = Lazy::new(|| KeyValue::new("strategy", "Manual"));
}

/// Thread-local cache for commonly used attribute sets
thread_local! {
    static ATTR_CACHE: RefCell<AttributeCache> = RefCell::new(AttributeCache::new());
}

/// Cache for commonly used attribute combinations
struct AttributeCache {
    // Cache node ID attributes (frequently used in hot paths)
    node_attrs: std::collections::HashMap<u64, Arc<[KeyValue]>>,
    // Cache VM name attributes
    vm_attrs: std::collections::HashMap<String, Arc<[KeyValue]>>,
    // Cache combined operation attributes
    operation_attrs: std::collections::HashMap<(&'static str, bool), Arc<[KeyValue]>>,
}

impl AttributeCache {
    fn new() -> Self {
        Self {
            node_attrs: std::collections::HashMap::with_capacity(16),
            vm_attrs: std::collections::HashMap::with_capacity(128),
            operation_attrs: std::collections::HashMap::with_capacity(8),
        }
    }
    
    fn get_node_attrs(&mut self, node_id: u64) -> Arc<[KeyValue]> {
        self.node_attrs.entry(node_id)
            .or_insert_with(|| Arc::from([KeyValue::new("node_id", node_id as i64)]))
            .clone()
    }
    
    fn get_vm_attrs(&mut self, vm_name: &str) -> Arc<[KeyValue]> {
        self.vm_attrs.entry(vm_name.to_string())
            .or_insert_with(|| Arc::from([KeyValue::new("vm_name", vm_name.to_string())]))
            .clone()
    }
    
    fn get_operation_attrs(&mut self, operation: &'static str, success: bool) -> Arc<[KeyValue]> {
        self.operation_attrs.entry((operation, success))
            .or_insert_with(|| {
                let op_attr = match operation {
                    "create" => &*static_attrs::OPERATION_CREATE,
                    "start" => &*static_attrs::OPERATION_START,
                    "stop" => &*static_attrs::OPERATION_STOP,
                    "delete" => &*static_attrs::OPERATION_DELETE,
                    _ => return Arc::from([KeyValue::new("operation", operation.to_string())]),
                };
                
                if success {
                    Arc::from([op_attr.clone()])
                } else {
                    Arc::from([op_attr.clone(), static_attrs::ERROR_TRUE.clone()])
                }
            })
            .clone()
    }
}

/// Optimized VM operation recording with minimal allocations
pub fn record_vm_operation_optimized(operation: &'static str, success: bool) {
    let metrics = crate::metrics_otel::metrics();
    
    ATTR_CACHE.with(|cache| {
        let attrs = cache.borrow_mut().get_operation_attrs(operation, success);
        
        match operation {
            "create" => {
                metrics.vm_create_total.add(1, &attrs);
                if !success {
                    metrics.vm_create_failed.add(1, &attrs);
                }
            }
            "start" => {
                metrics.vm_start_total.add(1, &attrs);
                if !success {
                    metrics.vm_start_failed.add(1, &attrs);
                }
            }
            "stop" => {
                metrics.vm_stop_total.add(1, &attrs);
                if !success {
                    metrics.vm_stop_failed.add(1, &attrs);
                }
            }
            "delete" => {
                metrics.vm_delete_total.add(1, &attrs);
                if !success {
                    metrics.vm_delete_failed.add(1, &attrs);
                }
            }
            _ => {} // Unknown operation
        }
    });
}

/// Optimized node resource metrics update
pub fn update_node_resource_metrics_optimized(node_id: u64, usage: &crate::vm_scheduler::NodeResourceUsage) {
    let metrics = crate::metrics_otel::metrics();
    
    ATTR_CACHE.with(|cache| {
        let attrs = cache.borrow_mut().get_node_attrs(node_id);
        
        // Update per-node resource availability with cached attributes
        metrics.node_vcpus_available.add(usage.available_vcpus() as i64, &attrs);
        metrics.node_memory_mb_available.add(usage.available_memory_mb() as i64, &attrs);
        metrics.node_disk_gb_available.add(usage.available_disk_gb() as i64, &attrs);
    });
}

/// Optimized VM placement recording with static strategy attributes
pub fn record_vm_placement_attempt_optimized(strategy: &str, success: bool, duration_secs: f64) {
    let metrics = crate::metrics_otel::metrics();
    
    // Use pre-allocated strategy attributes when possible
    let strategy_attr = match strategy {
        "MostAvailable" => &*static_attrs::STRATEGY_MOST_AVAILABLE,
        "LeastAvailable" => &*static_attrs::STRATEGY_LEAST_AVAILABLE,
        "RoundRobin" => &*static_attrs::STRATEGY_ROUND_ROBIN,
        "Manual" => &*static_attrs::STRATEGY_MANUAL,
        _ => &KeyValue::new("strategy", strategy.to_string()),
    };
    
    let attrs = [strategy_attr.clone()];
    
    metrics.vm_placement_attempts.add(1, &attrs);
    if !success {
        metrics.vm_placement_failures.add(1, &attrs);
    }
    metrics.vm_placement_duration.record(duration_secs, &attrs);
}

/// Optimized P2P verification recording
pub fn record_p2p_verification_optimized(success: bool, is_nar_hash: bool) {
    let metrics = crate::metrics_otel::metrics();
    
    let type_attr = if is_nar_hash {
        &*static_attrs::VERIFY_NAR_HASH
    } else {
        &*static_attrs::VERIFY_CHUNK_HASH
    };
    
    if success {
        metrics.p2p_verification_success.add(1, &[type_attr.clone()]);
    } else {
        metrics.p2p_verification_failed.add(1, &[type_attr.clone()]);
    }
}

/// Optimized P2P cache access recording
pub fn record_p2p_cache_access_optimized(hit: bool, is_chunk_cache: bool) {
    let metrics = crate::metrics_otel::metrics();
    
    let type_attr = if is_chunk_cache {
        &*static_attrs::CACHE_CHUNK
    } else {
        &*static_attrs::CACHE_IMAGE
    };
    
    if hit {
        metrics.p2p_cache_hits.add(1, &[type_attr.clone()]);
    } else {
        metrics.p2p_cache_misses.add(1, &[type_attr.clone()]);
    }
}

/// Pre-allocated empty attribute slice for metrics without labels
static EMPTY_ATTRS: &[KeyValue] = &[];

/// Optimized cluster resource metrics update (no allocations)
pub fn update_cluster_resource_metrics_optimized(summary: &crate::vm_scheduler::ClusterResourceSummary) {
    let metrics = crate::metrics_otel::metrics();
    
    // Use empty attributes slice - no allocations
    metrics.cluster_nodes_total.add(summary.total_nodes as i64, EMPTY_ATTRS);
    metrics.cluster_vcpus_total.add(summary.total_vcpus as i64, EMPTY_ATTRS);
    metrics.cluster_vcpus_used.add(summary.used_vcpus as i64, EMPTY_ATTRS);
    metrics.cluster_memory_mb_total.add(summary.total_memory_mb as i64, EMPTY_ATTRS);
    metrics.cluster_memory_mb_used.add(summary.used_memory_mb as i64, EMPTY_ATTRS);
    metrics.cluster_disk_gb_total.add(summary.total_disk_gb as i64, EMPTY_ATTRS);
    metrics.cluster_disk_gb_used.add(summary.used_disk_gb as i64, EMPTY_ATTRS);
}

/// Fast attribute builders for hot paths
pub mod fast_attrs {
    use opentelemetry::KeyValue;
    
    /// Pre-sized vector for building attribute sets
    pub struct FastAttrBuilder {
        attrs: Vec<KeyValue>,
    }
    
    impl FastAttrBuilder {
        /// Create with expected capacity
        #[inline]
        pub fn with_capacity(cap: usize) -> Self {
            Self {
                attrs: Vec::with_capacity(cap),
            }
        }
        
        /// Add a node ID attribute
        #[inline]
        pub fn node_id(mut self, id: u64) -> Self {
            self.attrs.push(KeyValue::new("node_id", id as i64));
            self
        }
        
        /// Add a peer ID attribute
        #[inline]
        pub fn peer_id(mut self, id: u64) -> Self {
            self.attrs.push(KeyValue::new("peer_id", id as i64));
            self
        }
        
        /// Add a static attribute
        #[inline]
        pub fn static_attr(mut self, attr: &KeyValue) -> Self {
            self.attrs.push(attr.clone());
            self
        }
        
        /// Build the attribute slice
        #[inline]
        pub fn build(self) -> Vec<KeyValue> {
            self.attrs
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_static_attributes() {
        // Verify static attributes are initialized correctly
        assert_eq!(static_attrs::OPERATION_CREATE.key.as_str(), "operation");
        assert_eq!(static_attrs::ERROR_TRUE.key.as_str(), "error");
    }
    
    #[test]
    fn test_attribute_cache() {
        let mut cache = AttributeCache::new();
        
        // Test node attribute caching
        let attrs1 = cache.get_node_attrs(42);
        let attrs2 = cache.get_node_attrs(42);
        assert!(Arc::ptr_eq(&attrs1, &attrs2)); // Same Arc instance
        
        // Test VM attribute caching
        let vm1 = cache.get_vm_attrs("test-vm");
        let vm2 = cache.get_vm_attrs("test-vm");
        assert!(Arc::ptr_eq(&vm1, &vm2));
    }
}