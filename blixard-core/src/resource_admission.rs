//! Enhanced resource admission control with overcommit policy support
//!
//! This module provides admission control for VM placement that integrates
//! with the ClusterResourceManager to enforce resource limits based on
//! configurable overcommit policies.

use crate::error::{BlixardError, BlixardResult};
use crate::raft_manager::WorkerCapabilities;
use crate::resource_management::{ClusterResourceManager, OvercommitPolicy};
use crate::try_into_bytes;
// Hypervisor type is used for resource admission validation
use crate::types::{VmConfig, VmState, VmStatus};
use redb::{Database, ReadableTable};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Configuration for resource admission control
#[derive(Debug, Clone)]
pub struct AdmissionControlConfig {
    /// Whether to enable strict admission control (no overcommit)
    pub strict_mode: bool,

    /// Default overcommit policy if not specified per-node
    pub default_overcommit_policy: OvercommitPolicy,

    /// Whether to account for preemptible VMs differently
    pub preemptible_discount: f64, // e.g., 0.5 = count preemptible VMs as 50% resources

    /// Whether to reserve resources for system processes
    pub enable_system_reserve: bool,
}

impl Default for AdmissionControlConfig {
    fn default() -> Self {
        Self {
            strict_mode: false,
            default_overcommit_policy: OvercommitPolicy::moderate(),
            preemptible_discount: 0.75, // Count preemptible VMs as 75% of resources
            enable_system_reserve: true,
        }
    }
}

/// Enhanced resource admission controller
pub struct ResourceAdmissionController {
    database: Arc<Database>,
    config: AdmissionControlConfig,
    resource_manager: Arc<tokio::sync::RwLock<ClusterResourceManager>>,
}

impl ResourceAdmissionController {
    pub fn new(
        database: Arc<Database>,
        config: AdmissionControlConfig,
        resource_manager: Arc<tokio::sync::RwLock<ClusterResourceManager>>,
    ) -> Self {
        Self {
            database,
            config,
            resource_manager,
        }
    }

    /// Synchronize resource state from database to resource manager
    pub async fn sync_resource_state(&self) -> BlixardResult<()> {
        let read_txn = self.database.begin_read()?;
        let worker_table = read_txn.open_table(crate::raft_storage::WORKER_TABLE)?;
        let vm_table = read_txn.open_table(crate::raft_storage::VM_STATE_TABLE)?;

        let mut resource_manager = self.resource_manager.write().await;

        // Update node capabilities
        for entry in worker_table.iter()? {
            let (key, value) = entry?;
            let node_id = u64::from_le_bytes(try_into_bytes!(
                key.value().try_into(),
                "node_id key",
                "8-byte array"
            ));
            let (_address, capabilities): (String, WorkerCapabilities) =
                bincode::deserialize(value.value()).map_err(|e| BlixardError::Serialization {
                    operation: "deserialize worker capabilities".to_string(),
                    source: Box::new(e),
                })?;

            // Register or update node in resource manager
            resource_manager.register_node(
                node_id,
                capabilities.cpu_cores,
                capabilities.memory_mb,
                capabilities.disk_gb,
            );
        }

        // Clear all allocations (we'll rebuild from VM state)
        for (_node_id, state) in resource_manager.node_states.iter_mut() {
            state.allocated_resources = Default::default();
            state.reservations.clear();
        }

        // Rebuild allocations from VM states
        for entry in vm_table.iter()? {
            let (_, vm_data) = entry?;
            if let Ok(vm_state) = bincode::deserialize::<VmState>(vm_data.value()) {
                if vm_state.status != VmStatus::Stopped && vm_state.status != VmStatus::Failed {
                    // Calculate resource usage based on preemptible discount
                    let cpu_usage = if vm_state.config.preemptible {
                        (vm_state.config.vcpus as f64 * self.config.preemptible_discount) as u32
                    } else {
                        vm_state.config.vcpus
                    };

                    let memory_usage = if vm_state.config.preemptible {
                        (vm_state.config.memory as f64 * self.config.preemptible_discount) as u64
                    } else {
                        vm_state.config.memory as u64
                    };

                    // Allocate resources on the node
                    if let Some(node_state) = resource_manager.get_node_state_mut(vm_state.node_id)
                    {
                        // Best effort allocation - don't fail if overcommitted
                        let _ = node_state.allocate(cpu_usage, memory_usage, 5);
                    }
                }
            }
        }

        Ok(())
    }

