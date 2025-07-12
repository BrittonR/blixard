//! Property-based tests for IP pool functionality

use blixard_core::ip_pool::{IpPoolConfig, IpPoolId, IpPoolState};
use proptest::prelude::*;
use std::collections::{BTreeSet, HashMap};
use std::net::{IpAddr, Ipv4Addr};

// Strategy for generating valid IPv4 addresses
fn ipv4_addr_strategy() -> impl Strategy<Value = Ipv4Addr> {
    (0u8..=255, 0u8..=255, 0u8..=255, 0u8..=255).prop_map(|(a, b, c, d)| Ipv4Addr::new(a, b, c, d))
}

// Strategy for generating valid IP addresses (v4 only for simplicity)
fn ip_addr_strategy() -> impl Strategy<Value = IpAddr> {
    ipv4_addr_strategy().prop_map(IpAddr::V4)
}

// Strategy for generating valid subnets
fn subnet_strategy() -> impl Strategy<Value = ipnet::IpNet> {
    (ipv4_addr_strategy(), 8u8..=30).prop_map(|(addr, prefix)| {
        // Mask the address to ensure it's a valid network address
        let mask = !((1u32 << (32 - prefix)) - 1);
        let addr_u32 = u32::from(addr);
        let network_u32 = addr_u32 & mask;
        let network_addr = Ipv4Addr::from(network_u32);
        ipnet::Ipv4Net::new(network_addr, prefix).unwrap().into()
    })
}

// Strategy for generating valid IP pool configurations
fn ip_pool_config_strategy() -> impl Strategy<Value = IpPoolConfig> {
    (
        1u64..1000,
        "[a-z]{3,10}",
        subnet_strategy(),
        prop::option::of(1u16..=4094),
        prop::collection::vec(ip_addr_strategy(), 0..3),
        prop::option::of("[a-z]{3,10}"),
        prop::bool::ANY,
    )
        .prop_flat_map(
            |(id, name, subnet, vlan_id, dns_servers, topology_hint, enabled)| {
                // Generate gateway within the subnet
                let subnet_clone = subnet.clone();
                let gateway_strategy = Just(match subnet {
                    ipnet::IpNet::V4(net) => {
                        let network = net.network();
                        let gateway_u32 = u32::from(network) + 1;
                        IpAddr::V4(Ipv4Addr::from(gateway_u32))
                    }
                    ipnet::IpNet::V6(_) => panic!("IPv6 not supported in this test"),
                });

                // Generate allocation range within the subnet
                let (start_strategy, end_strategy) = match subnet_clone {
                    ipnet::IpNet::V4(net) => {
                        let network_u32 = u32::from(net.network());
                        let broadcast_u32 = u32::from(net.broadcast());
                        let start = network_u32 + 10;
                        let end = broadcast_u32.saturating_sub(10).max(start + 1);

                        (
                            Just(IpAddr::V4(Ipv4Addr::from(start))),
                            Just(IpAddr::V4(Ipv4Addr::from(end))),
                        )
                    }
                    ipnet::IpNet::V6(_) => panic!("IPv6 not supported in this test"),
                };

                (
                    Just(id),
                    Just(name),
                    Just(subnet_clone),
                    Just(vlan_id),
                    gateway_strategy,
                    Just(dns_servers),
                    start_strategy,
                    end_strategy,
                    Just(topology_hint),
                    Just(BTreeSet::new()),
                    Just(enabled),
                    Just(HashMap::new()),
                )
            },
        )
        .prop_map(
            |(
                id,
                name,
                subnet,
                vlan_id,
                gateway,
                dns_servers,
                allocation_start,
                allocation_end,
                topology_hint,
                reserved_ips,
                enabled,
                tags,
            )| {
                IpPoolConfig {
                    id: IpPoolId(id),
                    name,
                    subnet,
                    vlan_id,
                    gateway,
                    dns_servers,
                    allocation_start,
                    allocation_end,
                    topology_hint,
                    reserved_ips,
                    enabled,
                    tags,
                }
            },
        )
}

