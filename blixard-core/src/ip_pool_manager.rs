//! IP Pool Manager - Handles IP allocation and management with Raft consensus
//!
//! This module provides the core logic for managing IP pools and allocations:
//! - Distributed IP allocation through Raft consensus
//! - Automatic pool selection based on strategy
//! - Integration with VM lifecycle (allocation on create, release on delete)
//! - Persistent storage of pools and allocations

use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::{
    error::{BlixardError, BlixardResult},
    get_mut_or_not_found,
    ip_pool::{
        EnhancedIpPoolState, IpAllocation, IpAllocationRequest, IpAllocationResult, IpPoolCommand,
        IpPoolConfig, IpPoolId, IpPoolSelectionStrategy, IpPoolState, NetworkConfig,
    },
    types::VmId,
    unwrap_helpers::{choose_random, max_by_safe, min_by_safe, time_since_epoch_safe},
};

/// IP Pool Manager - manages all IP pools and allocations
pub struct IpPoolManager {
    /// All configured IP pools with ResourcePool integration
    pools: Arc<RwLock<HashMap<IpPoolId, EnhancedIpPoolState>>>,
    /// IP to allocation mapping for quick lookup
    allocations: Arc<RwLock<HashMap<IpAddr, IpAllocation>>>,
    /// VM to IP mapping for quick lookup
    vm_allocations: Arc<RwLock<HashMap<VmId, Vec<IpAddr>>>>,
    /// Round-robin counter for pool selection
    round_robin_counter: Arc<RwLock<usize>>,
}

impl IpPoolManager {
    /// Create a new IP pool manager
    pub fn new() -> Self {
        Self {
            pools: Arc::new(RwLock::new(HashMap::new())),
            allocations: Arc::new(RwLock::new(HashMap::new())),
            vm_allocations: Arc::new(RwLock::new(HashMap::new())),
            round_robin_counter: Arc::new(RwLock::new(0)),
        }
    }

    /// Load pools and allocations from storage (called during initialization)
    pub async fn load_from_storage(
        &self,
        pools: Vec<IpPoolConfig>,
        allocations: Vec<IpAllocation>,
    ) -> BlixardResult<()> {
        let mut pools_map = self.pools.write().await;
        let mut allocations_map = self.allocations.write().await;
        let mut vm_allocations_map = self.vm_allocations.write().await;

        // Load pools with ResourcePool integration
        for config in pools {
            config.validate()?;
            let mut state = IpPoolState::new(config);

            // Mark allocated IPs in pool state
            for allocation in &allocations {
                if allocation.pool_id == state.config.id {
                    state.allocated_ips.insert(allocation.ip);
                    state.total_allocated += 1;
                }
            }

            // Create enhanced state with ResourcePool
            let enhanced_state = EnhancedIpPoolState::from_existing_state(state).await?;
            pools_map.insert(enhanced_state.state.config.id, enhanced_state);
        }

        // Load allocations
        for allocation in allocations {
            allocations_map.insert(allocation.ip, allocation.clone());

            vm_allocations_map
                .entry(allocation.vm_id)
                .or_insert_with(Vec::new)
                .push(allocation.ip);
        }

        info!(
            "Loaded {} IP pools with {} allocations",
            pools_map.len(),
            allocations_map.len()
        );

        Ok(())
    }

    /// Process an IP pool command (called by Raft state machine)
    pub async fn process_command(&self, command: IpPoolCommand) -> BlixardResult<()> {
        match command {
            IpPoolCommand::CreatePool(config) => self.create_pool(config).await,
            IpPoolCommand::UpdatePool(config) => self.update_pool(config).await,
            IpPoolCommand::DeletePool(pool_id) => self.delete_pool(pool_id).await,
            IpPoolCommand::SetPoolEnabled { pool_id, enabled } => {
                self.set_pool_enabled(pool_id, enabled).await
            }
            IpPoolCommand::ReserveIps { pool_id, ips } => self.reserve_ips(pool_id, ips).await,
            IpPoolCommand::ReleaseReservedIps { pool_id, ips } => {
                self.release_reserved_ips(pool_id, ips).await
            }
        }
    }

