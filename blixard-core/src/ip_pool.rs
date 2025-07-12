//! IP Pool Management for distributed VM network allocation
//!
//! This module provides IP address pool management with the following features:
//! - Multiple configurable IP pools with different subnets
//! - Distributed consensus through Raft for allocation consistency
//! - Support for VLAN tagging and topology affinity
//! - Automatic IP release on VM deletion
//! - Comprehensive metrics and monitoring

use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::{
    acquire_write_lock,
    error::{BlixardError, BlixardResult},
    patterns::resource_pool::{PoolConfig, PoolableResource, ResourceFactory, ResourcePool},
    types::VmId,
    unwrap_helpers::time_since_epoch_safe,
};
use async_trait::async_trait;
use std::sync::Arc;
use tracing;

/// Unique identifier for an IP pool
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IpPoolId(pub u64);

impl std::fmt::Display for IpPoolId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pool-{}", self.0)
    }
}

/// IP Address resource that can be pooled
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpAddress {
    /// The IP address
    pub ip: IpAddr,
    /// Pool this IP belongs to
    pub pool_id: IpPoolId,
    /// Whether this IP is currently valid for allocation
    pub is_valid: bool,
}

impl PoolableResource for IpAddress {
    fn is_valid(&self) -> bool {
        self.is_valid
    }

    fn reset(&mut self) -> BlixardResult<()> {
        // Reset any temporary state - IP addresses don't need special reset
        self.is_valid = true;
        Ok(())
    }
}

/// Factory for creating IP address resources within a pool
pub struct IpAddressFactory {
    /// Pool configuration
    config: IpPoolConfig,
    /// Currently allocated IPs (shared with pool state)
    allocated_ips: Arc<std::sync::RwLock<BTreeSet<IpAddr>>>,
    /// Next IP to try (for efficiency)
    _next_ip_hint: Arc<std::sync::RwLock<Option<IpAddr>>>,
}

