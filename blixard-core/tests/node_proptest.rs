// Property-based testing for node functionality
#![cfg(feature = "test-helpers")]

use proptest::prelude::*;
use tokio::time::Duration;
use tempfile::TempDir;
use once_cell::sync::Lazy;
use blixard_core::{
    node::Node,
    types::{NodeConfig, VmConfig, VmCommand, VmStatus},
    test_helpers::timing,
};

mod common;

// Shared runtime to prevent resource exhaustion from creating too many runtimes
static RUNTIME: Lazy<tokio::runtime::Runtime> = Lazy::new(|| {
    tokio::runtime::Runtime::new().unwrap()
});

// Strategy for generating valid node IDs
fn node_id_strategy() -> impl Strategy<Value = u64> {
    1u64..=1000u64
}

// Strategy for generating ports in testing range
fn test_port_strategy() -> impl Strategy<Value = u16> {
    7000u16..=8000u16
}

// Strategy for generating VM names
fn vm_name_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9\\-]{1,15}".prop_filter("Valid VM name", |s| {
        s.len() >= 2 && s.len() <= 16 &&
        s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') &&
        s.chars().next().unwrap().is_ascii_lowercase()
    })
}

// Strategy for generating VM resource configurations
fn vm_resources_strategy() -> impl Strategy<Value = (u32, u32)> {
    (1u32..=8u32, prop::sample::select(&[128, 256, 512, 1024, 2048]))
}

async fn create_test_node_with_config(config: NodeConfig) -> (Node, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let mut node_config = config;
    node_config.data_dir = temp_dir.path().to_string_lossy().to_string();
    (Node::new(node_config), temp_dir)
}

// Property: Node creation should always succeed with valid configs
proptest! {
    #[test]
    fn test_node_creation_properties(
        id in node_id_strategy(),
        _port in test_port_strategy(),
        use_tailscale in any::<bool>()
    ) {
        RUNTIME.block_on(async {
            let config = NodeConfig {
                id,
                data_dir: String::new(), // Will be replaced by create_test_node_with_config
                bind_addr: "127.0.0.1:0".parse().unwrap(),
                join_addr: if use_tailscale { 
                    Some("127.0.0.1:0".parse().unwrap())
                } else { 
                    None 
                },
                use_tailscale,
                vm_backend: "mock".to_string(),
            };
            
            let (node, _temp_dir) = create_test_node_with_config(config.clone()).await;
            
            // Node should be created in stopped state
            prop_assert!(!node.is_running().await);
            
            // Node should reflect the configuration
            // (We can't directly access private fields, but we test behavior)
            Ok(())
        }).unwrap();
    }
}

// Property: Node initialization should always succeed
proptest! {
    #[test]
    fn test_node_initialization_properties(
        id in node_id_strategy(),
        _port in test_port_strategy()
    ) {
        RUNTIME.block_on(async {
            let config = NodeConfig {
                id,
                data_dir: String::new(), // Will be replaced by create_test_node_with_config
                bind_addr: "127.0.0.1:0".parse().unwrap(),
                join_addr: None,
                use_tailscale: false,
            vm_backend: "mock".to_string(),
            };
            
            let (mut node, _temp_dir) = create_test_node_with_config(config).await;
            
            // Initialization should succeed for any valid config
            let result = node.initialize().await;
            prop_assert!(result.is_ok());
            
            // Node should still not be running after initialization
            prop_assert!(!node.is_running().await);
            Ok(())
        }).unwrap();
    }
}

// Property: VM command sending should work with any valid command
proptest! {
    #[test]
    fn test_vm_command_properties(
        node_id in node_id_strategy(),
        port in test_port_strategy(),
        vm_name in vm_name_strategy(),
        (vcpus, memory) in vm_resources_strategy(),
        command_type in 0u8..5u8,
        status in prop::sample::select(&[
            VmStatus::Creating,
            VmStatus::Starting,
            VmStatus::Running,
            VmStatus::Stopping,
            VmStatus::Stopped,
            VmStatus::Failed,
        ])
    ) {
        RUNTIME.block_on(async {
            let temp_config = NodeConfig {
                id: node_id,
                data_dir: String::new(), // Will be replaced by create_test_node_with_config
                bind_addr: format!("127.0.0.1:{}", port).parse().unwrap(),
                join_addr: None,
                use_tailscale: false,
            vm_backend: "mock".to_string(),
            };
            
            let (mut node, _temp_dir) = create_test_node_with_config(temp_config).await;
            node.initialize().await.unwrap();
            
            let vm_config = VmConfig {
                name: vm_name.clone(),
                config_path: "/tmp/test.nix".to_string(),
                vcpus,
                memory,
                tenant_id: "default".to_string(),
                ip_address: None,
            };
            
            let command = match command_type {
                0 => VmCommand::Create { config: vm_config, node_id },
                1 => VmCommand::Start { name: vm_name.clone() },
                2 => VmCommand::Stop { name: vm_name.clone() },
                3 => VmCommand::Delete { name: vm_name.clone() },
                4 => VmCommand::UpdateStatus { name: vm_name.clone(), status },
                _ => unreachable!(),
            };
            
            // Sending any valid command should succeed
            let result = node.send_vm_command(command).await;
            prop_assert!(result.is_ok());
            Ok(())
        }).unwrap();
    }
}