    /// Create a new IP pool
    async fn create_pool(&self, config: IpPoolConfig) -> BlixardResult<()> {
        config.validate()?;

        let mut pools = self.pools.write().await;

        if pools.contains_key(&config.id) {
            return Err(BlixardError::AlreadyExists {
                resource: format!("IP pool {}", config.id),
            });
        }

        let enhanced_state = EnhancedIpPoolState::new(config.clone()).await?;
        pools.insert(config.id, enhanced_state);

        info!("Created IP pool {} ({})", config.id, config.name);
        Ok(())
    }

    /// Update an existing pool
    async fn update_pool(&self, config: IpPoolConfig) -> BlixardResult<()> {
        config.validate()?;

        let mut pools = self.pools.write().await;

        let enhanced_state = pools
            .get_mut(&config.id)
            .ok_or_else(|| BlixardError::NotFound {
                resource: format!("IP pool {}", config.id),
            })?;

        // Preserve allocation state
        let allocated_ips = enhanced_state.state.allocated_ips.clone();
        let total_allocated = enhanced_state.state.total_allocated;
        let last_allocation = enhanced_state.state.last_allocation;

        // Update config in the basic state
        enhanced_state.state.config = config.clone();
        enhanced_state.state.allocated_ips = allocated_ips;
        enhanced_state.state.total_allocated = total_allocated;
        enhanced_state.state.last_allocation = last_allocation;

        // Sync the enhanced state to reflect changes
        enhanced_state.sync_state().await;

        info!("Updated IP pool {} ({})", config.id, config.name);
        Ok(())
    }

    /// Delete a pool (only if no allocations exist)
    async fn delete_pool(&self, pool_id: IpPoolId) -> BlixardResult<()> {
        let mut pools = self.pools.write().await;

        let state = pools.get(&pool_id).ok_or_else(|| BlixardError::NotFound {
            resource: format!("IP pool {}", pool_id),
        })?;

        if !state.state.allocated_ips.is_empty() {
            return Err(BlixardError::InvalidOperation {
                operation: "delete pool".to_string(),
                reason: format!(
                    "Pool {} has {} allocated IPs",
                    pool_id,
                    state.state.allocated_ips.len()
                ),
            });
        }

        pools.remove(&pool_id);
        info!("Deleted IP pool {}", pool_id);
        Ok(())
    }

    /// Enable or disable a pool
    async fn set_pool_enabled(&self, pool_id: IpPoolId, enabled: bool) -> BlixardResult<()> {
        let mut pools = self.pools.write().await;

        let state = pools
            .get_mut(&pool_id)
            .ok_or_else(|| BlixardError::NotFound {
                resource: format!("IP pool {}", pool_id),
            })?;

        state.state.config.enabled = enabled;
        info!("Set IP pool {} enabled={}", pool_id, enabled);
        Ok(())
    }

    /// Reserve IPs in a pool
    async fn reserve_ips(&self, pool_id: IpPoolId, ips: BTreeSet<IpAddr>) -> BlixardResult<()> {
        let mut pools = self.pools.write().await;

        let state = pools
            .get_mut(&pool_id)
            .ok_or_else(|| BlixardError::NotFound {
                resource: format!("IP pool {}", pool_id),
            })?;

        // Check all IPs are in subnet and not allocated
        for ip in &ips {
            if !state.state.config.subnet.contains(ip) {
                return Err(BlixardError::InvalidOperation {
                    operation: "reserve IP".to_string(),
                    reason: format!("IP {} is not in pool subnet", ip),
                });
            }

            if state.state.allocated_ips.contains(ip) {
                return Err(BlixardError::InvalidOperation {
                    operation: "reserve IP".to_string(),
                    reason: format!("IP {} is already allocated", ip),
                });
            }
        }

        state.state.config.reserved_ips.extend(ips.iter().cloned());
        info!("Reserved {} IPs in pool {}", ips.len(), pool_id);
        Ok(())
    }

    /// Release reserved IPs in a pool
    async fn release_reserved_ips(
        &self,
        pool_id: IpPoolId,
        ips: BTreeSet<IpAddr>,
    ) -> BlixardResult<()> {
        let mut pools = self.pools.write().await;

        let state = pools
            .get_mut(&pool_id)
            .ok_or_else(|| BlixardError::NotFound {
                resource: format!("IP pool {}", pool_id),
            })?;

        for ip in &ips {
            state.state.config.reserved_ips.remove(ip);
        }

        info!("Released {} reserved IPs in pool {}", ips.len(), pool_id);
        Ok(())
    }

