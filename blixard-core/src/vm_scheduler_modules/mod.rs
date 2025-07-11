pub mod placement_strategies;
pub mod resource_analysis;

use redb::Database;
use std::sync::Arc;
use std::collections::HashMap;

use crate::error::BlixardResult;
#[cfg(feature = "observability")]
use crate::metrics_otel;
use crate::resource_management::{ClusterResourceManager, OvercommitPolicy};
use crate::types::VmConfig;

// Re-export types from submodules
pub use placement_strategies::*;
pub use resource_analysis::*;

/// VM Scheduler for automatic placement of VMs across cluster nodes
pub struct VmScheduler {
    database: Arc<Database>,
    resource_manager: Arc<tokio::sync::RwLock<ClusterResourceManager>>,
    datacenter_latencies: Arc<tokio::sync::RwLock<DatacenterLatencyMap>>,
}

impl VmScheduler {
    /// Create a new VM scheduler
    pub fn new(database: Arc<Database>) -> Self {
        Self {
            database,
            resource_manager: Arc::new(tokio::sync::RwLock::new(ClusterResourceManager::new(
                OvercommitPolicy::moderate(),
            ))),
            datacenter_latencies: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }

    /// Create a new VM scheduler with custom overcommit policy
    pub fn with_overcommit_policy(database: Arc<Database>, policy: OvercommitPolicy) -> Self {
        Self {
            database,
            resource_manager: Arc::new(tokio::sync::RwLock::new(ClusterResourceManager::new(
                policy,
            ))),
            datacenter_latencies: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }

    /// Update network latency between datacenters
    pub async fn update_datacenter_latency(&self, dc1: &str, dc2: &str, latency_ms: u32) {
        let mut latencies = self.datacenter_latencies.write().await;
        // Store bidirectionally
        latencies.insert((dc1.to_string(), dc2.to_string()), latency_ms);
        latencies.insert((dc2.to_string(), dc1.to_string()), latency_ms);
    }

    /// Get network latency between two datacenters
    pub async fn get_datacenter_latency(&self, dc1: &str, dc2: &str) -> Option<u32> {
        let latencies = self.datacenter_latencies.read().await;
        latencies.get(&(dc1.to_string(), dc2.to_string())).copied()
    }

    /// Schedule VM placement using the given strategy
    pub async fn schedule_vm_placement(
        &self,
        vm_config: &VmConfig,
        strategy: PlacementStrategy,
    ) -> BlixardResult<PlacementDecision> {
        let start_time = std::time::Instant::now();
        let requirements = VmResourceRequirements::from(vm_config);
        let strategy_name = format!("{:?}", strategy);

        tracing::info!(
            "Scheduling VM '{}' with strategy: {}",
            vm_config.name,
            strategy_name
        );

        // Collect cluster state
        let context = self.collect_cluster_state(
            requirements, 
            strategy_name.clone(), 
            start_time, 
            vm_config
        ).await?;

        // Apply the placement strategy
        let decision = self.apply_placement_strategy(&context, vm_config, strategy).await?;

        // Record metrics
        let duration = start_time.elapsed();
        #[cfg(feature = "observability")]
        crate::metrics_otel::record_vm_scheduling_decision(&vm_config.name, &strategy_name, &decision, duration);

        tracing::info!(
            "VM '{}' scheduled to node {} using strategy {} (took {}ms)",
            vm_config.name,
            decision.target_node_id,
            strategy_name,
            duration.as_millis()
        );

        Ok(decision)
    }

    /// Get cluster-wide resource summary
    pub async fn get_cluster_resource_summary(&self) -> BlixardResult<ClusterResourceSummary> {
        let node_usage = self.get_cluster_resource_usage().await?;
        
        if node_usage.is_empty() {
            return Ok(ClusterResourceSummary {
                total_nodes: 0,
                healthy_nodes: 0,
                total_vcpus: 0,
                used_vcpus: 0,
                total_memory_mb: 0,
                used_memory_mb: 0,
                total_disk_gb: 0,
                used_disk_gb: 0,
                average_cpu_utilization: 0.0,
                average_memory_utilization: 0.0,
                average_disk_utilization: 0.0,
            });
        }

        let total_nodes = node_usage.len() as u32;
        let healthy_nodes = node_usage.iter().filter(|n| n.is_healthy).count() as u32;
        
        let total_vcpus = node_usage.iter().map(|n| n.capabilities.cpu_cores).sum();
        let used_vcpus = node_usage.iter().map(|n| n.used_vcpus).sum();
        
        let total_memory_mb = node_usage.iter().map(|n| n.capabilities.memory_mb).sum();
        let used_memory_mb = node_usage.iter().map(|n| n.used_memory_mb).sum();
        
        let total_disk_gb = node_usage.iter().map(|n| n.capabilities.disk_gb).sum();
        let used_disk_gb = node_usage.iter().map(|n| n.used_disk_gb).sum();

        let avg_cpu_util = if total_vcpus > 0 {
            (used_vcpus as f64 / total_vcpus as f64) * 100.0
        } else { 0.0 };
        
        let avg_mem_util = if total_memory_mb > 0 {
            (used_memory_mb as f64 / total_memory_mb as f64) * 100.0
        } else { 0.0 };
        
        let avg_disk_util = if total_disk_gb > 0 {
            (used_disk_gb as f64 / total_disk_gb as f64) * 100.0
        } else { 0.0 };

        Ok(ClusterResourceSummary {
            total_nodes,
            healthy_nodes,
            total_vcpus,
            used_vcpus,
            total_memory_mb,
            used_memory_mb,
            total_disk_gb,
            used_disk_gb,
            average_cpu_utilization: avg_cpu_util,
            average_memory_utilization: avg_mem_util,
            average_disk_utilization: avg_disk_util,
        })
    }
}