    /// Validate VM admission with overcommit policy support
    pub async fn validate_vm_admission(
        &self,
        config: &VmConfig,
        target_node_id: u64,
    ) -> BlixardResult<()> {
        // Sync current state
        self.sync_resource_state().await?;

        let resource_manager = self.resource_manager.read().await;

        // Get node state
        let node_state = resource_manager
            .get_node_state(target_node_id)
            .ok_or_else(|| BlixardError::NodeNotFound {
                node_id: target_node_id,
            })?;

        // Calculate resource requirements
        let (cpu_required, memory_required, disk_required) = if config.preemptible {
            (
                (config.vcpus as f64 * self.config.preemptible_discount) as u32,
                (config.memory as f64 * self.config.preemptible_discount) as u64,
                5u64, // Default disk requirement
            )
        } else {
            (config.vcpus, config.memory as u64, 5u64)
        };

        // Check if node can accommodate the VM
        if self.config.strict_mode {
            // In strict mode, don't allow any overcommit
            let physical = &node_state.physical_resources;
            let allocated = &node_state.allocated_resources;

            if allocated.cpu_cores + cpu_required > physical.cpu_cores {
                return Err(BlixardError::InsufficientResources {
                    requested: format!("{}vCPU", cpu_required),
                    available: format!(
                        "{}vCPU available (strict mode)",
                        physical.cpu_cores.saturating_sub(allocated.cpu_cores)
                    ),
                });
            }

            if allocated.memory_mb + memory_required > physical.memory_mb {
                return Err(BlixardError::InsufficientResources {
                    requested: format!("{}MB memory", memory_required),
                    available: format!(
                        "{}MB available (strict mode)",
                        physical.memory_mb.saturating_sub(allocated.memory_mb)
                    ),
                });
            }
        } else {
            // Use overcommit policy
            if !node_state.can_allocate(cpu_required, memory_required, disk_required) {
                let (avail_cpu, avail_mem, avail_disk) = node_state.available_resources();
                return Err(BlixardError::InsufficientResources {
                    requested: format!(
                        "CPU: {}, Memory: {}MB, Disk: {}GB",
                        cpu_required, memory_required, disk_required
                    ),
                    available: format!(
                        "CPU: {}, Memory: {}MB, Disk: {}GB (with overcommit)",
                        avail_cpu, avail_mem, avail_disk
                    ),
                });
            }

            // Warn if node is becoming overcommitted
            if node_state.is_overcommitted() {
                warn!(
                    "Node {} will be overcommitted after placing VM {}",
                    target_node_id, config.name
                );
            }
        }

        // Check feature requirements
        let worker_capabilities = self.get_worker_capabilities(target_node_id).await?;
        for feature in &["microvm"] {
            if !worker_capabilities.features.contains(&feature.to_string()) {
                return Err(BlixardError::SchedulingError {
                    message: format!(
                        "Node {} does not support required feature: {}",
                        target_node_id, feature
                    ),
                });
            }
        }

        info!(
            "VM {} admission approved for node {} (CPU: {}, Memory: {}MB)",
            config.name, target_node_id, cpu_required, memory_required
        );

        Ok(())
    }

    /// Allocate resources for a VM after admission
    pub async fn allocate_vm_resources(
        &self,
        config: &VmConfig,
        node_id: u64,
    ) -> BlixardResult<()> {
        let mut resource_manager = self.resource_manager.write().await;

        let node_state = resource_manager
            .get_node_state_mut(node_id)
            .ok_or_else(|| BlixardError::NodeNotFound { node_id })?;

        // Calculate resource requirements
        let (cpu_required, memory_required, disk_required) = if config.preemptible {
            (
                (config.vcpus as f64 * self.config.preemptible_discount) as u32,
                (config.memory as f64 * self.config.preemptible_discount) as u64,
                5u64,
            )
        } else {
            (config.vcpus, config.memory as u64, 5u64)
        };

        node_state.allocate(cpu_required, memory_required, disk_required)?;

        debug!(
            "Allocated resources for VM {} on node {}: CPU={}, Memory={}MB, Disk={}GB",
            config.name, node_id, cpu_required, memory_required, disk_required
        );

        Ok(())
    }