    /// Allocate an IP address for a VM using ResourcePool pattern
    pub async fn allocate_ip(
        &self,
        request: IpAllocationRequest,
    ) -> BlixardResult<IpAllocationResult> {
        let pools = self.pools.read().await;

        // Select pool based on request
        let pool_id = if let Some(preferred) = request.preferred_pool {
            // Verify preferred pool exists and has capacity
            let enhanced_state = pools
                .get(&preferred)
                .ok_or_else(|| BlixardError::NotFound {
                    resource: format!("IP pool {}", preferred),
                })?;

            if !enhanced_state.has_capacity().await {
                return Err(BlixardError::ResourceExhausted {
                    resource: format!("IP pool {}", preferred),
                });
            }

            preferred
        } else {
            // Select pool based on strategy
            self.select_pool(&pools, &request).await?
        };

        drop(pools);

        // Allocate IP from selected pool using ResourcePool
        let mut pools = self.pools.write().await;
        let enhanced_state = get_mut_or_not_found!(pools.get_mut(&pool_id), "IP Pool", pool_id);

        // Use ResourcePool for allocation
        let ip_address = enhanced_state.allocate_ip().await?;
        let ip = ip_address.ip;

        // Create allocation record
        let allocation = IpAllocation {
            ip,
            pool_id,
            vm_id: request.vm_id,
            mac_address: request.mac_address.clone(),
            allocated_at: time_since_epoch_safe() as i64,
            expires_at: None,
        };

        // Update allocation maps
        let mut allocations = self.allocations.write().await;
        let mut vm_allocations = self.vm_allocations.write().await;

        allocations.insert(ip, allocation.clone());
        vm_allocations
            .entry(request.vm_id)
            .or_insert_with(Vec::new)
            .push(ip);

        // Build network config
        let network_config = NetworkConfig {
            ip_address: ip,
            subnet: enhanced_state.state.config.subnet,
            gateway: enhanced_state.state.config.gateway,
            dns_servers: enhanced_state.state.config.dns_servers.clone(),
            vlan_id: enhanced_state.state.config.vlan_id,
            mac_address: request.mac_address,
        };

        info!(
            "Allocated IP {} from pool {} to VM {} using ResourcePool",
            ip, pool_id, request.vm_id
        );

        Ok(IpAllocationResult {
            allocation,
            network_config,
        })
    }

    /// Release all IPs allocated to a VM using ResourcePool pattern
    pub async fn release_vm_ips(&self, vm_id: VmId) -> BlixardResult<()> {
        let mut allocations = self.allocations.write().await;
        let mut vm_allocations = self.vm_allocations.write().await;
        let mut pools = self.pools.write().await;

        let ips = match vm_allocations.remove(&vm_id) {
            Some(ips) => ips,
            None => {
                debug!("No IPs allocated to VM {}", vm_id);
                return Ok(());
            }
        };

        for ip in ips {
            if let Some(allocation) = allocations.remove(&ip) {
                // Release IP using ResourcePool
                if let Some(enhanced_state) = pools.get_mut(&allocation.pool_id) {
                    enhanced_state.release_ip(ip).await?;
                    info!(
                        "Released IP {} from pool {} (was allocated to VM {}) using ResourcePool",
                        ip, allocation.pool_id, vm_id
                    );
                }
            }
        }

        Ok(())
    }

