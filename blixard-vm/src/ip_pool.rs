use std::collections::HashSet;
use std::net::Ipv4Addr;
use tracing::{info, warn};

use blixard_core::error::{BlixardError, BlixardResult};

/// Default network configuration constants
pub mod constants {
    use std::net::Ipv4Addr;
    
    /// Default subnet for VM networking
    pub const DEFAULT_SUBNET: &str = "10.0.0.0/24";
    
    /// Default gateway IP address
    pub const DEFAULT_GATEWAY: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 1);
    
    /// Default network base address
    pub const DEFAULT_NETWORK_BASE: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 0);
    
    /// Default start of VM IP allocation range
    pub const DEFAULT_ALLOCATION_START: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 10);
    
    /// Default subnet prefix length
    pub const DEFAULT_PREFIX_LEN: u8 = 24;
    
    /// Maximum number of IPs in a /24 subnet (excluding network and broadcast)
    pub const MAX_IPS_PER_SUBNET_24: usize = 254;
    
    /// MAC address prefix for locally administered addresses
    pub const MAC_PREFIX: &str = "02:00:00";
}

/// IP address pool manager for routed VM networking
///
/// Manages IP address allocation within a subnet for VM instances.
/// Default subnet is 10.0.0.0/24 with gateway at 10.0.0.1.
/// VM IPs are allocated from 10.0.0.10 onwards.
#[derive(Debug, Clone)]
pub struct IpAddressPool {
    /// The subnet CIDR (e.g., "10.0.0.0/24")
    subnet: String,
    /// Gateway IP address (e.g., "10.0.0.1")
    gateway: Ipv4Addr,
    /// Network base address (e.g., 10.0.0.0)
    network_base: Ipv4Addr,
    /// Subnet mask bits (e.g., 24 for /24)
    prefix_len: u8,
    /// Start of VM allocation range (e.g., 10.0.0.10)
    allocation_start: Ipv4Addr,
    /// Currently allocated IP addresses
    allocated_ips: HashSet<Ipv4Addr>,
    /// Next IP to try for allocation
    next_ip: Ipv4Addr,
}

impl Default for IpAddressPool {
    fn default() -> Self {
        Self::new()
    }
}

impl IpAddressPool {
    /// Create a new IP address pool with default subnet 10.0.0.0/24
    pub fn new() -> Self {
        Self::with_config(
            constants::DEFAULT_SUBNET,
            constants::DEFAULT_GATEWAY,
            constants::DEFAULT_NETWORK_BASE,
            constants::DEFAULT_PREFIX_LEN,
            constants::DEFAULT_ALLOCATION_START,
        )
    }
    
    /// Create a new IP address pool with custom configuration
    pub fn with_config(
        subnet: &str,
        gateway: Ipv4Addr,
        network_base: Ipv4Addr,
        prefix_len: u8,
        allocation_start: Ipv4Addr,
    ) -> Self {
        Self {
            subnet: subnet.to_string(),
            gateway,
            network_base,
            prefix_len,
            allocation_start,
            allocated_ips: HashSet::new(),
            next_ip: allocation_start,
        }
    }

    /// Allocate a new IP address from the pool
    pub fn allocate_ip(&mut self) -> BlixardResult<Ipv4Addr> {
        let max_attempts = self.calculate_max_ips();
        let mut attempts = 0;

        while attempts < max_attempts {
            let candidate_ip = self.next_ip;

            // Check if IP is available
            if self.is_ip_available(candidate_ip) {
                self.allocated_ips.insert(candidate_ip);
                self.advance_next_ip();
                info!("Allocated IP address: {}", candidate_ip);
                return Ok(candidate_ip);
            }

            self.advance_next_ip();
            attempts += 1;
        }

        Err(BlixardError::VmOperationFailed {
            operation: "allocate_ip".to_string(),
            details: format!("No available IP addresses in subnet {}", self.subnet),
        })
    }

    /// Release an IP address back to the pool
    pub fn release_ip(&mut self, ip: Ipv4Addr) {
        if self.allocated_ips.remove(&ip) {
            info!("Released IP address: {}", ip);
        } else {
            warn!("Attempted to release unallocated IP: {}", ip);
        }
    }

    /// Generate a unique MAC address for a VM
    pub fn generate_mac_address(&self, vm_name: &str) -> String {
        // Generate a deterministic MAC address based on VM name
        // Use a hash of the VM name to ensure uniqueness
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        vm_name.hash(&mut hasher);
        let hash = hasher.finish();

        // Use hash to generate last 3 octets of MAC
        // First 3 octets are 02:00:00 (locally administered)
        format!(
            "{}:{:02x}:{:02x}:{:02x}",
            constants::MAC_PREFIX,
            (hash >> 16) & 0xff,
            (hash >> 8) & 0xff,
            hash & 0xff
        )
    }

