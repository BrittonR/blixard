//! Memory-optimized VM scheduler operations
//!
//! This module demonstrates memory optimization techniques for VM scheduling,
//! which is a hot path that handles many VM configurations and placement decisions.

use crate::error::BlixardResult;
use crate::memory_optimization::{
    string_pool::{intern, InternedString},
    collection_utils::{FlatMap, SmallVec, PreallocateCapacity},
    arc_optimizer::ArcCache,
};
use crate::types::{VmConfig, VmId, NodeTopology};
use crate::vm_scheduler_modules::placement_strategies::PlacementDecision;
use std::sync::Arc;
use std::borrow::Cow;

/// Optimized VM configuration that reduces string allocations
#[derive(Debug, Clone)]
pub struct OptimizedVmConfig {
    pub id: VmId,
    pub name: InternedString,
    pub tenant_id: InternedString,
    pub config_path: Arc<str>,
    pub vcpus: u32,
    pub memory: u32,
    pub metadata: Option<Arc<FlatMap<InternedString, String>>>,
    pub ip_address: Option<Arc<str>>,
    pub priority: u32,
    pub preemptible: bool,
}

impl OptimizedVmConfig {
    /// Create from regular VmConfig with optimizations
    pub fn from_config(config: &VmConfig) -> Self {
        Self {
            id: VmId::from_string(&config.name),
            name: intern(&config.name),
            tenant_id: intern(&config.tenant_id),
            config_path: Arc::from(config.config_path.as_str()),
            vcpus: config.vcpus,
            memory: config.memory,
            metadata: config.metadata.as_ref().map(|m| {
                let mut optimized = FlatMap::with_capacity_headroom(m.len());
                for (k, v) in m {
                    optimized.insert(intern(k), v.clone());
                }
                Arc::new(optimized)
            }),
            ip_address: config.ip_address.as_ref().map(|ip| Arc::from(ip.as_str())),
            priority: config.priority,
            preemptible: config.preemptible,
        }
    }
    
    /// Convert back to regular VmConfig (allocates)
    pub fn to_config(&self) -> VmConfig {
        VmConfig {
            name: self.name.to_string(),
            config_path: self.config_path.to_string(),
            vcpus: self.vcpus,
            memory: self.memory,
            tenant_id: self.tenant_id.to_string(),
            ip_address: self.ip_address.as_ref().map(|s| s.to_string()),
            metadata: self.metadata.as_ref().map(|m| {
                m.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
            }),
            anti_affinity: None,
            priority: self.priority,
            preemptible: self.preemptible,
            locality_preference: Default::default(),
            health_check_config: None,
        }
    }
}

/// Cache for frequently accessed VM configurations
pub struct VmConfigCache {
    cache: ArcCache<VmId, OptimizedVmConfig>,
    tenant_cache: FlatMap<InternedString, Vec<VmId>>,
}

impl VmConfigCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: ArcCache::new(max_size),
            tenant_cache: FlatMap::default(),
        }
    }
    
    /// Get or create optimized config
    pub fn get_or_create(&mut self, config: &VmConfig) -> Arc<OptimizedVmConfig> {
        let vm_id = VmId::from_string(&config.name);
        
        self.cache.get_or_create(&vm_id, || {
            let optimized = OptimizedVmConfig::from_config(config);
            
            // Update tenant cache
            self.tenant_cache
                .entry(optimized.tenant_id.clone())
                .or_insert_with(|| Vec::with_capacity(10))
                .push(vm_id);
            
            optimized
        })
    }
    
    /// Get all VMs for a tenant (no allocation)
    pub fn get_tenant_vms(&self, tenant: &str) -> Option<&[VmId]> {
        let tenant_interned = intern(tenant);
        self.tenant_cache.get(&tenant_interned).map(|v| v.as_slice())
    }
}

/// Memory-optimized placement decision builder
pub struct OptimizedPlacementBuilder {
    /// Pre-allocated string builder for reasons
    reason_parts: Vec<Cow<'static, str>>,
    /// Cache of common reason strings
    reason_cache: FlatMap<u64, Arc<str>>,
}