    /// Select a pool based on strategy and requirements
    async fn select_pool(
        &self,
        pools: &HashMap<IpPoolId, EnhancedIpPoolState>,
        request: &IpAllocationRequest,
    ) -> BlixardResult<IpPoolId> {
        // Filter eligible pools - async capacity check
        let mut eligible_pools: Vec<(&IpPoolId, &EnhancedIpPoolState)> = Vec::new();

        for (pool_id, enhanced_state) in pools.iter() {
            // Must be enabled
            if !enhanced_state.state.config.enabled {
                continue;
            }

            // Check required tags
            let tags_match = request
                .required_tags
                .iter()
                .all(|(key, value)| enhanced_state.state.config.tags.get(key) == Some(value));

            if !tags_match {
                continue;
            }

            // Check topology hint if specified
            if let Some(hint) = &request.topology_hint {
                if let Some(pool_hint) = &enhanced_state.state.config.topology_hint {
                    if pool_hint != hint {
                        continue;
                    }
                }
            }

            // Check capacity using async method
            if enhanced_state.has_capacity().await {
                eligible_pools.push((pool_id, enhanced_state));
            }
        }

        if eligible_pools.is_empty() {
            return Err(BlixardError::ResourceExhausted {
                resource: "IP pools matching requirements".to_string(),
            });
        }

        // Select based on strategy using ResourcePool information
        let selected = match request.selection_strategy {
            IpPoolSelectionStrategy::LeastUtilized => {
                // Get utilization for all pools and find minimum
                let mut pool_utilizations = Vec::new();
                for (pool_id, enhanced_state) in &eligible_pools {
                    let utilization = enhanced_state.utilization_percent().await;
                    pool_utilizations.push((*pool_id, utilization));
                }
                let (pool_id, _) =
                    min_by_safe(pool_utilizations.into_iter(), |(_, utilization)| {
                        // Convert to integer for comparison (multiply by 1000 for precision)
                        (utilization * 1000.0) as u64
                    })?;
                pool_id
            }
            IpPoolSelectionStrategy::MostUtilized => {
                // Get utilization for all pools and find maximum
                let mut pool_utilizations = Vec::new();
                for (pool_id, enhanced_state) in &eligible_pools {
                    let utilization = enhanced_state.utilization_percent().await;
                    pool_utilizations.push((*pool_id, utilization));
                }
                let (pool_id, _) =
                    max_by_safe(pool_utilizations.into_iter(), |(_, utilization)| {
                        // Convert to integer for comparison (multiply by 1000 for precision)
                        (utilization * 1000.0) as u64
                    })?;
                pool_id
            }
            IpPoolSelectionStrategy::RoundRobin => {
                let mut counter = self.round_robin_counter.write().await;
                let index = *counter % eligible_pools.len();
                *counter = (*counter + 1) % eligible_pools.len();
                eligible_pools[index].0
            }
            IpPoolSelectionStrategy::TopologyAware => {
                // Prefer pools with matching topology, fall back to least utilized
                if let Some(hint) = &request.topology_hint {
                    if let Some((id, _)) = eligible_pools.iter().find(|(_, enhanced_state)| {
                        enhanced_state.state.config.topology_hint.as_ref() == Some(hint)
                    }) {
                        return Ok(**id);
                    }
                }
                // Fall back to least utilized
                let mut pool_utilizations = Vec::new();
                for (pool_id, enhanced_state) in &eligible_pools {
                    let utilization = enhanced_state.utilization_percent().await;
                    pool_utilizations.push((*pool_id, utilization));
                }
                let (pool_id, _) =
                    min_by_safe(pool_utilizations.into_iter(), |(_, utilization)| {
                        // Convert to integer for comparison (multiply by 1000 for precision)
                        (utilization * 1000.0) as u64
                    })?;
                pool_id
            }
            IpPoolSelectionStrategy::Random => {
                let pool_ids: Vec<_> = eligible_pools.iter().map(|(id, _)| *id).collect();
                *choose_random(&pool_ids)?
            }
        };

        Ok(*selected)
    }

    /// Get current state of all pools
    pub async fn get_pools(&self) -> Vec<IpPoolState> {
        let pools = self.pools.read().await;
        pools
            .values()
            .map(|enhanced| enhanced.state.clone())
            .collect()
    }