// Property: Node lifecycle should be consistent
proptest! {
    #[test]
    fn test_node_lifecycle_properties(
        id in node_id_strategy(),
        _port in test_port_strategy()
    ) {
        RUNTIME.block_on(async {
            let config = NodeConfig {
                id,
                data_dir: String::new(), // Will be replaced by create_test_node_with_config
                bind_addr: "127.0.0.1:0".parse().unwrap(),
                join_addr: None,
                use_tailscale: false,
            vm_backend: "mock".to_string(),
            };
            
            let (mut node, _temp_dir) = create_test_node_with_config(config).await;
            
            // Initial state
            prop_assert!(!node.is_running().await);
            
            // Initialize
            node.initialize().await.unwrap();
            prop_assert!(!node.is_running().await);
            
            // Start
            node.start().await.unwrap();
            prop_assert!(node.is_running().await);
            
            // Stop
            node.stop().await.unwrap();
            prop_assert!(!node.is_running().await);
            
            // Should be able to start again
            node.start().await.unwrap();
            prop_assert!(node.is_running().await);
            
            // Final stop
            node.stop().await.unwrap();
            prop_assert!(!node.is_running().await);
            Ok(())
        }).unwrap();
    }
}

// Property: Uninitialized nodes should fail cluster operations
proptest! {
    #[test]
    fn test_cluster_operations_uninitialized(
        id in node_id_strategy(),
        _port in test_port_strategy(),
        peer_port in test_port_strategy()
    ) {
        let _ = RUNTIME.block_on(async {
            let config = NodeConfig {
                id,
                data_dir: String::new(), // Will be replaced by create_test_node_with_config
                bind_addr: "127.0.0.1:0".parse().unwrap(),
                join_addr: None,
                use_tailscale: false,
            vm_backend: "mock".to_string(),
            };
            
            let (mut node, _temp_dir) = create_test_node_with_config(config).await;
            
            // Uninitialized node should fail cluster operations
            let join_result = node.join_cluster(
                Some(format!("127.0.0.1:{}", peer_port).parse().unwrap())
            ).await;
            prop_assert!(join_result.is_err(), "Uninitialized node should fail join_cluster");
            
            let leave_result = node.leave_cluster().await;
            prop_assert!(leave_result.is_err(), "Uninitialized node should fail leave_cluster");
            
            let status_result = node.get_cluster_status().await;
            prop_assert!(status_result.is_err(), "Uninitialized node should fail get_cluster_status");
            
            Ok(())
        }).unwrap();
    }
}

// Property: Node should handle multiple starts/stops gracefully
proptest! {
    #[test]
    fn test_node_multiple_starts_stops(
        id in node_id_strategy(),
        _port in test_port_strategy(),
        iterations in 1usize..=5usize
    ) {
        RUNTIME.block_on(async {
            let config = NodeConfig {
                id,
                data_dir: String::new(), // Will be replaced by create_test_node_with_config
                bind_addr: "127.0.0.1:0".parse().unwrap(),
                join_addr: None,
                use_tailscale: false,
            vm_backend: "mock".to_string(),
            };
            
            let (mut node, _temp_dir) = create_test_node_with_config(config).await;
            node.initialize().await.unwrap();
            
            for _ in 0..iterations {
                // Start
                let start_result = node.start().await;
                prop_assert!(start_result.is_ok());
                prop_assert!(node.is_running().await);
                
                // Small delay to ensure start completes
                timing::robust_sleep(Duration::from_millis(10)).await;
                
                // Stop
                let stop_result = node.stop().await;
                prop_assert!(stop_result.is_ok());
                prop_assert!(!node.is_running().await);
            }
            Ok(())
        }).unwrap();
    }
}

// Property: VM command sending should work before and after node start
proptest! {
    #[test]
    fn test_vm_commands_across_node_states(
        id in node_id_strategy(),
        _port in test_port_strategy(),
        vm_name in vm_name_strategy()
    ) {
        RUNTIME.block_on(async {
            let config = NodeConfig {
                id,
                data_dir: String::new(), // Will be replaced by create_test_node_with_config
                bind_addr: "127.0.0.1:0".parse().unwrap(),
                join_addr: None,
                use_tailscale: false,
            vm_backend: "mock".to_string(),
            };
            
            let (mut node, _temp_dir) = create_test_node_with_config(config).await;
            node.initialize().await.unwrap();
            
            let command = VmCommand::UpdateStatus {
                name: vm_name.clone(),
                status: VmStatus::Running,
            };
            
            // Should work when node is stopped
            let result1 = node.send_vm_command(command.clone()).await;
            prop_assert!(result1.is_ok());
            
            // Start node
            node.start().await.unwrap();
            
            // Should still work when node is running
            let result2 = node.send_vm_command(command.clone()).await;
            prop_assert!(result2.is_ok());
            
            // Stop node
            node.stop().await.unwrap();
            
            // VM commands should fail after full shutdown (stop() clears all components)
            let result3 = node.send_vm_command(command).await;
            prop_assert!(result3.is_err());
            Ok(())
        }).unwrap();
    }
}

// Property: Stop should always succeed regardless of node state
proptest! {
    #[test]
    fn test_stop_always_succeeds(
        id in node_id_strategy(),
        _port in test_port_strategy(),
        start_node in any::<bool>()
    ) {
        RUNTIME.block_on(async {
            let config = NodeConfig {
                id,
                data_dir: String::new(), // Will be replaced by create_test_node_with_config
                bind_addr: "127.0.0.1:0".parse().unwrap(),
                join_addr: None,
                use_tailscale: false,
            vm_backend: "mock".to_string(),
            };
            
            let (mut node, _temp_dir) = create_test_node_with_config(config).await;
            
            if start_node {
                node.initialize().await.unwrap();
                node.start().await.unwrap();
            }
            
            // Stop should always succeed
            let result = node.stop().await;
            prop_assert!(result.is_ok());
            prop_assert!(!node.is_running().await);
            
            // Multiple stops should be safe
            let result2 = node.stop().await;
            prop_assert!(result2.is_ok());
            prop_assert!(!node.is_running().await);
            Ok(())
        }).unwrap();
    }
}