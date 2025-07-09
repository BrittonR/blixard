//! IP Pool Management for distributed VM network allocation
//!
//! This module provides IP address pool management with the following features:
//! - Multiple configurable IP pools with different subnets
//! - Distributed consensus through Raft for allocation consistency
//! - Support for VLAN tagging and topology affinity
//! - Automatic IP release on VM deletion
//! - Comprehensive metrics and monitoring

use std::collections::{BTreeSet, HashMap};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use ipnet::IpNet;
use serde::{Deserialize, Serialize};

use crate::{
    error::{BlixardError, BlixardResult},
    types::VmId,
};

/// Unique identifier for an IP pool
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IpPoolId(pub u64);

impl std::fmt::Display for IpPoolId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pool-{}", self.0)
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
            subnet: IpNet::from_str("10.0.0.0/24").unwrap(),
            vlan_id: Some(100),
            gateway: IpAddr::from_str("10.0.0.1").unwrap(),
            dns_servers: vec![
                IpAddr::from_str("8.8.8.8").unwrap(),
                IpAddr::from_str("8.8.4.4").unwrap(),
            ],
            allocation_start: IpAddr::from_str("10.0.0.10").unwrap(),
            allocation_end: IpAddr::from_str("10.0.0.250").unwrap(),
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
        pool.gateway = IpAddr::from_str("192.168.1.1").unwrap();
        assert!(pool.validate().is_err());
        pool.gateway = IpAddr::from_str("10.0.0.1").unwrap();

        // Test invalid allocation range
        pool.allocation_start = IpAddr::from_str("10.0.0.250").unwrap();
        pool.allocation_end = IpAddr::from_str("10.0.0.10").unwrap();
        assert!(pool.validate().is_err());

        // Test invalid VLAN
        pool.allocation_start = IpAddr::from_str("10.0.0.10").unwrap();
        pool.vlan_id = Some(5000);
        assert!(pool.validate().is_err());
    }

    #[test]
    fn test_pool_capacity() {
        let pool = create_test_pool();
        // 241 IPs (10-250 inclusive)
        assert_eq!(pool.total_capacity(), 241);

        let mut pool_with_reserved = pool.clone();
        pool_with_reserved.reserved_ips.insert(IpAddr::from_str("10.0.0.100").unwrap());
        pool_with_reserved.reserved_ips.insert(IpAddr::from_str("10.0.0.101").unwrap());
        assert_eq!(pool_with_reserved.total_capacity(), 239);
    }

    #[test]
    fn test_ip_allocation() {
        let config = create_test_pool();
        let mut state = IpPoolState::new(config);

        // First available should be 10.0.0.10
        assert_eq!(
            state.next_available_ip(),
            Some(IpAddr::from_str("10.0.0.10").unwrap())
        );

        // Allocate some IPs
        state.allocated_ips.insert(IpAddr::from_str("10.0.0.10").unwrap());
        state.allocated_ips.insert(IpAddr::from_str("10.0.0.11").unwrap());

        // Next available should be 10.0.0.12
        assert_eq!(
            state.next_available_ip(),
            Some(IpAddr::from_str("10.0.0.12").unwrap())
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
            subnet: IpNet::from_str("2001:db8::/64").unwrap(),
            vlan_id: None,
            gateway: IpAddr::from_str("2001:db8::1").unwrap(),
            dns_servers: vec![IpAddr::from_str("2001:4860:4860::8888").unwrap()],
            allocation_start: IpAddr::from_str("2001:db8::1000").unwrap(),
            allocation_end: IpAddr::from_str("2001:db8::2000").unwrap(),
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