    /// Get allocations for a specific VM
    pub async fn get_vm_allocations(&self, vm_id: VmId) -> Vec<IpAllocation> {
        let allocations = self.allocations.read().await;
        let vm_allocations = self.vm_allocations.read().await;

        vm_allocations
            .get(&vm_id)
            .map(|ips| {
                ips.iter()
                    .filter_map(|ip| allocations.get(ip).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get pool statistics with ResourcePool integration
    pub async fn get_pool_stats(&self, pool_id: IpPoolId) -> BlixardResult<PoolStats> {
        let pools = self.pools.read().await;
        let enhanced_state = pools.get(&pool_id).ok_or_else(|| BlixardError::NotFound {
            resource: format!("IP pool {}", pool_id),
        })?;

        // Use ResourcePool stats if available, fall back to basic stats
        let utilization_percent = enhanced_state.utilization_percent().await;
        let available_ips = enhanced_state.state.config.total_capacity()
            - enhanced_state.state.allocated_ips.len() as u64;

        Ok(PoolStats {
            pool_id,
            total_ips: enhanced_state.state.config.total_capacity(),
            allocated_ips: enhanced_state.state.allocated_ips.len() as u64,
            reserved_ips: enhanced_state.state.config.reserved_ips.len() as u64,
            available_ips,
            utilization_percent,
            last_allocation: enhanced_state.state.last_allocation,
        })
    }
}

/// Statistics for an IP pool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    pub pool_id: IpPoolId,
    pub total_ips: u64,
    pub allocated_ips: u64,
    pub reserved_ips: u64,
    pub available_ips: u64,
    pub utilization_percent: f64,
    pub last_allocation: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    async fn create_test_manager() -> IpPoolManager {
        let manager = IpPoolManager::new();

        let pool1 = IpPoolConfig {
            id: IpPoolId(1),
            name: "pool1".to_string(),
            subnet: ipnet::IpNet::from_str("10.0.0.0/24").unwrap(),
            vlan_id: Some(100),
            gateway: IpAddr::from_str("10.0.0.1").unwrap(),
            dns_servers: vec![IpAddr::from_str("8.8.8.8").unwrap()],
            allocation_start: IpAddr::from_str("10.0.0.10").unwrap(),
            allocation_end: IpAddr::from_str("10.0.0.20").unwrap(),
            topology_hint: Some("zone-a".to_string()),
            reserved_ips: BTreeSet::new(),
            enabled: true,
            tags: HashMap::new(),
        };

        let pool2 = IpPoolConfig {
            id: IpPoolId(2),
            name: "pool2".to_string(),
            subnet: ipnet::IpNet::from_str("10.1.0.0/24").unwrap(),
            vlan_id: Some(200),
            gateway: IpAddr::from_str("10.1.0.1").unwrap(),
            dns_servers: vec![IpAddr::from_str("8.8.8.8").unwrap()],
            allocation_start: IpAddr::from_str("10.1.0.10").unwrap(),
            allocation_end: IpAddr::from_str("10.1.0.20").unwrap(),
            topology_hint: Some("zone-b".to_string()),
            reserved_ips: BTreeSet::new(),
            enabled: true,
            tags: HashMap::new(),
        };

        manager
            .load_from_storage(vec![pool1, pool2], vec![])
            .await
            .unwrap();
        manager
    }

    #[tokio::test]
    async fn test_ip_allocation() {
        let manager = create_test_manager().await;

        let request = IpAllocationRequest {
            vm_id: VmId::new(),
            preferred_pool: None,
            required_tags: HashMap::new(),
            topology_hint: None,
            selection_strategy: IpPoolSelectionStrategy::LeastUtilized,
            mac_address: "02:00:00:00:00:01".to_string(),
        };

        let result = manager.allocate_ip(request).await.unwrap();
        assert_eq!(result.allocation.ip, IpAddr::from_str("10.0.0.10").unwrap());
        assert_eq!(
            result.network_config.gateway,
            IpAddr::from_str("10.0.0.1").unwrap()
        );

        // Verify allocation is tracked
        let allocations = manager.get_vm_allocations(result.allocation.vm_id).await;
        assert_eq!(allocations.len(), 1);
        assert_eq!(allocations[0].ip, result.allocation.ip);
    }

    #[tokio::test]
    async fn test_pool_selection_strategies() {
        let manager = create_test_manager().await;

        // Allocate some IPs from pool1 to make it more utilized
        for i in 0..5 {
            let request = IpAllocationRequest {
                vm_id: VmId::new(),
                preferred_pool: Some(IpPoolId(1)),
                required_tags: HashMap::new(),
                topology_hint: None,
                selection_strategy: IpPoolSelectionStrategy::LeastUtilized,
                mac_address: format!("02:00:00:00:00:{:02x}", i + 1),
            };
            manager.allocate_ip(request).await.unwrap();
        }

        // Test least utilized strategy - should pick pool2
        let request = IpAllocationRequest {
            vm_id: VmId::new(),
            preferred_pool: None,
            required_tags: HashMap::new(),
            topology_hint: None,
            selection_strategy: IpPoolSelectionStrategy::LeastUtilized,
            mac_address: "02:00:00:00:01:00".to_string(),
        };

        let result = manager.allocate_ip(request).await.unwrap();
        assert_eq!(result.allocation.pool_id, IpPoolId(2));

        // Test topology aware strategy
        let request = IpAllocationRequest {
            vm_id: VmId::new(),
            preferred_pool: None,
            required_tags: HashMap::new(),
            topology_hint: Some("zone-a".to_string()),
            selection_strategy: IpPoolSelectionStrategy::TopologyAware,
            mac_address: "02:00:00:00:02:00".to_string(),
        };

        let result = manager.allocate_ip(request).await.unwrap();
        assert_eq!(result.allocation.pool_id, IpPoolId(1));
    }

    #[tokio::test]
    async fn test_ip_release() {
        let manager = create_test_manager().await;

        let vm_id = VmId::new();
        let request = IpAllocationRequest {
            vm_id,
            preferred_pool: None,
            required_tags: HashMap::new(),
            topology_hint: None,
            selection_strategy: IpPoolSelectionStrategy::LeastUtilized,
            mac_address: "02:00:00:00:00:01".to_string(),
        };

        let result = manager.allocate_ip(request).await.unwrap();
        let allocated_ip = result.allocation.ip;

        // Verify allocation exists
        let allocations = manager.get_vm_allocations(vm_id).await;
        assert_eq!(allocations.len(), 1);

        // Release IPs
        manager.release_vm_ips(vm_id).await.unwrap();

        // Verify allocation is gone
        let allocations = manager.get_vm_allocations(vm_id).await;
        assert_eq!(allocations.len(), 0);

        // Verify IP is available again
        let request2 = IpAllocationRequest {
            vm_id: VmId::new(),
            preferred_pool: Some(result.allocation.pool_id),
            required_tags: HashMap::new(),
            topology_hint: None,
            selection_strategy: IpPoolSelectionStrategy::LeastUtilized,
            mac_address: "02:00:00:00:00:02".to_string(),
        };

        let result2 = manager.allocate_ip(request2).await.unwrap();
        assert_eq!(result2.allocation.ip, allocated_ip);
    }

    #[tokio::test]
    async fn test_pool_commands() {
        let manager = IpPoolManager::new();

        // Create pool
        let pool = IpPoolConfig {
            id: IpPoolId(1),
            name: "test".to_string(),
            subnet: ipnet::IpNet::from_str("192.168.0.0/24").unwrap(),
            vlan_id: None,
            gateway: IpAddr::from_str("192.168.0.1").unwrap(),
            dns_servers: vec![],
            allocation_start: IpAddr::from_str("192.168.0.10").unwrap(),
            allocation_end: IpAddr::from_str("192.168.0.100").unwrap(),
            topology_hint: None,
            reserved_ips: BTreeSet::new(),
            enabled: true,
            tags: HashMap::new(),
        };

        manager
            .process_command(IpPoolCommand::CreatePool(pool.clone()))
            .await
            .unwrap();

        // Update pool
        let mut updated = pool.clone();
        updated.name = "updated".to_string();
        manager
            .process_command(IpPoolCommand::UpdatePool(updated))
            .await
            .unwrap();

        // Reserve IPs
        let mut reserved = BTreeSet::new();
        reserved.insert(IpAddr::from_str("192.168.0.50").unwrap());
        manager
            .process_command(IpPoolCommand::ReserveIps {
                pool_id: IpPoolId(1),
                ips: reserved.clone(),
            })
            .await
            .unwrap();

        // Disable pool
        manager
            .process_command(IpPoolCommand::SetPoolEnabled {
                pool_id: IpPoolId(1),
                enabled: false,
            })
            .await
            .unwrap();

        // Try to allocate from disabled pool - should fail
        let request = IpAllocationRequest {
            vm_id: VmId::new(),
            preferred_pool: Some(IpPoolId(1)),
            required_tags: HashMap::new(),
            topology_hint: None,
            selection_strategy: IpPoolSelectionStrategy::LeastUtilized,
            mac_address: "02:00:00:00:00:01".to_string(),
        };

        assert!(manager.allocate_ip(request).await.is_err());
    }
}
