use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use redb::ReadableTable;

use crate::error::{BlixardError, BlixardResult};
use crate::raft_manager::{WorkerCapabilities, WorkerStatus};
use crate::raft_storage::{VM_STATE_TABLE, WORKER_STATUS_TABLE, WORKER_TABLE};
use crate::types::{VmConfig, NodeTopology};
use crate::anti_affinity::AntiAffinityChecker;

/// Resource requirements for VM placement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmResourceRequirements {
    pub vcpus: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub required_features: Vec<String>,
}

impl From<&VmConfig> for VmResourceRequirements {
    fn from(config: &VmConfig) -> Self {
        Self {
            vcpus: config.vcpus,
            memory_mb: config.memory as u64,
            disk_gb: 5, // Default disk requirement
            required_features: vec!["microvm".to_string()],
        }
    }
}

/// Resource usage for a single cluster node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResourceUsage {
    pub node_id: u64,
    pub is_healthy: bool,
    pub capabilities: WorkerCapabilities,
    pub used_vcpus: u32,
    pub used_memory_mb: u64,
    pub used_disk_gb: u64,
    pub running_vms: u32,
    pub topology: Option<NodeTopology>,
    pub cost_per_hour: Option<f64>,
    pub current_utilization: NodeUtilization,
}

/// Current utilization percentages for a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeUtilization {
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub disk_percent: f64,
}

/// Cluster-wide resource summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterResourceSummary {
    pub total_nodes: u32,
    pub healthy_nodes: u32,
    pub total_vcpus: u32,
    pub used_vcpus: u32,
    pub total_memory_mb: u64,
    pub used_memory_mb: u64,
    pub total_disk_gb: u64,
    pub used_disk_gb: u64,
    pub average_cpu_utilization: f64,
    pub average_memory_utilization: f64,
    pub average_disk_utilization: f64,
}

/// Intermediate state for scheduling operations
#[derive(Debug, Clone)]
pub struct SchedulingContext {
    pub requirements: VmResourceRequirements,
    pub strategy_name: String,
    pub start_time: std::time::Instant,
    pub node_usage: Vec<NodeResourceUsage>,
    pub anti_affinity_checker: Option<AntiAffinityChecker>,
}

/// Network latency between datacenters (in milliseconds)
pub type DatacenterLatencyMap = HashMap<(String, String), u32>;

impl NodeResourceUsage {
    /// Check if this node can accommodate the given VM requirements
    pub fn can_accommodate(&self, requirements: &VmResourceRequirements) -> bool {
        if !self.is_healthy {
            return false;
        }

        // Check CPU
        let available_vcpus = self.capabilities.cpu_cores.saturating_sub(self.used_vcpus);
        if available_vcpus < requirements.vcpus {
            return false;
        }

        // Check memory
        let available_memory = self.capabilities.memory_mb.saturating_sub(self.used_memory_mb);
        if available_memory < requirements.memory_mb {
            return false;
        }

        // Check disk
        let available_disk = self.capabilities.disk_gb.saturating_sub(self.used_disk_gb);
        if available_disk < requirements.disk_gb {
            return false;
        }

        // Check required features
        for required_feature in &requirements.required_features {
            if !self.capabilities.features.contains(required_feature) {
                return false;
            }
        }

        true
    }

    /// Calculate a score for this node based on available resources
    pub fn calculate_availability_score(&self) -> f64 {
        if !self.is_healthy {
            return 0.0;
        }

        // Calculate available resources as percentages
        let cpu_available = if self.capabilities.cpu_cores > 0 {
            (self.capabilities.cpu_cores - self.used_vcpus) as f64 / self.capabilities.cpu_cores as f64
        } else {
            0.0
        };

        let memory_available = if self.capabilities.memory_mb > 0 {
            (self.capabilities.memory_mb - self.used_memory_mb) as f64 / self.capabilities.memory_mb as f64
        } else {
            0.0
        };

        let disk_available = if self.capabilities.disk_gb > 0 {
            (self.capabilities.disk_gb - self.used_disk_gb) as f64 / self.capabilities.disk_gb as f64
        } else {
            0.0
        };

        // Weighted average (CPU and memory are more important than disk)
        (cpu_available * 0.4 + memory_available * 0.4 + disk_available * 0.2) * 100.0
    }