impl IpAddressFactory {
    /// Create a new IP address factory for a pool
    pub fn new(
        config: IpPoolConfig,
        allocated_ips: Arc<std::sync::RwLock<BTreeSet<IpAddr>>>,
    ) -> Self {
        Self {
            config,
            allocated_ips,
            _next_ip_hint: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    /// Find the next available IP in the allocation range
    fn find_next_available_ip(&self) -> crate::error::BlixardResult<Option<IpAddr>> {
        let allocated =
            self.allocated_ips
                .read()
                .map_err(|e| crate::error::BlixardError::LockPoisoned {
                    operation: "read allocated IPs".to_string(),
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Lock poisoned: {}", e),
                    )),
                })?;

        match (self.config.allocation_start, self.config.allocation_end) {
            (IpAddr::V4(start), IpAddr::V4(end)) => {
                let start_u32 = u32::from(start);
                let end_u32 = u32::from(end);

                for i in start_u32..=end_u32 {
                    let ip = IpAddr::V4(Ipv4Addr::from(i));
                    if self.is_ip_available(&ip, &allocated) {
                        return Ok(Some(ip));
                    }
                }
                Ok(None)
            }
            (IpAddr::V6(start), IpAddr::V6(end)) => {
                let start_u128 = u128::from(start);
                let end_u128 = u128::from(end);

                // For IPv6, limit iteration to prevent performance issues
                let max_iterations = 10000;
                for i in 0..max_iterations.min(end_u128 - start_u128 + 1) {
                    let ip = IpAddr::V6(Ipv6Addr::from(start_u128 + i));
                    if self.is_ip_available(&ip, &allocated) {
                        return Ok(Some(ip));
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Check if an IP is available for allocation
    fn is_ip_available(&self, ip: &IpAddr, allocated: &BTreeSet<IpAddr>) -> bool {
        !allocated.contains(ip)
            && !self.config.reserved_ips.contains(ip)
            && self.config.subnet.contains(ip)
            && *ip != self.config.gateway
    }
}

#[async_trait]
impl ResourceFactory<IpAddress> for IpAddressFactory {
    async fn create(&self) -> BlixardResult<IpAddress> {
        let ip = self
            .find_next_available_ip()?
            .ok_or_else(|| BlixardError::ResourceExhausted {
                resource: format!("IP addresses in pool {}", self.config.id),
            })?;

        // Mark IP as allocated
        {
            let mut allocated =
                acquire_write_lock!(self.allocated_ips.write(), "allocate IP address");
            allocated.insert(ip);
        }

        Ok(IpAddress {
            ip,
            pool_id: self.config.id,
            is_valid: true,
        })
    }

    async fn destroy(&self, resource: IpAddress) -> BlixardResult<()> {
        // Remove IP from allocated set
        let mut allocated =
            acquire_write_lock!(self.allocated_ips.write(), "deallocate IP address");
        allocated.remove(&resource.ip);
        Ok(())
    }
}

/// Configuration for an IP address pool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpPoolConfig {
    /// Unique identifier for this pool
    pub id: IpPoolId,
    /// Human-readable name for the pool
    pub name: String,
    /// Network subnet (e.g., 10.0.0.0/24 or 2001:db8::/64)
    pub subnet: IpNet,
    /// Optional VLAN ID for this pool
    pub vlan_id: Option<u16>,
    /// Gateway IP address (must be within subnet)
    pub gateway: IpAddr,
    /// DNS servers for this pool
    pub dns_servers: Vec<IpAddr>,
    /// Start of allocation range (inclusive)
    pub allocation_start: IpAddr,
    /// End of allocation range (inclusive)
    pub allocation_end: IpAddr,
    /// Optional datacenter/zone affinity
    pub topology_hint: Option<String>,
    /// Reserved IP addresses that should not be allocated
    pub reserved_ips: BTreeSet<IpAddr>,
    /// Whether this pool is enabled for allocation
    pub enabled: bool,
    /// Tags for filtering and selection
    pub tags: HashMap<String, String>,
}

impl IpPoolConfig {
    /// Validate the pool configuration
    pub fn validate(&self) -> BlixardResult<()> {
        // Check gateway is within subnet
        if !self.subnet.contains(&self.gateway) {
            return Err(BlixardError::InvalidConfiguration {
                message: format!(
                    "Gateway {} is not within subnet {}",
                    self.gateway, self.subnet
                ),
            });
        }

        // Check allocation range is within subnet
        if !self.subnet.contains(&self.allocation_start) {
            return Err(BlixardError::InvalidConfiguration {
                message: format!(
                    "Allocation start {} is not within subnet {}",
                    self.allocation_start, self.subnet
                ),
            });
        }

        if !self.subnet.contains(&self.allocation_end) {
            return Err(BlixardError::InvalidConfiguration {
                message: format!(
                    "Allocation end {} is not within subnet {}",
                    self.allocation_end, self.subnet
                ),
            });
        }

        // Check allocation range is valid
        match (self.allocation_start, self.allocation_end) {
            (IpAddr::V4(start), IpAddr::V4(end)) => {
                if u32::from(start) > u32::from(end) {
                    return Err(BlixardError::InvalidConfiguration {
                        message: "Allocation start is after allocation end".to_string(),
                    });
                }
            }
            (IpAddr::V6(start), IpAddr::V6(end)) => {
                if u128::from(start) > u128::from(end) {
                    return Err(BlixardError::InvalidConfiguration {
                        message: "Allocation start is after allocation end".to_string(),
                    });
                }
            }
            _ => {
                return Err(BlixardError::InvalidConfiguration {
                    message: "Allocation range addresses must be same IP version".to_string(),
                });
            }
        }

        // Check reserved IPs are within subnet
        for ip in &self.reserved_ips {
            if !self.subnet.contains(ip) {
                return Err(BlixardError::InvalidConfiguration {
                    message: format!("Reserved IP {} is not within subnet {}", ip, self.subnet),
                });
            }
        }

        // Validate VLAN ID if specified
        if let Some(vlan) = self.vlan_id {
            if vlan == 0 || vlan > 4094 {
                return Err(BlixardError::InvalidConfiguration {
                    message: format!("Invalid VLAN ID {}: must be 1-4094", vlan),
                });
            }
        }

        Ok(())
    }

    /// Calculate the total number of allocatable IPs in this pool
    pub fn total_capacity(&self) -> u64 {
        match (self.allocation_start, self.allocation_end) {
            (IpAddr::V4(start), IpAddr::V4(end)) => {
                let start_u32 = u32::from(start);
                let end_u32 = u32::from(end);
                (end_u32 - start_u32 + 1) as u64 - self.reserved_ips.len() as u64
            }
            (IpAddr::V6(start), IpAddr::V6(end)) => {
                let start_u128 = u128::from(start);
                let end_u128 = u128::from(end);
                // For IPv6, we might have huge ranges, so cap at u64::MAX
                ((end_u128 - start_u128 + 1).min(u64::MAX as u128)) as u64
                    - self.reserved_ips.len() as u64
            }
            _ => 0,
        }
    }
}

/// Represents an allocated IP address
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpAllocation {
    /// The allocated IP address
    pub ip: IpAddr,
    /// Pool this IP was allocated from
    pub pool_id: IpPoolId,
    /// VM this IP is allocated to
    pub vm_id: VmId,
    /// MAC address associated with this IP
    pub mac_address: String,
    /// Timestamp when allocated (Unix timestamp)
    pub allocated_at: i64,
    /// Optional lease expiration time
    pub expires_at: Option<i64>,
}

/// Commands for IP pool operations through Raft
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpPoolCommand {
    /// Create a new IP pool
    CreatePool(IpPoolConfig),
    /// Update an existing pool configuration
    UpdatePool(IpPoolConfig),
    /// Delete an IP pool (fails if IPs are allocated)
    DeletePool(IpPoolId),
    /// Enable or disable a pool for new allocations
    SetPoolEnabled { pool_id: IpPoolId, enabled: bool },
    /// Reserve additional IPs in a pool
    ReserveIps {
        pool_id: IpPoolId,
        ips: BTreeSet<IpAddr>,
    },
    /// Release reserved IPs in a pool
    ReleaseReservedIps {
        pool_id: IpPoolId,
        ips: BTreeSet<IpAddr>,
    },
}

/// State of an IP pool including allocation tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpPoolState {
    /// Pool configuration
    pub config: IpPoolConfig,
    /// Currently allocated IPs in this pool
    pub allocated_ips: BTreeSet<IpAddr>,
    /// Allocation count for metrics
    pub total_allocated: u64,
    /// Last allocation timestamp
    pub last_allocation: Option<i64>,
}

/// Enhanced IP pool state with ResourcePool integration
pub struct EnhancedIpPoolState {
    /// Basic pool state (serializable)
    pub state: IpPoolState,
    /// Resource pool for managed allocation
    pub resource_pool: Option<ResourcePool<IpAddress>>,
    /// Shared allocated IPs for factory coordination
    allocated_ips_shared: Arc<std::sync::RwLock<BTreeSet<IpAddr>>>,
}

impl EnhancedIpPoolState {
    /// Create a new enhanced pool state from configuration
    pub async fn new(config: IpPoolConfig) -> BlixardResult<Self> {
        let state = IpPoolState::new(config.clone());
        let allocated_ips_shared = Arc::new(std::sync::RwLock::new(state.allocated_ips.clone()));

        // Create resource pool configuration
        let pool_config = PoolConfig {
            max_size: config.total_capacity() as usize,
            min_size: 0,
            acquire_timeout: std::time::Duration::from_secs(30),
            validate_on_acquire: true,
            reset_on_acquire: false,
        };

        // Create factory and resource pool
        let factory = Arc::new(IpAddressFactory::new(config, allocated_ips_shared.clone()));
        let resource_pool = ResourcePool::new(factory, pool_config);

        // Initialize the resource pool
        resource_pool.initialize().await?;

        Ok(Self {
            state,
            resource_pool: Some(resource_pool),
            allocated_ips_shared,
        })
    }

    /// Create from existing state (for loading from storage)
    pub async fn from_existing_state(state: IpPoolState) -> BlixardResult<Self> {
        let allocated_ips_shared = Arc::new(std::sync::RwLock::new(state.allocated_ips.clone()));

        // Create resource pool configuration
        let pool_config = PoolConfig {
            max_size: state.config.total_capacity() as usize,
            min_size: 0,
            acquire_timeout: std::time::Duration::from_secs(30),
            validate_on_acquire: true,
            reset_on_acquire: false,
        };

        // Create factory and resource pool
        let factory = Arc::new(IpAddressFactory::new(
            state.config.clone(),
            allocated_ips_shared.clone(),
        ));
        let resource_pool = ResourcePool::new(factory, pool_config);

        // Initialize the resource pool
        resource_pool.initialize().await?;

        Ok(Self {
            state,
            resource_pool: Some(resource_pool),
            allocated_ips_shared,
        })
    }

    /// Allocate an IP address using ResourcePool
    pub async fn allocate_ip(&mut self) -> BlixardResult<IpAddress> {
        let resource_pool = self
            .resource_pool
            .as_ref()
            .ok_or_else(|| BlixardError::Internal {
                message: "Resource pool not initialized".to_string(),
            })?;

        let pooled_ip = resource_pool.acquire().await?;
        let ip_address = pooled_ip.take()?; // Take ownership, removing from pool

        // Update state tracking
        self.state.allocated_ips.insert(ip_address.ip);
        self.state.total_allocated += 1;
        self.state.last_allocation = Some(time_since_epoch_safe() as i64);

        Ok(ip_address)
    }

    /// Release an IP address back to the pool
    pub async fn release_ip(&mut self, ip: IpAddr) -> BlixardResult<()> {
        // Remove from tracking
        if self.state.allocated_ips.remove(&ip) {
            self.state.total_allocated = self.state.total_allocated.saturating_sub(1);
        }

        // Remove from shared state
        {
            let mut allocated = acquire_write_lock!(
                self.allocated_ips_shared.write(),
                "remove allocated IP from shared state"
            );
            allocated.remove(&ip);
        }

        Ok(())
    }

    /// Check if the pool has available capacity using ResourcePool
    pub async fn has_capacity(&self) -> bool {
        if !self.state.config.enabled {
            return false;
        }

        if let Some(resource_pool) = &self.resource_pool {
            let stats = resource_pool.stats().await;
            stats.in_use < stats.max_size
        } else {
            // Fallback to original logic
            self.state.has_capacity()
        }
    }

    /// Get pool utilization using ResourcePool stats
    pub async fn utilization_percent(&self) -> f64 {
        if let Some(resource_pool) = &self.resource_pool {
            let stats = resource_pool.stats().await;
            if stats.max_size == 0 {
                return 100.0;
            }
            (stats.in_use as f64 / stats.max_size as f64) * 100.0
        } else {
            // Fallback to original calculation
            self.state.utilization_percent()
        }
    }

    /// Get current resource pool statistics
    pub async fn get_pool_stats(&self) -> Option<crate::patterns::resource_pool::PoolStats> {
        if let Some(resource_pool) = &self.resource_pool {
            Some(resource_pool.stats().await)
        } else {
            None
        }
    }

    /// Sync state from ResourcePool to maintain consistency
    pub async fn sync_state(&mut self) {
        if let Some(resource_pool) = &self.resource_pool {
            let _stats = resource_pool.stats().await;

            // Update allocated IPs from shared state
            match self.allocated_ips_shared.read() {
                Ok(allocated) => {
                    self.state.allocated_ips = allocated.clone();
                    self.state.total_allocated = allocated.len() as u64;
                }
                Err(e) => {
                    tracing::warn!("Failed to acquire read lock for allocated IPs sync: {}", e);
                }
            }
        }
    }
}

impl IpPoolState {
    /// Create a new pool state from configuration
    pub fn new(config: IpPoolConfig) -> Self {
        Self {
            config,
            allocated_ips: BTreeSet::new(),
            total_allocated: 0,
            last_allocation: None,
        }
    }

    /// Check if an IP is available for allocation
    pub fn is_available(&self, ip: &IpAddr) -> bool {
        !self.allocated_ips.contains(ip)
            && !self.config.reserved_ips.contains(ip)
            && self.config.subnet.contains(ip)
            && *ip != self.config.gateway
    }

    /// Find the next available IP in the pool
    pub fn next_available_ip(&self) -> Option<IpAddr> {
        match (self.config.allocation_start, self.config.allocation_end) {
            (IpAddr::V4(start), IpAddr::V4(end)) => {
                let start_u32 = u32::from(start);
                let end_u32 = u32::from(end);

                for i in start_u32..=end_u32 {
                    let ip = IpAddr::V4(Ipv4Addr::from(i));
                    if self.is_available(&ip) {
                        return Some(ip);
                    }
                }
                None
            }
            (IpAddr::V6(start), IpAddr::V6(end)) => {
                let start_u128 = u128::from(start);
                let end_u128 = u128::from(end);

                // For IPv6, limit iteration to prevent performance issues
                let max_iterations = 10000;
                for i in 0..max_iterations.min(end_u128 - start_u128 + 1) {
                    let ip = IpAddr::V6(Ipv6Addr::from(start_u128 + i));
                    if self.is_available(&ip) {
                        return Some(ip);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Get pool utilization as a percentage
    pub fn utilization_percent(&self) -> f64 {
        let total = self.config.total_capacity();
        if total == 0 {
            return 100.0;
        }
        (self.allocated_ips.len() as f64 / total as f64) * 100.0
    }

    /// Check if the pool has available capacity
    pub fn has_capacity(&self) -> bool {
        self.config.enabled && self.next_available_ip().is_some()
    }
}

/// Selection strategy for choosing an IP pool
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum IpPoolSelectionStrategy {
    /// Select pool with most available IPs
    LeastUtilized,
    /// Select pool with least available IPs (pack densely)
    MostUtilized,
    /// Round-robin across pools
    RoundRobin,
    /// Prefer pools matching topology hint
    TopologyAware,
    /// Random selection
    Random,
}

/// Request for IP allocation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpAllocationRequest {
    /// VM requesting the IP
    pub vm_id: VmId,
    /// Preferred pool ID (optional)
    pub preferred_pool: Option<IpPoolId>,
    /// Required tags for pool selection
    pub required_tags: HashMap<String, String>,
    /// Topology hint for pool selection
    pub topology_hint: Option<String>,
    /// Selection strategy if no pool specified
    pub selection_strategy: IpPoolSelectionStrategy,
    /// MAC address to associate with IP
    pub mac_address: String,
}

/// Result of an IP allocation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpAllocationResult {
    /// The allocated IP information
    pub allocation: IpAllocation,
    /// Network configuration for the VM
    pub network_config: NetworkConfig,
}

/// Network configuration provided with IP allocation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Allocated IP address
    pub ip_address: IpAddr,
    /// Network subnet mask
    pub subnet: IpNet,
    /// Gateway IP
    pub gateway: IpAddr,
    /// DNS servers
    pub dns_servers: Vec<IpAddr>,
    /// Optional VLAN ID
    pub vlan_id: Option<u16>,
    /// MAC address
    pub mac_address: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn create_test_pool() -> IpPoolConfig {
        IpPoolConfig {
            id: IpPoolId(1),
            name: "test-pool".to_string(),
            subnet: IpNet::from_str("10.0.0.0/24").expect("Test subnet should be valid"),
            vlan_id: Some(100),
            gateway: IpAddr::from_str("10.0.0.1").expect("Test gateway should be valid"),
            dns_servers: vec![
                IpAddr::from_str("8.8.8.8").expect("Test DNS server should be valid"),
                IpAddr::from_str("8.8.4.4").expect("Test DNS server should be valid"),
            ],
            allocation_start: IpAddr::from_str("10.0.0.10")
                .expect("Test allocation start should be valid"),
            allocation_end: IpAddr::from_str("10.0.0.250")
                .expect("Test allocation end should be valid"),
            topology_hint: Some("zone-1".to_string()),
            reserved_ips: BTreeSet::new(),
            enabled: true,
            tags: HashMap::new(),
        }
    }

    #[test]
    fn test_pool_validation() {
        let mut pool = create_test_pool();
        assert!(pool.validate().is_ok());

        // Test invalid gateway
        pool.gateway = IpAddr::from_str("192.168.1.1").expect("Test IP should be valid");
        assert!(pool.validate().is_err());
        pool.gateway = IpAddr::from_str("10.0.0.1").expect("Test IP should be valid");

        // Test invalid allocation range
        pool.allocation_start = IpAddr::from_str("10.0.0.250").expect("Test IP should be valid");
        pool.allocation_end = IpAddr::from_str("10.0.0.10").expect("Test IP should be valid");
        assert!(pool.validate().is_err());

        // Test invalid VLAN
        pool.allocation_start = IpAddr::from_str("10.0.0.10").expect("Test IP should be valid");
        pool.vlan_id = Some(5000);
        assert!(pool.validate().is_err());
    }

    #[test]
    fn test_pool_capacity() {
        let pool = create_test_pool();
        // 241 IPs (10-250 inclusive)
        assert_eq!(pool.total_capacity(), 241);

        let mut pool_with_reserved = pool.clone();
        pool_with_reserved
            .reserved_ips
            .insert(IpAddr::from_str("10.0.0.100").expect("Test IP should be valid"));
        pool_with_reserved
            .reserved_ips
            .insert(IpAddr::from_str("10.0.0.101").expect("Test IP should be valid"));
        assert_eq!(pool_with_reserved.total_capacity(), 239);
    }

    #[test]
    fn test_ip_allocation() {
        let config = create_test_pool();
        let mut state = IpPoolState::new(config);

        // First available should be 10.0.0.10
        assert_eq!(
            state.next_available_ip(),
            Some(IpAddr::from_str("10.0.0.10").expect("Test IP should be valid"))
        );

        // Allocate some IPs
        state
            .allocated_ips
            .insert(IpAddr::from_str("10.0.0.10").expect("Test IP should be valid"));
        state
            .allocated_ips
            .insert(IpAddr::from_str("10.0.0.11").expect("Test IP should be valid"));

        // Next available should be 10.0.0.12
        assert_eq!(
            state.next_available_ip(),
            Some(IpAddr::from_str("10.0.0.12").expect("Test IP should be valid"))
        );

        // Test utilization
        assert!(state.utilization_percent() < 1.0);
        assert!(state.has_capacity());
    }

    #[test]
    fn test_ipv6_pool() {
        let config = IpPoolConfig {
            id: IpPoolId(2),
            name: "ipv6-pool".to_string(),
            subnet: IpNet::from_str("2001:db8::/64").expect("Test IPv6 subnet should be valid"),
            vlan_id: None,
            gateway: IpAddr::from_str("2001:db8::1").expect("Test IPv6 gateway should be valid"),
            dns_servers: vec![
                IpAddr::from_str("2001:4860:4860::8888").expect("Test IPv6 DNS should be valid")
            ],
            allocation_start: IpAddr::from_str("2001:db8::1000")
                .expect("Test IPv6 start should be valid"),
            allocation_end: IpAddr::from_str("2001:db8::2000")
                .expect("Test IPv6 end should be valid"),
            topology_hint: None,
            reserved_ips: BTreeSet::new(),
            enabled: true,
            tags: HashMap::new(),
        };

        assert!(config.validate().is_ok());
        let state = IpPoolState::new(config);
        assert!(state.next_available_ip().is_some());
    }
}