    /// Get the gateway IP address
    pub fn gateway(&self) -> Ipv4Addr {
        self.gateway
    }

    /// Get the subnet CIDR string
    pub fn subnet(&self) -> &str {
        &self.subnet
    }
    
    /// Get the number of allocated IPs
    pub fn allocated_count(&self) -> usize {
        self.allocated_ips.len()
    }
    
    /// Get the total capacity of the pool
    pub fn capacity(&self) -> usize {
        self.calculate_max_ips()
    }
    
    /// Check if a specific IP is allocated
    pub fn is_allocated(&self, ip: Ipv4Addr) -> bool {
        self.allocated_ips.contains(&ip)
    }

    /// Check if an IP address is available for allocation
    fn is_ip_available(&self, ip: Ipv4Addr) -> bool {
        !self.allocated_ips.contains(&ip) 
            && ip != self.gateway 
            && self.is_ip_in_subnet(ip)
    }

    /// Check if an IP address is within the subnet range
    fn is_ip_in_subnet(&self, ip: Ipv4Addr) -> bool {
        let ip_num = u32::from(ip);
        let network_num = u32::from(self.network_base);
        let mask = !0u32 << (32 - self.prefix_len);

        (ip_num & mask) == (network_num & mask) && ip_num >= u32::from(self.allocation_start)
    }

    /// Advance to the next IP address candidate
    fn advance_next_ip(&mut self) {
        let next_num = u32::from(self.next_ip) + 1;
        self.next_ip = Ipv4Addr::from(next_num);

        // Wrap around if we go past the allocation range
        if !self.is_ip_in_subnet(self.next_ip) {
            self.next_ip = self.allocation_start;
        }
    }
    
    /// Calculate maximum number of IPs in the subnet
    fn calculate_max_ips(&self) -> usize {
        match self.prefix_len {
            24 => constants::MAX_IPS_PER_SUBNET_24,
            prefix => {
                // Calculate for other subnet sizes
                let total_ips = 1usize << (32 - prefix);
                // Subtract network and broadcast addresses
                total_ips.saturating_sub(2)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ip_allocation() {
        let mut pool = IpAddressPool::new();
        
        // First allocation should be 10.0.0.10
        let ip1 = pool.allocate_ip().unwrap();
        assert_eq!(ip1, Ipv4Addr::new(10, 0, 0, 10));
        
        // Second allocation should be 10.0.0.11
        let ip2 = pool.allocate_ip().unwrap();
        assert_eq!(ip2, Ipv4Addr::new(10, 0, 0, 11));
        
        // Check allocation tracking
        assert!(pool.is_allocated(ip1));
        assert!(pool.is_allocated(ip2));
        assert_eq!(pool.allocated_count(), 2);
    }

    #[test]
    fn test_ip_release() {
        let mut pool = IpAddressPool::new();
        
        let ip = pool.allocate_ip().unwrap();
        assert!(pool.is_allocated(ip));
        
        pool.release_ip(ip);
        assert!(!pool.is_allocated(ip));
        assert_eq!(pool.allocated_count(), 0);
    }

    #[test]
    fn test_mac_address_generation() {
        let pool = IpAddressPool::new();
        
        let mac1 = pool.generate_mac_address("vm1");
        let mac2 = pool.generate_mac_address("vm2");
        let mac1_dup = pool.generate_mac_address("vm1");
        
        // Same VM name should produce same MAC
        assert_eq!(mac1, mac1_dup);
        
        // Different VM names should produce different MACs
        assert_ne!(mac1, mac2);
        
        // All MACs should start with the local prefix
        assert!(mac1.starts_with("02:00:00:"));
        assert!(mac2.starts_with("02:00:00:"));
    }

    #[test]
    fn test_subnet_validation() {
        let pool = IpAddressPool::new();
        
        // Gateway should not be available for allocation
        assert!(!pool.is_ip_available(Ipv4Addr::new(10, 0, 0, 1)));
        
        // IPs before allocation start should not be available
        assert!(!pool.is_ip_available(Ipv4Addr::new(10, 0, 0, 5)));
        
        // IPs in valid range should be available
        assert!(pool.is_ip_available(Ipv4Addr::new(10, 0, 0, 50)));
        
        // IPs outside subnet should not be available
        assert!(!pool.is_ip_available(Ipv4Addr::new(10, 0, 1, 10)));
    }
}