    /// Update current utilization percentages
    pub fn update_utilization(&mut self) {
        self.current_utilization = NodeUtilization {
            cpu_percent: if self.capabilities.cpu_cores > 0 {
                (self.used_vcpus as f64 / self.capabilities.cpu_cores as f64) * 100.0
            } else {
                0.0
            },
            memory_percent: if self.capabilities.memory_mb > 0 {
                (self.used_memory_mb as f64 / self.capabilities.memory_mb as f64) * 100.0
            } else {
                0.0
            },
            disk_percent: if self.capabilities.disk_gb > 0 {
                (self.used_disk_gb as f64 / self.capabilities.disk_gb as f64) * 100.0
            } else {
                0.0
            },
        };
    }
}

/// Implementation for the VmScheduler's resource analysis methods
impl super::VmScheduler {
    /// Collect cluster state and prepare scheduling context
    pub(super) async fn collect_cluster_state(
        &self,
        requirements: VmResourceRequirements,
        strategy_name: String,
        start_time: std::time::Instant,
        vm_config: &VmConfig,
    ) -> BlixardResult<SchedulingContext> {
        // Get current cluster resource usage
        let node_usage = self.get_cluster_resource_usage().await?;

        if node_usage.is_empty() {
            return Err(BlixardError::SchedulingError {
                message: "No healthy worker nodes available for VM placement".to_string(),
            });
        }

        // Build anti-affinity checker if rules are specified
        let anti_affinity_checker = if vm_config.anti_affinity.is_some() {
            Some(self.build_anti_affinity_checker().await?)
        } else {
            None
        };

        Ok(SchedulingContext {
            requirements,
            strategy_name,
            start_time,
            node_usage,
            anti_affinity_checker,
        })
    }

    /// Get current resource usage for all nodes in the cluster
    pub async fn get_cluster_resource_usage(&self) -> BlixardResult<Vec<NodeResourceUsage>> {
        let read_txn = self.database.begin_read().map_err(|e| BlixardError::Storage {
            operation: "begin read transaction".to_string(),
            source: Box::new(e),
        })?;

        // Get all workers and their capabilities
        let worker_table = read_txn.open_table(WORKER_TABLE).map_err(|e| BlixardError::Storage {
            operation: "open worker table".to_string(),
            source: Box::new(e),
        })?;

        // Get worker status
        let status_table = read_txn.open_table(WORKER_STATUS_TABLE).map_err(|e| BlixardError::Storage {
            operation: "open worker status table".to_string(),
            source: Box::new(e),
        })?;

        // Get VM assignments to calculate resource usage
        let vm_table = read_txn.open_table(VM_STATE_TABLE).map_err(|e| BlixardError::Storage {
            operation: "open VM state table".to_string(),
            source: Box::new(e),
        })?;

        let mut node_usage = Vec::new();

        for entry in worker_table.iter().map_err(|e| BlixardError::Storage {
            operation: "iterate workers".to_string(),
            source: Box::new(e),
        })? {
            let (node_id_bytes, worker_data) = entry.map_err(|e| BlixardError::Storage {
                operation: "read worker entry".to_string(),
                source: Box::new(e),
            })?;

            let node_id = u64::from_be_bytes(
                node_id_bytes.value().try_into().map_err(|_| {
                    BlixardError::Serialization {
                        operation: "parse node ID".to_string(),
                        source: Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "Invalid node ID format",
                        )),
                    }
                })?,
            );

            let capabilities: WorkerCapabilities = bincode::deserialize(worker_data.value())
                .map_err(|e| BlixardError::serialization(
                    "deserialize worker capabilities",
                    e
                ))?;

            // Get worker status
            let node_id_bytes = node_id.to_be_bytes();
            let is_healthy = if let Ok(Some(status_data)) = status_table.get(&node_id_bytes[..]) {
                let status: WorkerStatus = bincode::deserialize(status_data.value())
                    .map_err(|e| BlixardError::serialization(
                        "deserialize worker status",
                        e
                    ))?;
                matches!(status, crate::raft::proposals::WorkerStatus::Online)
            } else {
                false // No status means not healthy
            };

