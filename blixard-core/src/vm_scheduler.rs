use std::sync::Arc;
use redb::{Database, ReadableTable};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::error::{BlixardError, BlixardResult};
use crate::types::VmConfig;
use crate::raft_manager::{WorkerCapabilities, WorkerStatus};
use crate::storage::{WORKER_TABLE, WORKER_STATUS_TABLE, VM_STATE_TABLE};
use crate::metrics_otel;
use crate::anti_affinity::{AntiAffinityChecker, AntiAffinityRules};
use crate::resource_management::{
    NodeResourceState, OvercommitPolicy, ResourceReservation, ClusterResourceManager,
};

/// VM placement strategy for determining where to place VMs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlacementStrategy {
    /// Place VM on node with most available resources
    MostAvailable,
    /// Place VM on node with least available resources (bin packing)
    LeastAvailable,
    /// Spread VMs evenly across nodes
    RoundRobin,
    /// Place VM on specific node (manual placement)
    Manual { node_id: u64 },
}

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

/// Current resource usage for a worker node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResourceUsage {
    pub node_id: u64,
    pub capabilities: WorkerCapabilities,
    pub used_vcpus: u32,
    pub used_memory_mb: u64,
    pub used_disk_gb: u64,
    pub running_vms: u32,
}

impl NodeResourceUsage {
    /// Calculate available resources
    pub fn available_vcpus(&self) -> u32 {
        self.capabilities.cpu_cores.saturating_sub(self.used_vcpus)
    }
    
    pub fn available_memory_mb(&self) -> u64 {
        self.capabilities.memory_mb.saturating_sub(self.used_memory_mb)
    }
    
    pub fn available_disk_gb(&self) -> u64 {
        self.capabilities.disk_gb.saturating_sub(self.used_disk_gb)
    }
    
    /// Check if this node can accommodate the given resource requirements
    pub fn can_accommodate(&self, requirements: &VmResourceRequirements) -> bool {
        self.available_vcpus() >= requirements.vcpus &&
        self.available_memory_mb() >= requirements.memory_mb &&
        self.available_disk_gb() >= requirements.disk_gb &&
        requirements.required_features.iter().all(|feature| self.capabilities.features.contains(feature))
    }
    
    /// Calculate a placement score for resource-based strategies
    /// Higher score = better placement candidate
    pub fn placement_score(&self, strategy: &PlacementStrategy) -> f64 {
        match strategy {
            PlacementStrategy::MostAvailable => {
                // Favor nodes with most available resources (percentage-based)
                let cpu_available_pct = if self.capabilities.cpu_cores > 0 {
                    self.available_vcpus() as f64 / self.capabilities.cpu_cores as f64
                } else {
                    0.0
                };
                let mem_available_pct = if self.capabilities.memory_mb > 0 {
                    self.available_memory_mb() as f64 / self.capabilities.memory_mb as f64
                } else {
                    0.0
                };
                let disk_available_pct = if self.capabilities.disk_gb > 0 {
                    self.available_disk_gb() as f64 / self.capabilities.disk_gb as f64
                } else {
                    0.0
                };
                
                (cpu_available_pct + mem_available_pct + disk_available_pct) / 3.0
            }
            PlacementStrategy::LeastAvailable => {
                // Favor nodes with least available resources (bin packing)
                let cpu_used_pct = if self.capabilities.cpu_cores > 0 {
                    self.used_vcpus as f64 / self.capabilities.cpu_cores as f64
                } else {
                    0.0
                };
                let mem_used_pct = if self.capabilities.memory_mb > 0 {
                    self.used_memory_mb as f64 / self.capabilities.memory_mb as f64
                } else {
                    0.0
                };
                let disk_used_pct = if self.capabilities.disk_gb > 0 {
                    self.used_disk_gb as f64 / self.capabilities.disk_gb as f64
                } else {
                    0.0
                };
                
                (cpu_used_pct + mem_used_pct + disk_used_pct) / 3.0
            }
            PlacementStrategy::RoundRobin => {
                // Favor nodes with fewer running VMs
                1.0 / (1.0 + self.running_vms as f64)
            }
            PlacementStrategy::Manual { .. } => {
                // Manual placement doesn't use scoring
                0.0
            }
        }
    }
}

