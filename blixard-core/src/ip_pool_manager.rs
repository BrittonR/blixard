//! IP Pool Manager - Handles IP allocation and management with Raft consensus
//!
//! This module provides the core logic for managing IP pools and allocations:
//! - Distributed IP allocation through Raft consensus
//! - Automatic pool selection based on strategy
//! - Integration with VM lifecycle (allocation on create, release on delete)
//! - Persistent storage of pools and allocations

use std::collections::{HashMap, BTreeSet};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::{
    error::{BlixardError, BlixardResult},
    ip_pool::{
        IpAllocation, IpAllocationRequest, IpAllocationResult, IpPoolCommand, IpPoolConfig,
        IpPoolId, IpPoolSelectionStrategy, IpPoolState, NetworkConfig,
    },
    types::VmId,
};

/// IP Pool Manager - manages all IP pools and allocations
pub struct IpPoolManager {
    /// All configured IP pools
    pools: Arc<RwLock<HashMap<IpPoolId, IpPoolState>>>,
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

        // Load pools
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
            
            pools_map.insert(state.config.id, state);
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
            IpPoolCommand::ReserveIps { pool_id, ips } => {
                self.reserve_ips(pool_id, ips).await
            }
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
        
        let state = IpPoolState::new(config.clone());
        pools.insert(config.id, state);
        