            // Calculate current resource usage from VMs
            let (used_vcpus, used_memory_mb, used_disk_gb, running_vms) = 
                self.calculate_node_resource_usage(node_id, &vm_table)?;

            let mut usage = NodeResourceUsage {
                node_id,
                is_healthy,
                capabilities,
                used_vcpus,
                used_memory_mb,
                used_disk_gb,
                running_vms,
                topology: None, // TODO: Load from NODE_TOPOLOGY_TABLE if needed
                cost_per_hour: None, // TODO: Load from configuration
                current_utilization: NodeUtilization {
                    cpu_percent: 0.0,
                    memory_percent: 0.0,
                    disk_percent: 0.0,
                },
            };

            usage.update_utilization();
            node_usage.push(usage);
        }

        Ok(node_usage)
    }

    /// Calculate resource usage for a specific node from its running VMs
    fn calculate_node_resource_usage(
        &self,
        node_id: u64,
        vm_table: &redb::ReadOnlyTable<&str, &[u8]>,
    ) -> BlixardResult<(u32, u64, u64, u32)> 
    {
        let mut used_vcpus = 0;
        let mut used_memory_mb = 0;
        let mut used_disk_gb = 0;
        let mut running_vms = 0;

        for entry in vm_table.iter().map_err(|e| BlixardError::Storage {
            operation: "iterate VMs".to_string(),
            source: Box::new(e),
        })? {
            let (_, vm_data) = entry.map_err(|e| BlixardError::Storage {
                operation: "read VM entry".to_string(),
                source: Box::new(e),
            })?;

            let vm_config: VmConfig = bincode::deserialize(vm_data.value())
                .map_err(|e| BlixardError::Serialization {
                    operation: "deserialize VM config".to_string(),
                    source: Box::new(e),
                })?;

            // TODO: VM placement tracking - assigned_node field doesn't exist in VmConfig
            // For now, assume all VMs are assigned to count resource usage
            // This is temporary until proper placement tracking is implemented
            used_vcpus += vm_config.vcpus;
            used_memory_mb += vm_config.memory as u64;
            used_disk_gb += 5; // Default disk usage per VM
            running_vms += 1;
        }

        Ok((used_vcpus, used_memory_mb, used_disk_gb, running_vms))
    }

    /// Build anti-affinity checker for constraint validation
    pub(super) async fn build_anti_affinity_checker(&self) -> BlixardResult<AntiAffinityChecker> {
        let read_txn = self.database.begin_read().map_err(|e| BlixardError::Storage {
            operation: "begin read transaction".to_string(),
            source: Box::new(e),
        })?;

        let vm_table = read_txn.open_table(VM_STATE_TABLE).map_err(|e| BlixardError::Storage {
            operation: "open VM state table".to_string(),
            source: Box::new(e),
        })?;

        let mut existing_placements = Vec::new();

        for entry in vm_table.iter().map_err(|e| BlixardError::Storage {
            operation: "iterate VMs".to_string(),
            source: Box::new(e),
        })? {
            let (vm_name, vm_data) = entry.map_err(|e| BlixardError::Storage {
                operation: "read VM entry".to_string(),
                source: Box::new(e),
            })?;

            let vm_config: VmConfig = bincode::deserialize(vm_data.value())
                .map_err(|e| BlixardError::Serialization {
                    operation: "deserialize VM config".to_string(),
                    source: Box::new(e),
                })?;

            // For now, create empty groups and use a default node (0) for VMs without placement info
            // TODO: VM placement tracking - need to track which node VMs are actually placed on
            let vm_name_str = std::str::from_utf8(vm_name.value().as_ref()).unwrap_or("unknown").to_string();
            let groups = vm_config.metadata.as_ref()
                .and_then(|metadata| metadata.get("affinity-group"))
                .map(|group| vec![group.clone()])
                .unwrap_or_default();
            let node_id = 0u64; // Default node until proper placement tracking is implemented
            
            if !groups.is_empty() {
                existing_placements.push((vm_name_str, groups, node_id));
            }
        }

        Ok(AntiAffinityChecker::new(existing_placements))
    }
}