/// Placement decision result
#[derive(Debug, Clone)]
pub struct PlacementDecision {
    pub selected_node_id: u64,
    pub reason: String,
    pub alternative_nodes: Vec<u64>,
}

/// VM Scheduler for automatic placement of VMs across cluster nodes
pub struct VmScheduler {
    database: Arc<Database>,
    resource_manager: Arc<tokio::sync::RwLock<ClusterResourceManager>>,
}

impl VmScheduler {
    /// Create a new VM scheduler
    pub fn new(database: Arc<Database>) -> Self {
        Self { 
            database,
            resource_manager: Arc::new(tokio::sync::RwLock::new(
                ClusterResourceManager::new(OvercommitPolicy::moderate())
            )),
        }
    }
    
    /// Create a new VM scheduler with custom overcommit policy
    pub fn with_overcommit_policy(database: Arc<Database>, policy: OvercommitPolicy) -> Self {
        Self { 
            database,
            resource_manager: Arc::new(tokio::sync::RwLock::new(
                ClusterResourceManager::new(policy)
            )),
        }
    }
    
    /// Schedule a VM placement based on resource requirements and strategy
    pub async fn schedule_vm_placement(
        &self,
        vm_config: &VmConfig,
        strategy: PlacementStrategy,
    ) -> BlixardResult<PlacementDecision> {
        let start_time = std::time::Instant::now();
        let requirements = VmResourceRequirements::from(vm_config);
        let strategy_name = format!("{:?}", strategy);
        
        // Handle manual placement first
        if let PlacementStrategy::Manual { node_id } = strategy {
            let result = self.validate_manual_placement(node_id, &requirements, vm_config.anti_affinity.as_ref()).await;
            let duration = start_time.elapsed().as_secs_f64();
            metrics_otel::record_vm_placement_attempt(&strategy_name, result.is_ok(), duration);
            return result;
        }
        
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
        
        // Filter nodes that can accommodate the VM
        let mut candidate_nodes: Vec<_> = node_usage
            .iter()
            .filter(|usage| usage.can_accommodate(&requirements))
            .collect();
        
        // Apply anti-affinity hard constraints if present
        if let (Some(rules), Some(checker)) = (vm_config.anti_affinity.as_ref(), &anti_affinity_checker) {
            candidate_nodes.retain(|usage| {
                match checker.check_hard_constraints(rules, usage.node_id) {
                    Ok(()) => true,
                    Err(reason) => {
                        info!("Node {} excluded by anti-affinity: {}", usage.node_id, reason);
                        false
                    }
                }
            });
        }
        
        if candidate_nodes.is_empty() {
            return Err(BlixardError::SchedulingError {
                message: format!(
                    "No nodes can accommodate VM '{}' (requires: {}vCPU, {}MB RAM, {}GB disk, features: {:?}, anti-affinity: {})",
                    vm_config.name,
                    requirements.vcpus,
                    requirements.memory_mb,
                    requirements.disk_gb,
                    requirements.required_features,
                    vm_config.anti_affinity.is_some()
                ),
            });
        }
        
        // Select the best node based on strategy and anti-affinity soft constraints
        let selected_node = self.select_best_node_with_anti_affinity(
            candidate_nodes,
            &strategy,
            vm_config.anti_affinity.as_ref(),
            anti_affinity_checker.as_ref()
        )?;
        
        let alternative_nodes: Vec<u64> = node_usage
            .iter()
            .filter(|usage| {
                usage.node_id != selected_node.node_id && 
                usage.can_accommodate(&requirements) &&
                // Check anti-affinity for alternatives too
                if let (Some(rules), Some(checker)) = (vm_config.anti_affinity.as_ref(), &anti_affinity_checker) {
                    checker.check_hard_constraints(rules, usage.node_id).is_ok()
                } else {
                    true
                }
            })
            .map(|usage| usage.node_id)
            .collect();
        
        let reason = format!(
            "Selected node {} using {:?} strategy. Available: {}vCPU, {}MB RAM, {}GB disk{}",
            selected_node.node_id,
            strategy,
            selected_node.available_vcpus(),
            selected_node.available_memory_mb(),
            selected_node.available_disk_gb(),
            if vm_config.anti_affinity.is_some() { " (anti-affinity rules applied)" } else { "" }
        );
        
        let decision = PlacementDecision {
            selected_node_id: selected_node.node_id,
            reason,
            alternative_nodes,
        };
        
        // Record successful placement metrics
        let duration = start_time.elapsed().as_secs_f64();
        metrics_otel::record_vm_placement_attempt(&strategy_name, true, duration);
        
        Ok(decision)
    }
    
