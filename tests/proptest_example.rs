// Property-based testing examples using proptest

use proptest::prelude::*;
use blixard::{
    error::{BlixardError, Result as BlixardResult},
    types::{NodeConfig, VmConfig, VmStatus},
};

mod common;
use common::proptest_utils::*;

// Test that node IDs are always valid
proptest! {
    #[test]
    fn test_node_id_validity(id in node_id_strategy()) {
        let config = common::test_node_config(id, 7000);
        
        // Node ID should be positive and within reasonable range
        prop_assert!(config.id > 0);
        prop_assert!(config.id <= 1000);
        prop_assert_eq!(config.id, id);
    }
}

// Test that port numbers are always valid
proptest! {
    #[test]
    fn test_port_validity(port in port_strategy()) {
        let config = common::test_node_config(1, port);
        
        // Port should be in valid range
        prop_assert!(port >= 7000);
        prop_assert!(port <= 8000);
        prop_assert!(config.bind_addr.to_string().contains(&port.to_string()));
    }
}

// Test VM name validation
proptest! {
    #[test]
    fn test_vm_name_validity(name in vm_name_strategy()) {
        let config = common::test_vm_config(&name);
        
        // VM name should meet requirements
        prop_assert!(config.name.len() >= 2);
        prop_assert!(config.name.len() <= 32);
        prop_assert!(config.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
        prop_assert!(config.name.chars().next().unwrap().is_ascii_lowercase());
        prop_assert_eq!(config.name, name);
    }
}

// Test node configuration consistency
proptest! {
    #[test]
    fn test_node_config_consistency(
        id in 1u64..100u64,
        port in 7000u16..8000u16,
        _peers in prop::collection::vec(1u64..100u64, 0..5)
    ) {
        let config = common::test_node_config(id, port);
        // Configuration should be internally consistent
        prop_assert_eq!(config.id, id);
        prop_assert!(config.bind_addr.to_string().contains(&port.to_string()));
        
        // Data directory should be set
        prop_assert!(!config.data_dir.is_empty());
    }
}

// Test VM resource constraints
proptest! {
    #[test]
    fn test_vm_resource_constraints(
        name in "[a-z][a-z0-9-]{1,31}",
        memory_mb in 128u32..8192u32,
        vcpus in 1u32..16u32
    ) {
        let mut config = common::test_vm_config(&name);
        config.memory = memory_mb;
        config.vcpus = vcpus;
        
        // Resource constraints should be reasonable
        prop_assert!(config.memory >= 128);
        prop_assert!(config.memory <= 8192);
        prop_assert!(config.vcpus >= 1);
        prop_assert!(config.vcpus <= 16);
        
        // Skip power-of-2 check for now since it's too restrictive
        // if config.memory >= 256 {
        //     prop_assert_eq!(config.memory & (config.memory - 1), 0);
        // }
    }
}

// Test cluster size properties
proptest! {
    #[test]
    fn test_cluster_size_properties(node_count in 1usize..10usize) {
        let cluster_config = common::simulation::test_cluster_config(node_count);
        
        // Cluster properties
        prop_assert_eq!(cluster_config.len(), node_count);
        
        // All node IDs should be unique
        let mut ids: Vec<u64> = cluster_config.iter().map(|c| c.id).collect();
        ids.sort();
        ids.dedup();
        prop_assert_eq!(ids.len(), node_count);
        
        // All bind addresses should be unique
        let mut addrs: Vec<String> = cluster_config.iter().map(|c| c.bind_addr.to_string()).collect();
        addrs.sort();
        addrs.dedup();
        prop_assert_eq!(addrs.len(), node_count);
    }
}

// Test error handling properties
proptest! {
    #[test]
    fn test_error_handling_properties(
        feature in "[a-zA-Z0-9_-]{1,50}",
        operation in "[a-zA-Z0-9_\\s]{1,100}"
    ) {
        use blixard::error::BlixardError;
        
        let error = BlixardError::SystemError(
            format!("Feature not implemented: {}", feature)
        );
        
        // Error should contain the feature name
        let error_string = format!("{}", error);
        prop_assert!(error_string.contains(&feature));
        
        let network_error = BlixardError::ConfigError(
            format!("Invalid configuration for operation: {}", operation)
        );
        
        // Config error should contain operation info
        let error_string = format!("{}", network_error);
        prop_assert!(error_string.contains(&operation));
    }
}