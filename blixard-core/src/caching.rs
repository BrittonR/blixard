//! Caching layer for frequently computed values
//!
//! This module provides caching mechanisms to avoid recomputing
//! expensive operations in hot paths.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Cached value with TTL
#[derive(Debug, Clone)]
pub struct CachedValue<T> {
    value: T,
    cached_at: Instant,
    ttl: Duration,
}

impl<T> CachedValue<T> {
    fn new(value: T, ttl: Duration) -> Self {
        Self {
            value,
            cached_at: Instant::now(),
            ttl,
        }
    }
    
    fn is_valid(&self) -> bool {
        self.cached_at.elapsed() < self.ttl
    }
    
    fn get(&self) -> &T {
        &self.value
    }
}

/// Generic cache with TTL support
pub struct TtlCache<K, V> 
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    cache: RwLock<HashMap<K, CachedValue<V>>>,
    default_ttl: Duration,
}

impl<K, V> TtlCache<K, V>
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    /// Create a new TTL cache
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            default_ttl,
        }
    }
    
    /// Get a value from cache or compute it
    pub async fn get_or_compute<F, Fut>(&self, key: K, compute: F) -> V
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = V>,
    {
        // Fast path: check if cached value exists and is valid
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(&key) {
                if cached.is_valid() {
                    return cached.get().clone();
                }
            }
        }
        
        // Slow path: compute and cache
        let value = compute().await;
        let cached_value = CachedValue::new(value.clone(), self.default_ttl);
        
        {
            let mut cache = self.cache.write().await;
            cache.insert(key, cached_value);
        }
        
        value
    }
    
    /// Invalidate a specific key
    pub async fn invalidate(&self, key: &K) {
        let mut cache = self.cache.write().await;
        cache.remove(key);
    }
    
    /// Clear all cached values
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
    
    /// Clean up expired entries
    pub async fn cleanup_expired(&self) {
        let mut cache = self.cache.write().await;
        cache.retain(|_, cached| cached.is_valid());
    }
}

/// Specialized cache for cluster resource summaries
pub struct ResourceSummaryCache {
    cache: TtlCache<u64, ClusterResourceSummary>,
}

impl ResourceSummaryCache {
    pub fn new() -> Self {
        Self {
            cache: TtlCache::new(Duration::from_secs(30)), // 30 second TTL
        }
    }
    
    pub async fn get_cluster_summary<F, Fut>(&self, node_id: u64, compute: F) -> ClusterResourceSummary
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ClusterResourceSummary>,
    {
        self.cache.get_or_compute(node_id, compute).await
    }
}

/// Cached cluster resource summary to avoid recomputation
#[derive(Debug, Clone)]
pub struct ClusterResourceSummary {
    pub total_nodes: u32,
    pub healthy_nodes: u32,
    pub total_vcpus: u32,
    pub used_vcpus: u32,
    pub total_memory_mb: u64,
    pub used_memory_mb: u64,
    pub total_disk_gb: u64,
    pub used_disk_gb: u64,
    pub computed_at: Instant,
}

/// Cache for VM placement decisions
pub struct PlacementDecisionCache {
    cache: TtlCache<PlacementKey, PlacementResult>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct PlacementKey {
    pub vm_requirements: VmRequirements,
    pub cluster_state_hash: u64,
}

#[derive(Debug, Clone)]
pub struct VmRequirements {
    pub vcpus: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub features: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PlacementResult {
    pub target_node: Option<u64>,
    pub confidence_score: f64,
    pub reason: String,
}

impl PlacementDecisionCache {
    pub fn new() -> Self {
        Self {
            cache: TtlCache::new(Duration::from_secs(10)), // Short TTL for placement decisions
        }
    }
    
    pub async fn get_placement<F, Fut>(&self, key: PlacementKey, compute: F) -> PlacementResult
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = PlacementResult>,
    {
        self.cache.get_or_compute(key, compute).await
    }
}

/// Cache for peer connection information
pub struct PeerConnectionCache {
    cache: TtlCache<u64, PeerConnectionInfo>,
}

#[derive(Debug, Clone)]
pub struct PeerConnectionInfo {
    pub node_id: u64,
    pub is_connected: bool,
    pub last_seen: Instant,
    pub connection_quality: ConnectionQuality,
    pub round_trip_time: Duration,
}

#[derive(Debug, Clone)]
pub enum ConnectionQuality {
    Excellent,
    Good,
    Fair,
    Poor,
    Disconnected,
}

impl PeerConnectionCache {
    pub fn new() -> Self {
        Self {
            cache: TtlCache::new(Duration::from_secs(5)), // Very short TTL for connection info
        }
    }
    