impl OptimizedPlacementBuilder {
    pub fn new() -> Self {
        Self {
            reason_parts: Vec::with_capacity(5),
            reason_cache: FlatMap::default(),
        }
    }
    
    /// Build placement decision with minimal allocations
    pub fn build_decision(
        &mut self,
        node_id: u64,
        strategy: &str,
        score: f64,
        alternatives: SmallVec<u64>,
    ) -> PlacementDecision {
        // Build reason string efficiently
        self.reason_parts.clear();
        self.reason_parts.push(Cow::Borrowed("Selected node "));
        self.reason_parts.push(Cow::Owned(node_id.to_string()));
        self.reason_parts.push(Cow::Borrowed(" using "));
        self.reason_parts.push(Cow::Owned(strategy.to_string()));
        self.reason_parts.push(Cow::Borrowed(" strategy"));
        
        let reason = self.reason_cache.entry(node_id).or_insert_with(|| {
            let mut reason = String::with_capacity(
                self.reason_parts.iter().map(|s| s.len()).sum()
            );
            for part in &self.reason_parts {
                reason.push_str(part);
            }
            Arc::from(reason.as_str())
        }).to_string();
        
        PlacementDecision {
            target_node_id: node_id,
            selected_node_id: node_id, // Alias
            strategy_used: strategy.to_string(),
            confidence_score: score,
            preempted_vms: Vec::new(),
            resource_fit_score: score,
            alternative_nodes: alternatives.to_vec(),
            reason,
        }
    }
}

/// Batch VM scheduling with optimized memory usage
pub struct BatchVmScheduler {
    config_cache: VmConfigCache,
    placement_builder: OptimizedPlacementBuilder,
    /// Reusable buffer for node scores
    node_scores: Vec<(u64, f64)>,
    /// Reusable buffer for topology lookups
    topology_buffer: FlatMap<u64, Arc<NodeTopology>>,
}

impl BatchVmScheduler {
    pub fn new(cache_size: usize) -> Self {
        Self {
            config_cache: VmConfigCache::new(cache_size),
            placement_builder: OptimizedPlacementBuilder::new(),
            node_scores: Vec::with_capacity(100),
            topology_buffer: FlatMap::default(),
        }
    }
    
    /// Schedule multiple VMs with batched operations
    pub async fn schedule_batch(
        &mut self,
        configs: &[VmConfig],
        available_nodes: &[(u64, u32, u32)], // (node_id, available_cpu, available_memory)
    ) -> Vec<BlixardResult<PlacementDecision>> {
        // Pre-size result vector
        let mut results = Vec::with_capacity(configs.len());
        
        // Process each VM config
        for config in configs {
            // Get cached config
            let optimized = self.config_cache.get_or_create(config);
            
            // Find suitable nodes (reuse buffer)
            self.node_scores.clear();
            for &(node_id, avail_cpu, avail_mem) in available_nodes {
                if avail_cpu >= optimized.vcpus && avail_mem >= optimized.memory {
                    // Simple scoring: available resources
                    let score = (avail_cpu as f64 / optimized.vcpus as f64) 
                              + (avail_mem as f64 / optimized.memory as f64);
                    self.node_scores.push((node_id, score));
                }
            }
            
            // Select best node
            if let Some(&(best_node, best_score)) = self.node_scores
                .iter()
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            {
                // Get alternative nodes (top 3)
                let mut alternatives = SmallVec::new();
                let mut sorted_scores = self.node_scores.clone();
                sorted_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                
                for &(node_id, _) in sorted_scores.iter().skip(1).take(3) {
                    alternatives.push(node_id);
                }
                
                let decision = self.placement_builder.build_decision(
                    best_node,
                    "MostAvailable",
                    best_score,
                    alternatives,
                );
                
                results.push(Ok(decision));
            } else {
                results.push(Err(crate::error::BlixardError::ResourceExhausted {
                    resource: format!("No node can accommodate VM {}", optimized.name),
                }));
            }
        }
        
        results
    }
}

