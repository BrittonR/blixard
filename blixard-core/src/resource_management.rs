//! Resource reservation and overcommit management
//!
//! This module provides resource reservation tracking and overcommit policies
//! to enable efficient resource utilization while maintaining safety limits.

use crate::error::{BlixardError, BlixardResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Resource overcommit policy configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OvercommitPolicy {
    /// CPU overcommit ratio (e.g., 2.0 = allow 2x CPU allocation)
    pub cpu_overcommit_ratio: f64,

    /// Memory overcommit ratio (e.g., 1.5 = allow 1.5x memory allocation)
    pub memory_overcommit_ratio: f64,

    /// Disk overcommit ratio (e.g., 1.2 = allow 1.2x disk allocation)
    pub disk_overcommit_ratio: f64,

    /// Whether to enforce hard limits when physical resources are exhausted
    pub enforce_hard_limits: bool,

    /// Reserved percentage of resources for system operations (0.0-1.0)
    pub system_reserve_percentage: f64,
}

impl Default for OvercommitPolicy {
    fn default() -> Self {
        Self {
            cpu_overcommit_ratio: 1.0,    // No overcommit by default
            memory_overcommit_ratio: 1.0, // No overcommit by default
            disk_overcommit_ratio: 1.0,   // No overcommit by default
            enforce_hard_limits: true,
            system_reserve_percentage: 0.1, // Reserve 10% for system
        }
    }
}

impl OvercommitPolicy {
    /// Create a conservative policy with no overcommit
    pub fn conservative() -> Self {
        Self::default()
    }

    /// Create a moderate policy with reasonable overcommit
    pub fn moderate() -> Self {
        Self {
            cpu_overcommit_ratio: 2.0, // 2x CPU overcommit (common for web workloads)
            memory_overcommit_ratio: 1.2, // 20% memory overcommit
            disk_overcommit_ratio: 1.1, // 10% disk overcommit
            enforce_hard_limits: true,
            system_reserve_percentage: 0.15, // Reserve 15% for system
        }
    }

    /// Create an aggressive policy with high overcommit
    pub fn aggressive() -> Self {
        Self {
            cpu_overcommit_ratio: 4.0,       // 4x CPU overcommit
            memory_overcommit_ratio: 1.5,    // 50% memory overcommit
            disk_overcommit_ratio: 1.3,      // 30% disk overcommit
            enforce_hard_limits: false,      // Allow soft violations
            system_reserve_percentage: 0.05, // Only 5% system reserve
        }
    }
}

/// Resource reservation for a specific VM or system component
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceReservation {
    /// Unique identifier for this reservation
    pub id: String,

    /// Entity that owns this reservation (VM name, system component, etc.)
    pub owner: String,

    /// Reserved CPU cores
    pub cpu_cores: u32,

    /// Reserved memory in MB
    pub memory_mb: u64,

    /// Reserved disk in GB
    pub disk_gb: u64,

    /// Priority level (higher = more important)
    pub priority: u32,

    /// Whether this is a hard reservation (cannot be violated)
    pub is_hard: bool,

    /// Expiration time (None = permanent)
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Tracks resource allocations and reservations for a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResourceState {
    /// Physical capabilities of the node
    pub physical_resources: PhysicalResources,

    /// Current allocations (may exceed physical due to overcommit)
    pub allocated_resources: AllocatedResources,

    /// Active reservations
    pub reservations: HashMap<String, ResourceReservation>,

    /// Overcommit policy for this node
    pub overcommit_policy: OvercommitPolicy,
}

/// Physical resources available on a node
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicalResources {
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
}

/// Resources allocated to VMs (may exceed physical)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AllocatedResources {
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
}

impl NodeResourceState {
    /// Create a new resource state for a node
    pub fn new(
        cpu_cores: u32,
        memory_mb: u64,
        disk_gb: u64,
        overcommit_policy: OvercommitPolicy,
    ) -> Self {
        Self {
            physical_resources: PhysicalResources {
                cpu_cores,
                memory_mb,
                disk_gb,
            },
            allocated_resources: AllocatedResources::default(),
            reservations: HashMap::new(),
            overcommit_policy,
        }
    }

    /// Calculate effective capacity considering overcommit policy
    pub fn effective_capacity(&self) -> (u32, u64, u64) {
        let policy = &self.overcommit_policy;
        let physical = &self.physical_resources;

        // Apply system reserve
        let reserve_factor = 1.0 - policy.system_reserve_percentage;

        let effective_cpu =
            ((physical.cpu_cores as f64) * policy.cpu_overcommit_ratio * reserve_factor) as u32;
        let effective_memory =
            ((physical.memory_mb as f64) * policy.memory_overcommit_ratio * reserve_factor) as u64;
        let effective_disk =
            ((physical.disk_gb as f64) * policy.disk_overcommit_ratio * reserve_factor) as u64;

        (effective_cpu, effective_memory, effective_disk)
    }

    /// Calculate available resources considering overcommit
    pub fn available_resources(&self) -> (u32, u64, u64) {
        let (effective_cpu, effective_memory, effective_disk) = self.effective_capacity();
        let allocated = &self.allocated_resources;

        let available_cpu = effective_cpu.saturating_sub(allocated.cpu_cores);
        let available_memory = effective_memory.saturating_sub(allocated.memory_mb);
        let available_disk = effective_disk.saturating_sub(allocated.disk_gb);

        (available_cpu, available_memory, available_disk)
    }

    /// Check if resources can be allocated
    pub fn can_allocate(&self, cpu: u32, memory: u64, disk: u64) -> bool {
        let (available_cpu, available_memory, available_disk) = self.available_resources();

        available_cpu >= cpu && available_memory >= memory && available_disk >= disk
    }

    /// Allocate resources
    pub fn allocate(&mut self, cpu: u32, memory: u64, disk: u64) -> BlixardResult<()> {
        if !self.can_allocate(cpu, memory, disk) {
            return Err(BlixardError::InsufficientResources {
                requested: format!("CPU: {}, Memory: {}MB, Disk: {}GB", cpu, memory, disk),
                available: {
                    let (a_cpu, a_mem, a_disk) = self.available_resources();
                    format!("CPU: {}, Memory: {}MB, Disk: {}GB", a_cpu, a_mem, a_disk)
                },
            });
        }

        self.allocated_resources.cpu_cores += cpu;
        self.allocated_resources.memory_mb += memory;
        self.allocated_resources.disk_gb += disk;

        Ok(())
    }

    /// Release allocated resources
    pub fn release(&mut self, cpu: u32, memory: u64, disk: u64) {
        self.allocated_resources.cpu_cores = self.allocated_resources.cpu_cores.saturating_sub(cpu);
        self.allocated_resources.memory_mb =
            self.allocated_resources.memory_mb.saturating_sub(memory);
        self.allocated_resources.disk_gb = self.allocated_resources.disk_gb.saturating_sub(disk);
    }

    /// Add a resource reservation
    pub fn add_reservation(&mut self, reservation: ResourceReservation) -> BlixardResult<()> {
        // Check if reservation can be satisfied
        if reservation.is_hard
            && !self.can_allocate(
                reservation.cpu_cores,
                reservation.memory_mb,
                reservation.disk_gb,
            )
        {
            return Err(BlixardError::InsufficientResources {
                requested: format!(
                    "Reservation {}: CPU: {}, Memory: {}MB, Disk: {}GB",
                    reservation.id,
                    reservation.cpu_cores,
                    reservation.memory_mb,
                    reservation.disk_gb
                ),
                available: {
                    let (a_cpu, a_mem, a_disk) = self.available_resources();
                    format!("CPU: {}, Memory: {}MB, Disk: {}GB", a_cpu, a_mem, a_disk)
                },
            });
        }

        // Allocate resources for the reservation
        self.allocate(
            reservation.cpu_cores,
            reservation.memory_mb,
            reservation.disk_gb,
        )?;

        self.reservations
            .insert(reservation.id.clone(), reservation);
        Ok(())
    }

    /// Remove a reservation by ID
    pub fn remove_reservation(&mut self, reservation_id: &str) -> Option<ResourceReservation> {
        if let Some(reservation) = self.reservations.remove(reservation_id) {
            // Release the reserved resources
            self.release(
                reservation.cpu_cores,
                reservation.memory_mb,
                reservation.disk_gb,
            );
            Some(reservation)
        } else {
            None
        }
    }

    /// Clean up expired reservations
    pub fn cleanup_expired_reservations(&mut self) {
        let now = chrono::Utc::now();
        let expired_ids: Vec<String> = self
            .reservations
            .iter()
            .filter(|(_, res)| {
                if let Some(expires_at) = res.expires_at {
                    expires_at < now
                } else {
                    false
                }
            })
            .map(|(id, _)| id.clone())
            .collect();

        for id in expired_ids {
            self.remove_reservation(&id);
        }
    }

    /// Get utilization percentage for each resource type
    pub fn utilization_percentages(&self) -> (f64, f64, f64) {
        let physical = &self.physical_resources;
        let allocated = &self.allocated_resources;

        let cpu_util = if physical.cpu_cores > 0 {
            (allocated.cpu_cores as f64 / physical.cpu_cores as f64) * 100.0
        } else {
            0.0
        };

        let memory_util = if physical.memory_mb > 0 {
            (allocated.memory_mb as f64 / physical.memory_mb as f64) * 100.0
        } else {
            0.0
        };

        let disk_util = if physical.disk_gb > 0 {
            (allocated.disk_gb as f64 / physical.disk_gb as f64) * 100.0
        } else {
            0.0
        };

        (cpu_util, memory_util, disk_util)
    }

    /// Check if overcommit limits are exceeded
    pub fn is_overcommitted(&self) -> bool {
        let (cpu_util, memory_util, disk_util) = self.utilization_percentages();
        let policy = &self.overcommit_policy;

        cpu_util > policy.cpu_overcommit_ratio * 100.0
            || memory_util > policy.memory_overcommit_ratio * 100.0
            || disk_util > policy.disk_overcommit_ratio * 100.0
    }
}

/// Cluster-wide resource management
pub struct ClusterResourceManager {
    /// Resource state for each node
    pub node_states: HashMap<u64, NodeResourceState>,

    /// Default overcommit policy for new nodes
    default_policy: OvercommitPolicy,
}

impl ClusterResourceManager {
    /// Create a new cluster resource manager
    pub fn new(default_policy: OvercommitPolicy) -> Self {
        Self {
            node_states: HashMap::new(),
            default_policy,
        }
    }

    /// Register a node with its physical resources
    pub fn register_node(&mut self, node_id: u64, cpu_cores: u32, memory_mb: u64, disk_gb: u64) {
        let state =
            NodeResourceState::new(cpu_cores, memory_mb, disk_gb, self.default_policy.clone());
        self.node_states.insert(node_id, state);
    }

    /// Update overcommit policy for a node
    pub fn update_node_policy(
        &mut self,
        node_id: u64,
        policy: OvercommitPolicy,
    ) -> BlixardResult<()> {
        self.node_states
            .get_mut(&node_id)
            .map(|state| state.overcommit_policy = policy)
            .ok_or_else(|| BlixardError::NodeNotFound { node_id })
    }

    /// Get resource state for a node
    pub fn get_node_state(&self, node_id: u64) -> Option<&NodeResourceState> {
        self.node_states.get(&node_id)
    }

    /// Get mutable resource state for a node
    pub fn get_node_state_mut(&mut self, node_id: u64) -> Option<&mut NodeResourceState> {
        self.node_states.get_mut(&node_id)
    }

    /// Find nodes with available capacity for given requirements
    pub fn find_nodes_with_capacity(&self, cpu: u32, memory: u64, disk: u64) -> Vec<u64> {
        self.node_states
            .iter()
            .filter(|(_, state)| state.can_allocate(cpu, memory, disk))
            .map(|(id, _)| *id)
            .collect()
    }

    /// Get cluster-wide resource summary
    pub fn cluster_summary(&self) -> ClusterResourceSummary {
        let mut total_physical = PhysicalResources {
            cpu_cores: 0,
            memory_mb: 0,
            disk_gb: 0,
        };
        let mut total_allocated = AllocatedResources::default();
        let mut total_reserved = AllocatedResources::default();

        for state in self.node_states.values() {
            total_physical.cpu_cores += state.physical_resources.cpu_cores;
            total_physical.memory_mb += state.physical_resources.memory_mb;
            total_physical.disk_gb += state.physical_resources.disk_gb;

            total_allocated.cpu_cores += state.allocated_resources.cpu_cores;
            total_allocated.memory_mb += state.allocated_resources.memory_mb;
            total_allocated.disk_gb += state.allocated_resources.disk_gb;

            for reservation in state.reservations.values() {
                total_reserved.cpu_cores += reservation.cpu_cores;
                total_reserved.memory_mb += reservation.memory_mb;
                total_reserved.disk_gb += reservation.disk_gb;
            }
        }

        ClusterResourceSummary {
            total_physical,
            total_allocated,
            total_reserved,
            node_count: self.node_states.len(),
        }
    }
}

/// Summary of cluster-wide resource usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterResourceSummary {
    pub total_physical: PhysicalResources,
    pub total_allocated: AllocatedResources,
    pub total_reserved: AllocatedResources,
    pub node_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overcommit_policy() {
        let conservative = OvercommitPolicy::conservative();
        assert_eq!(conservative.cpu_overcommit_ratio, 1.0);

        let moderate = OvercommitPolicy::moderate();
        assert_eq!(moderate.cpu_overcommit_ratio, 2.0);

        let aggressive = OvercommitPolicy::aggressive();
        assert_eq!(aggressive.cpu_overcommit_ratio, 4.0);
    }

    #[test]
    fn test_effective_capacity() {
        let mut state = NodeResourceState::new(
            10,    // CPU cores
            16384, // Memory MB
            100,   // Disk GB
            OvercommitPolicy::moderate(),
        );

        let (cpu, memory, disk) = state.effective_capacity();
        // Moderate policy: 2x CPU, 1.2x memory, 1.1x disk, with 15% reserve
        assert_eq!(cpu, 17); // 10 * 2.0 * 0.85 = 17
        assert_eq!(memory, 16711); // 16384 * 1.2 * 0.85 = 16711
        assert_eq!(disk, 93); // 100 * 1.1 * 0.85 = 93.5 -> 93
    }

    #[test]
    fn test_resource_allocation() {
        let mut state = NodeResourceState::new(10, 16384, 100, OvercommitPolicy::default());

        // Test successful allocation
        assert!(state.can_allocate(4, 8192, 50));
        state.allocate(4, 8192, 50).expect("Resource allocation should succeed in test");

        // Check available resources
        let (cpu, memory, disk) = state.available_resources();
        assert_eq!(cpu, 5); // 10 * 0.9 - 4 = 5
        assert_eq!(memory, 6553); // 16384 * 0.9 - 8192 = 6553
        assert_eq!(disk, 40); // 100 * 0.9 - 50 = 40

        // Test allocation failure
        assert!(!state.can_allocate(10, 10000, 50));
    }

    #[test]
    fn test_reservations() {
        let mut state = NodeResourceState::new(10, 16384, 100, OvercommitPolicy::default());

        let reservation = ResourceReservation {
            id: "test-res".to_string(),
            owner: "system".to_string(),
            cpu_cores: 2,
            memory_mb: 4096,
            disk_gb: 10,
            priority: 100,
            is_hard: true,
            expires_at: None,
        };

        // Add reservation
        state.add_reservation(reservation.clone()).unwrap();
        assert_eq!(state.reservations.len(), 1);

        // Check that resources are allocated
        let (cpu, memory, disk) = state.available_resources();
        assert_eq!(cpu, 7); // 10 * 0.9 - 2 = 7

        // Remove reservation
        let removed = state.remove_reservation("test-res").unwrap();
        assert_eq!(removed.id, reservation.id);
        assert_eq!(state.reservations.len(), 0);

        // Check that resources are released
        let (cpu, memory, disk) = state.available_resources();
        assert_eq!(cpu, 9); // 10 * 0.9 = 9
    }
}