    /// Validate that a manual placement is feasible
    async fn validate_manual_placement(
        &self,
        node_id: u64,
        requirements: &VmResourceRequirements,
        anti_affinity_rules: Option<&AntiAffinityRules>,
    ) -> BlixardResult<PlacementDecision> {
        let node_usage = self.get_node_resource_usage(node_id).await?;
        
        if !node_usage.can_accommodate(requirements) {
            return Err(BlixardError::SchedulingError {
                message: format!(
                    "Node {} cannot accommodate VM (requires: {}vCPU, {}MB RAM, {}GB disk). Available: {}vCPU, {}MB RAM, {}GB disk",
                    node_id,
                    requirements.vcpus,
                    requirements.memory_mb,
                    requirements.disk_gb,
                    node_usage.available_vcpus(),
                    node_usage.available_memory_mb(),
                    node_usage.available_disk_gb()
                ),
            });
        }
        
        // Check anti-affinity constraints if present
        if let Some(rules) = anti_affinity_rules {
            let checker = self.build_anti_affinity_checker().await?;
            if let Err(reason) = checker.check_hard_constraints(rules, node_id) {
                return Err(BlixardError::SchedulingError {
                    message: format!("Manual placement on node {} violates anti-affinity: {}", node_id, reason),
                });
            }
        }
        
        Ok(PlacementDecision {
            selected_node_id: node_id,
            reason: format!("Manual placement on node {}", node_id),
            alternative_nodes: vec![],
        })
    }
    