/// Example showing memory-optimized VM operations
pub async fn optimized_scheduling_example() -> BlixardResult<()> {
    // Initialize object pools
    crate::memory_optimization::object_pool::global_pools::VM_CONFIG_POOL
        .initialize()
        .await?;
    
    // Create batch scheduler
    let mut scheduler = BatchVmScheduler::new(1000);
    
    // Example VMs to schedule
    let configs = vec![
        VmConfig {
            name: "web-1".to_string(),
            vcpus: 2,
            memory: 2048,
            tenant_id: "tenant-a".to_string(),
            ..Default::default()
        },
        VmConfig {
            name: "web-2".to_string(),
            vcpus: 2,
            memory: 2048,
            tenant_id: "tenant-a".to_string(), // Same tenant, will reuse interned string
            ..Default::default()
        },
        VmConfig {
            name: "db-1".to_string(),
            vcpus: 4,
            memory: 8192,
            tenant_id: "tenant-b".to_string(),
            ..Default::default()
        },
    ];
    
    // Available nodes
    let nodes = vec![
        (1, 8, 16384),  // node 1: 8 CPUs, 16GB RAM available
        (2, 4, 8192),   // node 2: 4 CPUs, 8GB RAM available
        (3, 16, 32768), // node 3: 16 CPUs, 32GB RAM available
    ];
    
    // Schedule with minimal allocations
    let decisions = scheduler.schedule_batch(&configs, &nodes).await;
    
    // Show results
    for (config, result) in configs.iter().zip(decisions.iter()) {
        match result {
            Ok(decision) => {
                println!("VM {} scheduled on node {} (score: {:.2})",
                    config.name, decision.target_node_id, decision.confidence_score);
            }
            Err(e) => {
                println!("Failed to schedule VM {}: {}", config.name, e);
            }
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_optimized_vm_config() {
        let config = VmConfig {
            name: "test-vm".to_string(),
            vcpus: 2,
            memory: 1024,
            tenant_id: "test-tenant".to_string(),
            ..Default::default()
        };
        
        let optimized = OptimizedVmConfig::from_config(&config);
        assert_eq!(optimized.name.as_str(), "test-vm");
        assert_eq!(optimized.tenant_id.as_str(), "test-tenant");
        
        // Convert back
        let restored = optimized.to_config();
        assert_eq!(restored.name, config.name);
        assert_eq!(restored.tenant_id, config.tenant_id);
    }
    
    #[test]
    fn test_vm_config_cache() {
        let mut cache = VmConfigCache::new(10);
        
        let config1 = VmConfig {
            name: "vm1".to_string(),
            tenant_id: "tenant1".to_string(),
            ..Default::default()
        };
        
        let arc1 = cache.get_or_create(&config1);
        let arc2 = cache.get_or_create(&config1);
        
        // Should return same Arc
        assert!(Arc::ptr_eq(&arc1, &arc2));
        
        // Check tenant cache
        let tenant_vms = cache.get_tenant_vms("tenant1").unwrap();
        assert_eq!(tenant_vms.len(), 1);
    }
    
    #[tokio::test]
    async fn test_batch_scheduler() {
        let mut scheduler = BatchVmScheduler::new(100);
        
        let configs = vec![
            VmConfig {
                name: "small-vm".to_string(),
                vcpus: 1,
                memory: 512,
                ..Default::default()
            },
            VmConfig {
                name: "large-vm".to_string(),
                vcpus: 8,
                memory: 16384,
                ..Default::default()
            },
        ];
        
        let nodes = vec![
            (1, 2, 1024),   // Small node
            (2, 16, 32768), // Large node
        ];
        
        let results = scheduler.schedule_batch(&configs, &nodes).await;
        
        assert_eq!(results.len(), 2);
        assert!(results[0].is_ok());
        assert!(results[1].is_ok());
        
        // Small VM should go to either node
        // Large VM should only fit on node 2
        assert_eq!(results[1].as_ref().unwrap().target_node_id, 2);
    }
}