    /// Release resources when a VM is stopped or deleted
    pub async fn release_vm_resources(&self, config: &VmConfig, node_id: u64) -> BlixardResult<()> {
        let mut resource_manager = self.resource_manager.write().await;

        if let Some(node_state) = resource_manager.get_node_state_mut(node_id) {
            // Calculate resource requirements
            let (cpu_required, memory_required, disk_required) = if config.preemptible {
                (
                    (config.vcpus as f64 * self.config.preemptible_discount) as u32,
                    (config.memory as f64 * self.config.preemptible_discount) as u64,
                    5u64,
                )
            } else {
                (config.vcpus, config.memory as u64, 5u64)
            };

            node_state.release(cpu_required, memory_required, disk_required);

            debug!(
                "Released resources for VM {} on node {}: CPU={}, Memory={}MB, Disk={}GB",
                config.name, node_id, cpu_required, memory_required, disk_required
            );
        }

        Ok(())
    }

    /// Get worker capabilities for a node
    async fn get_worker_capabilities(&self, node_id: u64) -> BlixardResult<WorkerCapabilities> {
        let read_txn = self.database.begin_read()?;
        let worker_table = read_txn.open_table(crate::raft_storage::WORKER_TABLE)?;

        if let Some(data) = worker_table.get(node_id.to_le_bytes().as_ref())? {
            let (_address, capabilities): (String, WorkerCapabilities) =
                bincode::deserialize(data.value()).map_err(|e| BlixardError::Serialization {
                    operation: "deserialize worker capabilities".to_string(),
                    source: Box::new(e),
                })?;
            Ok(capabilities)
        } else {
            Err(BlixardError::NodeNotFound { node_id })
        }
    }

    /// Get current resource utilization summary
    pub async fn get_resource_summary(&self) -> BlixardResult<ResourceUtilizationSummary> {
        self.sync_resource_state().await?;

        let resource_manager = self.resource_manager.read().await;
        let cluster_summary = resource_manager.cluster_summary();

        let total_cpu = cluster_summary.total_physical.cpu_cores;
        let total_memory = cluster_summary.total_physical.memory_mb;
        let total_disk = cluster_summary.total_physical.disk_gb;

        let allocated_cpu = cluster_summary.total_allocated.cpu_cores;
        let allocated_memory = cluster_summary.total_allocated.memory_mb;
        let allocated_disk = cluster_summary.total_allocated.disk_gb;

        Ok(ResourceUtilizationSummary {
            total_cpu,
            total_memory_mb: total_memory,
            total_disk_gb: total_disk,
            allocated_cpu,
            allocated_memory_mb: allocated_memory,
            allocated_disk_gb: allocated_disk,
            cpu_utilization_percent: if total_cpu > 0 {
                (allocated_cpu as f64 / total_cpu as f64) * 100.0
            } else {
                0.0
            },
            memory_utilization_percent: if total_memory > 0 {
                (allocated_memory as f64 / total_memory as f64) * 100.0
            } else {
                0.0
            },
            disk_utilization_percent: if total_disk > 0 {
                (allocated_disk as f64 / total_disk as f64) * 100.0
            } else {
                0.0
            },
            node_count: cluster_summary.node_count,
        })
    }
}

/// Summary of cluster-wide resource utilization
#[derive(Debug, Clone)]
pub struct ResourceUtilizationSummary {
    pub total_cpu: u32,
    pub total_memory_mb: u64,
    pub total_disk_gb: u64,
    pub allocated_cpu: u32,
    pub allocated_memory_mb: u64,
    pub allocated_disk_gb: u64,
    pub cpu_utilization_percent: f64,
    pub memory_utilization_percent: f64,
    pub disk_utilization_percent: f64,
    pub node_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_admission_control_with_overcommit() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let database = Arc::new(Database::create(&db_path).unwrap());

        // Initialize tables
        let write_txn = database.begin_write().unwrap();
        let _ = write_txn
            .open_table(crate::raft_storage::WORKER_TABLE)
            .unwrap();
        let _ = write_txn
            .open_table(crate::raft_storage::VM_STATE_TABLE)
            .unwrap();
        write_txn.commit().unwrap();

        // Create admission controller with moderate overcommit
        let config = AdmissionControlConfig {
            strict_mode: false,
            default_overcommit_policy: OvercommitPolicy::moderate(),
            preemptible_discount: 0.5,
            enable_system_reserve: true,
        };

        let resource_manager = Arc::new(tokio::sync::RwLock::new(ClusterResourceManager::new(
            config.default_overcommit_policy.clone(),
        )));