    /// Select the best node from candidates based on placement strategy
    fn select_best_node<'a>(
        &self,
        candidates: Vec<&'a NodeResourceUsage>,
        strategy: &PlacementStrategy,
    ) -> BlixardResult<&'a NodeResourceUsage> {
        match strategy {
            PlacementStrategy::RoundRobin => {
                // For round-robin, select node with fewest running VMs
                candidates
                    .iter()
                    .min_by_key(|usage| usage.running_vms)
                    .copied()
                    .ok_or_else(|| BlixardError::SchedulingError {
                        message: "No candidate nodes available".to_string(),
                    })
            }
            _ => {
                // For resource-based strategies, use placement scoring
                candidates
                    .iter()
                    .max_by(|a, b| {
                        a.placement_score(strategy)
                            .partial_cmp(&b.placement_score(strategy))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .copied()
                    .ok_or_else(|| BlixardError::SchedulingError {
                        message: "No candidate nodes available".to_string(),
                    })
            }
        }
    }
    
    /// Get current resource usage for all healthy worker nodes
    pub async fn get_cluster_resource_usage(&self) -> BlixardResult<Vec<NodeResourceUsage>> {
        let read_txn = self.database.begin_read()?;
        
        let worker_table = read_txn.open_table(WORKER_TABLE)?;
        let status_table = read_txn.open_table(WORKER_STATUS_TABLE)?;
        let vm_table = read_txn.open_table(VM_STATE_TABLE)?;
        
        let mut node_usage = Vec::new();
        
        // Iterate through all workers
        for worker_result in worker_table.iter()? {
            let (node_id_bytes, worker_data) = worker_result?;
            let node_id = u64::from_le_bytes(
                node_id_bytes.value()[..8].try_into()
                    .map_err(|_| BlixardError::Storage {
                        operation: "parse node ID".to_string(),
                        source: Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid node ID")),
                    })?
            );
            
            // Check if worker is healthy
            let is_healthy = status_table.get(node_id_bytes.value())?
                .map(|status_data| status_data.value()[0] == WorkerStatus::Online as u8)
                .unwrap_or(false);
            
            if !is_healthy {
                continue; // Skip unhealthy workers
            }
            
            // Parse worker capabilities
            let (_, capabilities): (String, WorkerCapabilities) = bincode::deserialize(worker_data.value())?;
            
            // Calculate current resource usage from running VMs
            let (used_vcpus, used_memory_mb, used_disk_gb, running_vms) = self.calculate_node_usage(&vm_table, node_id)?;
            
            node_usage.push(NodeResourceUsage {
                node_id,
                capabilities,
                used_vcpus,
                used_memory_mb,
                used_disk_gb,
                running_vms,
            });
        }
        
        Ok(node_usage)
    }
    
    /// Get resource usage for a specific node
    async fn get_node_resource_usage(&self, node_id: u64) -> BlixardResult<NodeResourceUsage> {
        let read_txn = self.database.begin_read()?;
        
        let worker_table = read_txn.open_table(WORKER_TABLE)?;
        let status_table = read_txn.open_table(WORKER_STATUS_TABLE)?;
        let vm_table = read_txn.open_table(VM_STATE_TABLE)?;
        
        let node_id_bytes = node_id.to_le_bytes();
        
        // Get worker capabilities
        let worker_data = worker_table.get(node_id_bytes.as_slice())?
            .ok_or_else(|| BlixardError::SchedulingError {
                message: format!("Node {} not found in worker registry", node_id),
            })?;
        
        // Check if worker is healthy
        let is_healthy = status_table.get(node_id_bytes.as_slice())?
            .map(|status_data| status_data.value()[0] == WorkerStatus::Online as u8)
            .unwrap_or(false);
        
        if !is_healthy {
            return Err(BlixardError::SchedulingError {
                message: format!("Node {} is not healthy", node_id),
            });
        }
        
        let (_, capabilities): (String, WorkerCapabilities) = bincode::deserialize(worker_data.value())?;
        
        // Calculate current resource usage
        let (used_vcpus, used_memory_mb, used_disk_gb, running_vms) = self.calculate_node_usage(&vm_table, node_id)?;
        
        Ok(NodeResourceUsage {
            node_id,
            capabilities,
            used_vcpus,
            used_memory_mb,
            used_disk_gb,
            running_vms,
        })
    }
    
    /// Calculate resource usage for a specific node from VM states
    fn calculate_node_usage(
        &self,
        vm_table: &redb::ReadOnlyTable<&str, &[u8]>,
        node_id: u64,
    ) -> BlixardResult<(u32, u64, u64, u32)> {
        let mut used_vcpus = 0u32;
        let mut used_memory_mb = 0u64;
        let mut used_disk_gb = 0u64;
        let mut running_vms = 0u32;
        
        // Iterate through all VMs to find ones on this node
        for vm_result in vm_table.iter()? {
            let (_, vm_data) = vm_result?;
            let vm_state: crate::types::VmState = bincode::deserialize(vm_data.value())?;
            
            if vm_state.node_id == node_id {
                // Count resources for VMs that are running or starting
                match vm_state.status {
                    crate::types::VmStatus::Running | 
                    crate::types::VmStatus::Starting => {
                        used_vcpus += vm_state.config.vcpus;
                        used_memory_mb += vm_state.config.memory as u64;
                        used_disk_gb += 5; // Default disk usage per VM
                        running_vms += 1;
                    }
                    _ => {
                        // VMs that are stopped, failed, etc. don't consume resources
                    }
                }
            }
        }
        
        Ok((used_vcpus, used_memory_mb, used_disk_gb, running_vms))
    }
    
    /// Get cluster-wide resource summary
    pub async fn get_cluster_resource_summary(&self) -> BlixardResult<ClusterResourceSummary> {
        let node_usage = self.get_cluster_resource_usage().await?;
        
        let mut summary = ClusterResourceSummary {
            total_nodes: node_usage.len() as u32,
            total_vcpus: 0,
            used_vcpus: 0,
            total_memory_mb: 0,
            used_memory_mb: 0,
            total_disk_gb: 0,
            used_disk_gb: 0,
            total_running_vms: 0,
            nodes: node_usage.clone(),
        };
        
        for usage in &node_usage {
            summary.total_vcpus += usage.capabilities.cpu_cores;
            summary.used_vcpus += usage.used_vcpus;
            summary.total_memory_mb += usage.capabilities.memory_mb;
            summary.used_memory_mb += usage.used_memory_mb;
            summary.total_disk_gb += usage.capabilities.disk_gb;
            summary.used_disk_gb += usage.used_disk_gb;
            summary.total_running_vms += usage.running_vms;
        }
        
        // Update cluster-wide resource metrics
        metrics_otel::update_cluster_resource_metrics(&summary);
        
        // Update per-node resource metrics  
        for usage in &node_usage {
            metrics_otel::update_node_resource_metrics(usage.node_id, usage);
        }
        
        Ok(summary)
    }
    
    /// Update the resource manager with node information
    pub async fn sync_resource_manager(&self) -> BlixardResult<()> {
        let read_txn = self.database.begin_read()?;
        let worker_table = read_txn.open_table(WORKER_TABLE)?;
        let status_table = read_txn.open_table(WORKER_STATUS_TABLE)?;
        
        let mut resource_manager = self.resource_manager.write().await;
        
        // Sync all worker nodes
        for result in worker_table.iter()? {
            let (key, value) = result?;
            let node_id = u64::from_le_bytes(key.value().try_into().unwrap());
            
            // Check if worker is online
            if let Ok(Some(status_data)) = status_table.get(key.value()) {
                if status_data.value().first() == Some(&(WorkerStatus::Online as u8)) {
                    let (_name, capabilities): (String, WorkerCapabilities) = 
                        bincode::deserialize(value.value())?;
                    
                    // Register or update node in resource manager
                    resource_manager.register_node(
                        node_id,
                        capabilities.cpu_cores,
                        capabilities.memory_mb,
                        capabilities.disk_gb,
                    );
                }
            }
        }
        
        Ok(())
    }
    
    /// Schedule VM placement with resource reservation support
    pub async fn schedule_vm_placement_with_reservation(
        &self,
        vm_config: &VmConfig,
        strategy: PlacementStrategy,
        reservation_priority: Option<u32>,
    ) -> BlixardResult<PlacementDecision> {
        // First sync resource manager with current state
        self.sync_resource_manager().await?;
        
        // Schedule placement using normal logic
        let decision = self.schedule_vm_placement(vm_config, strategy).await?;
        
        // If successful and reservation requested, create a reservation
        if let Some(priority) = reservation_priority {
            let mut resource_manager = self.resource_manager.write().await;
            
            if let Some(node_state) = resource_manager.get_node_state_mut(decision.selected_node_id) {
                let reservation = ResourceReservation {
                    id: format!("vm-{}-reservation", vm_config.name),
                    owner: vm_config.name.clone(),
                    cpu_cores: vm_config.vcpus,
                    memory_mb: vm_config.memory as u64,
                    disk_gb: 10, // Default disk reservation
                    priority,
                    is_hard: true,
                    expires_at: None, // Permanent until VM is deleted
                };
                
                // Try to add reservation
                if let Err(e) = node_state.add_reservation(reservation) {
                    tracing::warn!(
                        "Failed to create reservation for VM {} on node {}: {}",
                        vm_config.name, decision.selected_node_id, e
                    );
                }
            }
        }
        
        Ok(decision)
    }
    
    /// Get resource utilization considering overcommit
    pub async fn get_node_resource_utilization(&self, node_id: u64) -> BlixardResult<Option<(f64, f64, f64)>> {
        let resource_manager = self.resource_manager.read().await;
        
        if let Some(node_state) = resource_manager.get_node_state(node_id) {
            Ok(Some(node_state.utilization_percentages()))
        } else {
            Ok(None)
        }
    }
    
    /// Update overcommit policy for a specific node
    pub async fn update_node_overcommit_policy(
        &self,
        node_id: u64,
        policy: OvercommitPolicy,
    ) -> BlixardResult<()> {
        let mut resource_manager = self.resource_manager.write().await;
        resource_manager.update_node_policy(node_id, policy)
    }
    
    /// Get nodes that support overcommit
    pub async fn get_overcommit_capable_nodes(&self) -> BlixardResult<Vec<u64>> {
        let resource_manager = self.resource_manager.read().await;
        let _summary = resource_manager.cluster_summary();
        
        // Return all nodes that have overcommit enabled
        let mut capable_nodes = Vec::new();
        for (node_id, state) in resource_manager.node_states.iter() {
            if state.overcommit_policy.cpu_overcommit_ratio > 1.0 ||
               state.overcommit_policy.memory_overcommit_ratio > 1.0 ||
               state.overcommit_policy.disk_overcommit_ratio > 1.0 {
                capable_nodes.push(*node_id);
            }
        }
        
        Ok(capable_nodes)
    }
    
    /// Create a resource reservation for system use
    pub async fn create_system_reservation(
        &self,
        node_id: u64,
        cpu_cores: u32,
        memory_mb: u64,
        disk_gb: u64,
        reservation_id: &str,
    ) -> BlixardResult<()> {
        let mut resource_manager = self.resource_manager.write().await;
        
        if let Some(node_state) = resource_manager.get_node_state_mut(node_id) {
            let reservation = ResourceReservation {
                id: reservation_id.to_string(),
                owner: "system".to_string(),
                cpu_cores,
                memory_mb,
                disk_gb,
                priority: 1000, // High priority for system reservations
                is_hard: true,
                expires_at: None,
            };
            
            node_state.add_reservation(reservation)?;
        } else {
            return Err(BlixardError::NodeNotFound { node_id });
        }
        
        Ok(())
    }
}