proptest! {
    #[test]
    fn test_ip_pool_config_validation(config in ip_pool_config_strategy()) {
        // The generated config should always be valid
        prop_assert!(config.validate().is_ok());
    }

    #[test]
    fn test_ip_pool_capacity_calculation(config in ip_pool_config_strategy()) {
        let capacity = config.total_capacity();

        // Capacity should be positive
        prop_assert!(capacity > 0);

        // Capacity should not exceed the theoretical maximum for the subnet
        match config.subnet {
            ipnet::IpNet::V4(net) => {
                let max_hosts = net.hosts().count() as u64;
                prop_assert!(capacity <= max_hosts);
            }
            _ => {}
        }
    }

    #[test]
    fn test_ip_pool_state_allocation(config in ip_pool_config_strategy()) {
        let mut state = IpPoolState::new(config.clone());

        // Initial state should have no allocations
        prop_assert_eq!(state.allocated_ips.len(), 0);
        prop_assert_eq!(state.utilization_percent(), 0.0);
        prop_assert!(state.has_capacity());

        // Allocate IPs until pool is full
        let mut allocated_count = 0;
        while let Some(ip) = state.next_available_ip() {
            prop_assert!(state.is_available(&ip));
            state.allocated_ips.insert(ip);
            allocated_count += 1;

            // Prevent infinite loop in case of bug
            if allocated_count > 1000 {
                break;
            }
        }

        // After full allocation, pool should report no capacity
        if allocated_count == config.total_capacity() {
            prop_assert!(!state.has_capacity());
            prop_assert_eq!(state.utilization_percent(), 100.0);
        }
    }

    #[test]
    fn test_ip_allocation_never_duplicates(config in ip_pool_config_strategy()) {
        let mut state = IpPoolState::new(config.clone());
        let mut allocated_ips = BTreeSet::new();

        // Allocate multiple IPs and ensure no duplicates
        for _ in 0..20.min(config.total_capacity()) {
            if let Some(ip) = state.next_available_ip() {
                // Should not have been allocated before
                prop_assert!(!allocated_ips.contains(&ip));

                allocated_ips.insert(ip);
                state.allocated_ips.insert(ip);
            }
        }
    }

    #[test]
    fn test_reserved_ips_not_allocated(mut config in ip_pool_config_strategy()) {
        // Reserve some IPs
        let mut reserved = BTreeSet::new();
        if let ipnet::IpNet::V4(net) = config.subnet {
            let start_u32 = match config.allocation_start {
                IpAddr::V4(addr) => u32::from(addr),
                _ => panic!("Expected IPv4"),
            };

            // Reserve a few IPs in the allocation range
            for i in 0..3 {
                let ip = IpAddr::V4(Ipv4Addr::from(start_u32 + i));
                if config.subnet.contains(&ip) {
                    reserved.insert(ip);
                }
            }
        }

        config.reserved_ips = reserved.clone();
        let mut state = IpPoolState::new(config);

        // Allocate IPs and ensure reserved ones are never allocated
        let mut allocated = BTreeSet::new();
        for _ in 0..50 {
            if let Some(ip) = state.next_available_ip() {
                prop_assert!(!reserved.contains(&ip));
                allocated.insert(ip);
                state.allocated_ips.insert(ip);
            }
        }
    }

    #[test]
    fn test_gateway_never_allocated(config in ip_pool_config_strategy()) {
        let mut state = IpPoolState::new(config.clone());

        // Allocate many IPs and ensure gateway is never allocated
        for _ in 0..config.total_capacity() {
            if let Some(ip) = state.next_available_ip() {
                prop_assert_ne!(ip, config.gateway);
                state.allocated_ips.insert(ip);
            }
        }
    }
}

#[test]
fn test_specific_edge_cases() {
    // Test /32 subnet (single host)
    let config = IpPoolConfig {
        id: IpPoolId(1),
        name: "single-host".to_string(),
        subnet: "10.0.0.1/32".parse().unwrap(),
        vlan_id: None,
        gateway: "10.0.0.1".parse().unwrap(),
        dns_servers: vec![],
        allocation_start: "10.0.0.1".parse().unwrap(),
        allocation_end: "10.0.0.1".parse().unwrap(),
        topology_hint: None,
        reserved_ips: BTreeSet::new(),
        enabled: true,
        tags: HashMap::new(),
    };

    assert!(config.validate().is_ok());
    let state = IpPoolState::new(config);
    // Gateway takes the only IP, so no allocation possible
    assert_eq!(state.next_available_ip(), None);

    // Test /31 subnet (point-to-point link)
    let config = IpPoolConfig {
        id: IpPoolId(2),
        name: "p2p-link".to_string(),
        subnet: "10.0.0.0/31".parse().unwrap(),
        vlan_id: None,
        gateway: "10.0.0.0".parse().unwrap(),
        dns_servers: vec![],
        allocation_start: "10.0.0.0".parse().unwrap(),
        allocation_end: "10.0.0.1".parse().unwrap(),
        topology_hint: None,
        reserved_ips: BTreeSet::new(),
        enabled: true,
        tags: HashMap::new(),
    };

    assert!(config.validate().is_ok());
    let mut state = IpPoolState::new(config);
    // Should be able to allocate the non-gateway IP
    assert_eq!(state.next_available_ip(), Some("10.0.0.1".parse().unwrap()));
}