    pub async fn get_connection_info<F, Fut>(&self, node_id: u64, compute: F) -> PeerConnectionInfo
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = PeerConnectionInfo>,
    {
        self.cache.get_or_compute(node_id, compute).await
    }
}

/// Atomic counters for frequently accessed metrics
pub struct AtomicMetricsCache {
    pub vm_count: std::sync::atomic::AtomicU64,
    pub running_vm_count: std::sync::atomic::AtomicU64,
    pub cluster_node_count: std::sync::atomic::AtomicU64,
    pub total_vcpus: std::sync::atomic::AtomicU64,
    pub used_vcpus: std::sync::atomic::AtomicU64,
    pub total_memory_mb: std::sync::atomic::AtomicU64,
    pub used_memory_mb: std::sync::atomic::AtomicU64,
}

impl AtomicMetricsCache {
    pub fn new() -> Self {
        Self {
            vm_count: std::sync::atomic::AtomicU64::new(0),
            running_vm_count: std::sync::atomic::AtomicU64::new(0),
            cluster_node_count: std::sync::atomic::AtomicU64::new(0),
            total_vcpus: std::sync::atomic::AtomicU64::new(0),
            used_vcpus: std::sync::atomic::AtomicU64::new(0),
            total_memory_mb: std::sync::atomic::AtomicU64::new(0),
            used_memory_mb: std::sync::atomic::AtomicU64::new(0),
        }
    }
    
    /// Get VM count without locking
    pub fn vm_count(&self) -> u64 {
        self.vm_count.load(std::sync::atomic::Ordering::Acquire)
    }
    
    /// Increment VM count atomically
    pub fn increment_vm_count(&self) {
        self.vm_count.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    }
    
    /// Decrement VM count atomically
    pub fn decrement_vm_count(&self) {
        self.vm_count.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
    
    /// Get running VM count without locking
    pub fn running_vm_count(&self) -> u64 {
        self.running_vm_count.load(std::sync::atomic::Ordering::Acquire)
    }
    
    /// Update running VM count
    pub fn set_running_vm_count(&self, count: u64) {
        self.running_vm_count.store(count, std::sync::atomic::Ordering::Release);
    }
    
    /// Get cluster node count without locking
    pub fn cluster_node_count(&self) -> u64 {
        self.cluster_node_count.load(std::sync::atomic::Ordering::Acquire)
    }
    
    /// Update cluster node count
    pub fn set_cluster_node_count(&self, count: u64) {
        self.cluster_node_count.store(count, std::sync::atomic::Ordering::Release);
    }
    
    /// Get total vCPUs
    pub fn total_vcpus(&self) -> u64 {
        self.total_vcpus.load(std::sync::atomic::Ordering::Acquire)
    }
    
    /// Update total vCPUs
    pub fn set_total_vcpus(&self, count: u64) {
        self.total_vcpus.store(count, std::sync::atomic::Ordering::Release);
    }
    
    /// Get used vCPUs
    pub fn used_vcpus(&self) -> u64 {
        self.used_vcpus.load(std::sync::atomic::Ordering::Acquire)
    }
    
    /// Update used vCPUs
    pub fn add_used_vcpus(&self, count: u64) {
        self.used_vcpus.fetch_add(count, std::sync::atomic::Ordering::AcqRel);
    }
    
    /// Remove used vCPUs
    pub fn remove_used_vcpus(&self, count: u64) {
        self.used_vcpus.fetch_sub(count, std::sync::atomic::Ordering::AcqRel);
    }
}

/// Global cache manager for all performance caches
pub struct GlobalCacheManager {
    pub resource_summary: ResourceSummaryCache,
    pub placement_decisions: PlacementDecisionCache,
    pub peer_connections: PeerConnectionCache,
    pub atomic_metrics: AtomicMetricsCache,
}

impl GlobalCacheManager {
    pub fn new() -> Self {
        Self {
            resource_summary: ResourceSummaryCache::new(),
            placement_decisions: PlacementDecisionCache::new(),
            peer_connections: PeerConnectionCache::new(),
            atomic_metrics: AtomicMetricsCache::new(),
        }
    }
    
    /// Clean up all expired cache entries
    pub async fn cleanup_expired(&self) {
        self.resource_summary.cache.cleanup_expired().await;
        self.placement_decisions.cache.cleanup_expired().await;
        self.peer_connections.cache.cleanup_expired().await;
    }
    
    /// Clear all caches
    pub async fn clear_all(&self) {
        self.resource_summary.cache.clear().await;
        self.placement_decisions.cache.clear().await;
        self.peer_connections.cache.clear().await;
    }
}

/// Global cache instance
static GLOBAL_CACHE: std::sync::OnceLock<GlobalCacheManager> = std::sync::OnceLock::new();

/// Get the global cache manager
pub fn global_cache() -> &'static GlobalCacheManager {
    GLOBAL_CACHE.get_or_init(|| GlobalCacheManager::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    
    #[tokio::test]
    async fn test_ttl_cache() {
        let cache = TtlCache::new(Duration::from_millis(100));
        
        // First computation
        let value1 = cache.get_or_compute("key1", || async { "computed_value" }).await;
        assert_eq!(value1, "computed_value");
        
        // Should return cached value
        let value2 = cache.get_or_compute("key1", || async { "new_value" }).await;
        assert_eq!(value2, "computed_value");
        
        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(150)).await;
        
        // Should recompute after expiration
        let value3 = cache.get_or_compute("key1", || async { "new_value" }).await;
        assert_eq!(value3, "new_value");
    }
    
    #[test]
    fn test_atomic_metrics_cache() {
        let cache = AtomicMetricsCache::new();
        
        assert_eq!(cache.vm_count(), 0);
        cache.increment_vm_count();
        assert_eq!(cache.vm_count(), 1);
        cache.increment_vm_count();
        assert_eq!(cache.vm_count(), 2);
        cache.decrement_vm_count();
        assert_eq!(cache.vm_count(), 1);
    }
}