/// Cluster-wide resource summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterResourceSummary {
    pub total_nodes: u32,
    pub total_vcpus: u32,
    pub used_vcpus: u32,
    pub total_memory_mb: u64,
    pub used_memory_mb: u64,
    pub total_disk_gb: u64,
    pub used_disk_gb: u64,
    pub total_running_vms: u32,
    pub nodes: Vec<NodeResourceUsage>,
}

impl ClusterResourceSummary {
    /// Calculate cluster-wide resource utilization percentages
    pub fn utilization_percentages(&self) -> (f64, f64, f64) {
        let cpu_util = if self.total_vcpus > 0 {
            (self.used_vcpus as f64 / self.total_vcpus as f64) * 100.0
        } else {
            0.0
        };
        
        let memory_util = if self.total_memory_mb > 0 {
            (self.used_memory_mb as f64 / self.total_memory_mb as f64) * 100.0
        } else {
            0.0
        };
        
        let disk_util = if self.total_disk_gb > 0 {
            (self.used_disk_gb as f64 / self.total_disk_gb as f64) * 100.0
        } else {
            0.0
        };
        
        (cpu_util, memory_util, disk_util)
    }
}

impl VmScheduler {
    /// Build anti-affinity checker from current VM distribution
    async fn build_anti_affinity_checker(&self) -> BlixardResult<AntiAffinityChecker> {
        let read_txn = self.database.begin_read()?;
        let vm_table = read_txn.open_table(VM_STATE_TABLE)?;
        
        let mut vm_distribution = Vec::new();
        
        // Collect VM distribution with their anti-affinity groups
        for vm_result in vm_table.iter()? {
            let (vm_name_key, vm_data) = vm_result?;
            let vm_name = vm_name_key.value().to_string();
            
            let vm_state: crate::types::VmState = bincode::deserialize(vm_data.value())?;
            
            // Only consider running/starting VMs
            match vm_state.status {
                crate::types::VmStatus::Running | crate::types::VmStatus::Starting => {
                    let mut groups = Vec::new();
                    
                    // Extract anti-affinity groups from this VM
                    if let Some(rules) = &vm_state.config.anti_affinity {
                        for rule in &rules.rules {
                            groups.push(rule.group_key.clone());
                        }
                    }
                    
                    if !groups.is_empty() {
                        vm_distribution.push((vm_name, groups, vm_state.node_id));
                    }
                }
                _ => {}
            }
        }
        
        Ok(AntiAffinityChecker::new(vm_distribution))
    }
    