        info!("Created IP pool {} ({})", config.id, config.name);
        Ok(())
    }

    /// Update an existing pool
    async fn update_pool(&self, config: IpPoolConfig) -> BlixardResult<()> {
        config.validate()?;
        
        let mut pools = self.pools.write().await;
        
        let state = pools.get_mut(&config.id).ok_or_else(|| BlixardError::NotFound {
            resource: format!("IP pool {}", config.id),
        })?;
        
        // Preserve allocation state
        let allocated_ips = state.allocated_ips.clone();
        let total_allocated = state.total_allocated;
        let last_allocation = state.last_allocation;
        
        // Update config
        state.config = config.clone();
        state.allocated_ips = allocated_ips;
        state.total_allocated = total_allocated;
        state.last_allocation = last_allocation;
        
        info!("Updated IP pool {} ({})", config.id, config.name);
        Ok(())
    }

    /// Delete a pool (only if no allocations exist)
    async fn delete_pool(&self, pool_id: IpPoolId) -> BlixardResult<()> {
        let mut pools = self.pools.write().await;
        
        let state = pools.get(&pool_id).ok_or_else(|| BlixardError::NotFound {
            resource: format!("IP pool {}", pool_id),
        })?;
        
        if !state.allocated_ips.is_empty() {
            return Err(BlixardError::InvalidOperation {
                operation: "delete pool".to_string(),
                reason: format!(
                    "Pool {} has {} allocated IPs",
                    pool_id,
                    state.allocated_ips.len()
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
        
        let state = pools.get_mut(&pool_id).ok_or_else(|| BlixardError::NotFound {
            resource: format!("IP pool {}", pool_id),
        })?;
        
        state.config.enabled = enabled;
        info!("Set IP pool {} enabled={}", pool_id, enabled);
        Ok(())
    }

    /// Reserve IPs in a pool
    async fn reserve_ips(&self, pool_id: IpPoolId, ips: BTreeSet<IpAddr>) -> BlixardResult<()> {
        let mut pools = self.pools.write().await;
        
        let state = pools.get_mut(&pool_id).ok_or_else(|| BlixardError::NotFound {
            resource: format!("IP pool {}", pool_id),
        })?;
        
        // Check all IPs are in subnet and not allocated
        for ip in &ips {
            if !state.config.subnet.contains(ip) {
                return Err(BlixardError::InvalidOperation {
                    operation: "reserve IP".to_string(),
                    reason: format!("IP {} is not in pool subnet", ip),
                });
            }
            
            if state.allocated_ips.contains(ip) {
                return Err(BlixardError::InvalidOperation {
                    operation: "reserve IP".to_string(),
                    reason: format!("IP {} is already allocated", ip),
                });
            }
        }
        
        state.config.reserved_ips.extend(ips.iter().cloned());
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
        
        let state = pools.get_mut(&pool_id).ok_or_else(|| BlixardError::NotFound {
            resource: format!("IP pool {}", pool_id),
        })?;
        
        for ip in &ips {
            state.config.reserved_ips.remove(ip);
        }
        
        info!("Released {} reserved IPs in pool {}", ips.len(), pool_id);
        Ok(())
    }

    /// Allocate an IP address for a VM
    pub async fn allocate_ip(
        &self,
        request: IpAllocationRequest,
    ) -> BlixardResult<IpAllocationResult> {
        let pools = self.pools.read().await;
        
        // Select pool based on request
        let pool_id = if let Some(preferred) = request.preferred_pool {
            // Verify preferred pool exists and has capacity
            let state = pools.get(&preferred).ok_or_else(|| BlixardError::NotFound {
                resource: format!("IP pool {}", preferred),
            })?;
            
            if !state.has_capacity() {
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
        
        // Allocate IP from selected pool
        let mut pools = self.pools.write().await;
        let state = pools.get_mut(&pool_id).unwrap();
        
        let ip = state.next_available_ip().ok_or_else(|| BlixardError::ResourceExhausted {
            resource: format!("IP pool {}", pool_id),
        })?;
        
        // Mark IP as allocated
        state.allocated_ips.insert(ip);
        state.total_allocated += 1;
        state.last_allocation = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
        );
        
        // Create allocation record
        let allocation = IpAllocation {
            ip,
            pool_id,
            vm_id: request.vm_id,
            mac_address: request.mac_address.clone(),
            allocated_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
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
            subnet: state.config.subnet,
            gateway: state.config.gateway,
            dns_servers: state.config.dns_servers.clone(),
            vlan_id: state.config.vlan_id,
            mac_address: request.mac_address,
        };
        
        info!(
            "Allocated IP {} from pool {} to VM {}",
            ip, pool_id, request.vm_id
        );
        
        Ok(IpAllocationResult {
            allocation,
            network_config,
        })
    }

    /// Release all IPs allocated to a VM
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
                // Update pool state
                if let Some(state) = pools.get_mut(&allocation.pool_id) {
                    state.allocated_ips.remove(&ip);
                    info!(
                        "Released IP {} from pool {} (was allocated to VM {})",
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
        pools: &HashMap<IpPoolId, IpPoolState>,
        request: &IpAllocationRequest,
    ) -> BlixardResult<IpPoolId> {
        // Filter eligible pools
        let mut eligible_pools: Vec<(&IpPoolId, &IpPoolState)> = pools
            .iter()
            .filter(|(_, state)| {
                // Must have capacity and be enabled
                if !state.has_capacity() {
                    return false;
                }
                
                // Check required tags
                for (key, value) in &request.required_tags {
                    if state.config.tags.get(key) != Some(value) {
                        return false;
                    }
                }
                
                // Check topology hint if specified
                if let Some(hint) = &request.topology_hint {
                    if let Some(pool_hint) = &state.config.topology_hint {
                        if pool_hint != hint {
                            return false;
                        }
                    }
                }
                
                true
            })
            .collect();
        
        if eligible_pools.is_empty() {
            return Err(BlixardError::ResourceExhausted {
                resource: "IP pools matching requirements".to_string(),
            });
        }
        
        // Select based on strategy
        let selected = match request.selection_strategy {
            IpPoolSelectionStrategy::LeastUtilized => {
                eligible_pools.sort_by(|(_, a), (_, b)| {
                    a.utilization_percent()
                        .partial_cmp(&b.utilization_percent())
                        .unwrap()
                });
                eligible_pools[0].0
            }
            IpPoolSelectionStrategy::MostUtilized => {
                eligible_pools.sort_by(|(_, a), (_, b)| {
                    b.utilization_percent()
                        .partial_cmp(&a.utilization_percent())
                        .unwrap()
                });
                eligible_pools[0].0
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
                    if let Some((id, _)) = eligible_pools.iter().find(|(_, state)| {
                        state.config.topology_hint.as_ref() == Some(hint)
                    }) {
                        return Ok(**id);
                    }
                }
                // Fall back to least utilized
                eligible_pools.sort_by(|(_, a), (_, b)| {
                    a.utilization_percent()
                        .partial_cmp(&b.utilization_percent())
                        .unwrap()
                });
                eligible_pools[0].0
            }
            IpPoolSelectionStrategy::Random => {
                let mut rng = rand::thread_rng();
                eligible_pools.choose(&mut rng).unwrap().0
            }
        };
        
        Ok(*selected)
    }

    /// Get current state of all pools
    pub async fn get_pools(&self) -> Vec<IpPoolState> {
        let pools = self.pools.read().await;
        pools.values().cloned().collect()
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

    /// Get pool statistics
    pub async fn get_pool_stats(&self, pool_id: IpPoolId) -> BlixardResult<PoolStats> {
        let pools = self.pools.read().await;
        let state = pools.get(&pool_id).ok_or_else(|| BlixardError::NotFound {
            resource: format!("IP pool {}", pool_id),
        })?;
        
        Ok(PoolStats {
            pool_id,
            total_ips: state.config.total_capacity(),
            allocated_ips: state.allocated_ips.len() as u64,
            reserved_ips: state.config.reserved_ips.len() as u64,
            available_ips: state.config.total_capacity() - state.allocated_ips.len() as u64,
            utilization_percent: state.utilization_percent(),
            last_allocation: state.last_allocation,
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
        
        manager.load_from_storage(vec![pool1, pool2], vec![]).await.unwrap();
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
        assert_eq!(result.network_config.gateway, IpAddr::from_str("10.0.0.1").unwrap());
        
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
        
        manager.process_command(IpPoolCommand::CreatePool(pool.clone())).await.unwrap();
        
        // Update pool
        let mut updated = pool.clone();
        updated.name = "updated".to_string();
        manager.process_command(IpPoolCommand::UpdatePool(updated)).await.unwrap();
        
        // Reserve IPs
        let mut reserved = BTreeSet::new();
        reserved.insert(IpAddr::from_str("192.168.0.50").unwrap());
        manager.process_command(IpPoolCommand::ReserveIps {
            pool_id: IpPoolId(1),
            ips: reserved.clone(),
        }).await.unwrap();
        
        // Disable pool
        manager.process_command(IpPoolCommand::SetPoolEnabled {
            pool_id: IpPoolId(1),
            enabled: false,
        }).await.unwrap();
        
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