        let controller =
            ResourceAdmissionController::new(database.clone(), config, resource_manager);

        // Register a node with 10 CPUs, 16GB RAM
        let write_txn = database.begin_write().unwrap();
        let mut worker_table = write_txn
            .open_table(crate::raft_storage::WORKER_TABLE)
            .unwrap();
        let capabilities = WorkerCapabilities {
            cpu_cores: 10,
            memory_mb: 16384,
            disk_gb: 100,
            features: vec!["microvm".to_string()],
        };
        let node_data = bincode::serialize(&("127.0.0.1:7001", capabilities)).unwrap();
        worker_table
            .insert(&1u64.to_le_bytes(), node_data.as_slice())
            .unwrap();
        write_txn.commit().unwrap();

        // Sync state
        controller.sync_resource_state().await.unwrap();

        // Test admission with moderate overcommit (2x CPU, 1.2x memory)
        // With 15% system reserve, effective capacity is:
        // CPU: 10 * 2.0 * 0.85 = 17
        // Memory: 16384 * 1.2 * 0.85 = 16711 MB

        // Should admit VMs up to effective capacity
        let vm_config = VmConfig {
            name: "test-vm".to_string(),
            vcpus: 15,
            memory: 16000,
            preemptible: false,
            priority: 100,
            anti_affinity: None,
            networks: vec![],
            volumes: vec![],
            nixos_modules: vec![],
            kernel: None,
            init_command: None,
            flake_modules: vec![],
            hypervisor: Hypervisor::CloudHypervisor,
            tenant_id: None,
        };

        // Should succeed (within overcommit limits)
        controller
            .validate_vm_admission(&vm_config, 1)
            .await
            .unwrap();

        // Test with preemptible VM (should use discount)
        let preemptible_vm = VmConfig {
            name: "preemptible-vm".to_string(),
            vcpus: 4,
            memory: 4096,
            preemptible: true,
            ..vm_config.clone()
        };

        // Should succeed with discounted resources
        controller
            .validate_vm_admission(&preemptible_vm, 1)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_strict_admission_control() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let database = Arc::new(Database::create(&db_path).unwrap());

        // Initialize tables
        let write_txn = database.begin_write().unwrap();
        let _ = write_txn
            .open_table(crate::raft_storage::WORKER_TABLE)
            .unwrap();
        let _ = write_txn
            .open_table(crate::raft_storage::VM_STATE_TABLE)
            .unwrap();
        write_txn.commit().unwrap();

        // Create admission controller in strict mode (no overcommit)
        let config = AdmissionControlConfig {
            strict_mode: true,
            default_overcommit_policy: OvercommitPolicy::conservative(),
            preemptible_discount: 1.0,
            enable_system_reserve: false,
        };

        let resource_manager = Arc::new(tokio::sync::RwLock::new(ClusterResourceManager::new(
            config.default_overcommit_policy.clone(),
        )));

        let controller =
            ResourceAdmissionController::new(database.clone(), config, resource_manager);

        // Register a node with 4 CPUs, 8GB RAM
        let write_txn = database.begin_write().unwrap();
        let mut worker_table = write_txn
            .open_table(crate::raft_storage::WORKER_TABLE)
            .unwrap();
        let capabilities = WorkerCapabilities {
            cpu_cores: 4,
            memory_mb: 8192,
            disk_gb: 100,
            features: vec!["microvm".to_string()],
        };
        let node_data = bincode::serialize(&("127.0.0.1:7001", capabilities)).unwrap();
        worker_table
            .insert(&1u64.to_le_bytes(), node_data.as_slice())
            .unwrap();
        write_txn.commit().unwrap();

        // Sync state
        controller.sync_resource_state().await.unwrap();

        // Test strict admission (no overcommit allowed)
        let vm_config = VmConfig {
            name: "test-vm".to_string(),
            vcpus: 5, // More than physical capacity
            memory: 4096,
            preemptible: false,
            priority: 100,
            anti_affinity: None,
            networks: vec![],
            volumes: vec![],
            nixos_modules: vec![],
            kernel: None,
            init_command: None,
            flake_modules: vec![],
            hypervisor: Hypervisor::CloudHypervisor,
            tenant_id: None,
        };

        // Should fail (exceeds physical capacity in strict mode)
        let result = controller.validate_vm_admission(&vm_config, 1).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            BlixardError::InsufficientResources { .. }
        ));
    }
}