    /// Select the best node considering anti-affinity soft constraints
    fn select_best_node_with_anti_affinity<'a>(
        &self,
        candidates: Vec<&'a NodeResourceUsage>,
        strategy: &PlacementStrategy,
        anti_affinity_rules: Option<&AntiAffinityRules>,
        checker: Option<&AntiAffinityChecker>,
    ) -> BlixardResult<&'a NodeResourceUsage> {
        // If no anti-affinity rules, use regular selection
        if anti_affinity_rules.is_none() || checker.is_none() {
            return self.select_best_node(candidates, strategy);
        }
        
        let rules = anti_affinity_rules.unwrap();
        let checker = checker.unwrap();
        
        // Calculate combined scores (placement score - anti-affinity penalty)
        let scored_candidates: Vec<(&NodeResourceUsage, f64)> = candidates
            .iter()
            .map(|&usage| {
                let placement_score = usage.placement_score(strategy);
                let anti_affinity_penalty = checker.calculate_soft_constraint_penalty(rules, usage.node_id);
                
                // Normalize penalty to be in similar range as placement score (0-1)
                let normalized_penalty = anti_affinity_penalty / 10.0; // Adjust scale as needed
                let combined_score = placement_score - normalized_penalty;
                
                (usage, combined_score)
            })
            .collect();
        
        // Log scoring details for debugging
        for (usage, score) in &scored_candidates {
            let placement_score = usage.placement_score(strategy);
            let penalty = checker.calculate_soft_constraint_penalty(rules, usage.node_id);
            info!(
                "Node {} scoring - placement: {:.3}, anti-affinity penalty: {:.3}, combined: {:.3}",
                usage.node_id, placement_score, penalty, score
            );
        }
        
        // Select node with highest combined score
        scored_candidates
            .into_iter()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(usage, _)| usage)
            .ok_or_else(|| BlixardError::SchedulingError {
                message: "No candidate nodes available after anti-affinity scoring".to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    fn create_test_database() -> (Arc<Database>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let database = Arc::new(Database::create(&db_path).unwrap());
        (database, temp_dir)
    }
    
    #[tokio::test]
    async fn test_placement_strategy_scoring() {
        let usage = NodeResourceUsage {
            node_id: 1,
            capabilities: WorkerCapabilities {
                cpu_cores: 8,
                memory_mb: 16384,
                disk_gb: 100,
                features: vec!["microvm".to_string()],
            },
            used_vcpus: 4,
            used_memory_mb: 8192,
            used_disk_gb: 50,
            running_vms: 2,
        };
        
        // Most available should favor less utilized nodes
        let most_score = usage.placement_score(&PlacementStrategy::MostAvailable);
        assert!(most_score == 0.5); // 50% available across all resources
        
        // Least available should favor more utilized nodes  
        let least_score = usage.placement_score(&PlacementStrategy::LeastAvailable);
        assert!(least_score == 0.5); // 50% used across all resources
        
        // Round robin should favor nodes with fewer VMs
        let round_score = usage.placement_score(&PlacementStrategy::RoundRobin);
        assert!(round_score == 1.0 / 3.0); // 1/(1+2)
    }
    
    #[tokio::test] 
    async fn test_resource_requirements_from_vm_config() {
        let vm_config = VmConfig {
            name: "test-vm".to_string(),
            config_path: "".to_string(),
            vcpus: 4,
            memory: 8192,
            tenant_id: "default".to_string(),
            ip_address: None,
            metadata: None,
            anti_affinity: None,
        };
        
        let requirements = VmResourceRequirements::from(&vm_config);
        assert_eq!(requirements.vcpus, 4);
        assert_eq!(requirements.memory_mb, 8192);
        assert_eq!(requirements.disk_gb, 5); // Default
        assert_eq!(requirements.required_features, vec!["microvm".to_string()]);
    }
    
    #[tokio::test]
    async fn test_can_accommodate() {
        let usage = NodeResourceUsage {
            node_id: 1,
            capabilities: WorkerCapabilities {
                cpu_cores: 8,
                memory_mb: 16384,
                disk_gb: 100,
                features: vec!["microvm".to_string()],
            },
            used_vcpus: 4,
            used_memory_mb: 8192,
            used_disk_gb: 50,
            running_vms: 2,
        };
        
        let small_requirements = VmResourceRequirements {
            vcpus: 2,
            memory_mb: 4096,
            disk_gb: 10,
            required_features: vec!["microvm".to_string()],
        };
        
        let large_requirements = VmResourceRequirements {
            vcpus: 8,
            memory_mb: 16384,
            disk_gb: 100,
            required_features: vec!["microvm".to_string()],
        };
        
        assert!(usage.can_accommodate(&small_requirements));
        assert!(!usage.can_accommodate(&large_requirements));
    }
}