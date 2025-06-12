// Comprehensive Node functionality tests

use std::net::SocketAddr;
use tokio::time::{timeout, Duration};
use tempfile::TempDir;

use blixard::{
    node::Node,
    types::{NodeConfig, VmConfig, VmCommand, VmStatus},
    error::BlixardError,
};

mod common;

async fn create_test_node(id: u64) -> (Node, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let config = NodeConfig {
        id,
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
    };
    
    (Node::new(config), temp_dir)
}

#[tokio::test]
async fn test_node_creation() {
    let (node, _temp_dir) = create_test_node(1).await;
    assert!(!node.is_running());
}

#[tokio::test]
async fn test_node_initialization() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    
    // Initialization should succeed
    let result = node.initialize().await;
    assert!(result.is_ok(), "Node initialization failed: {:?}", result);
}

#[tokio::test]
async fn test_node_lifecycle_with_initialization() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    
    // Initialize first
    node.initialize().await.unwrap();
    
    // Start node
    node.start().await.unwrap();
    assert!(node.is_running());
    
    // Give it a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Stop node
    node.stop().await.unwrap();
    assert!(!node.is_running());
}

#[tokio::test]
async fn test_vm_command_send() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    
    // Initialize node to set up channels
    node.initialize().await.unwrap();
    
    let vm_config = VmConfig {
        name: "test-vm".to_string(),
        config_path: "/tmp/test.nix".to_string(),
        memory: 512,
        vcpus: 1,
    };
    
    let command = VmCommand::Create {
        config: vm_config,
        node_id: 1,
    };
    
    // Should succeed in sending command (even though processing will fail)
    let result = node.send_vm_command(command).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_vm_command_send_without_initialization() {
    let (node, _temp_dir) = create_test_node(1).await;
    
    let vm_config = VmConfig {
        name: "test-vm".to_string(),
        config_path: "/tmp/test.nix".to_string(),
        memory: 512,
        vcpus: 1,
    };
    
    let command = VmCommand::Create {
        config: vm_config,
        node_id: 1,
    };
    
    // Should fail when not initialized
    let result = node.send_vm_command(command).await;
    assert!(matches!(result, Err(BlixardError::Internal { .. })));
}

#[tokio::test]
async fn test_cluster_operations_not_implemented() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    
    // Test join_cluster
    let result = node.join_cluster(None).await;
    assert!(matches!(result, Err(BlixardError::NotImplemented { .. })));
    
    // Test leave_cluster
    let result = node.leave_cluster().await;
    assert!(matches!(result, Err(BlixardError::NotImplemented { .. })));
    
    // Test get_cluster_status
    let result = node.get_cluster_status().await;
    assert!(matches!(result, Err(BlixardError::NotImplemented { .. })));
}

#[tokio::test]
async fn test_node_stop_without_start() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    
    // Should succeed even if never started
    let result = node.stop().await;
    assert!(result.is_ok());
    assert!(!node.is_running());
}

#[tokio::test]
async fn test_node_multiple_starts() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    
    node.initialize().await.unwrap();
    
    // First start
    node.start().await.unwrap();
    assert!(node.is_running());
    
    // Second start should replace the first
    node.start().await.unwrap();
    assert!(node.is_running());
    
    node.stop().await.unwrap();
}

#[tokio::test]
async fn test_node_with_different_configs() {
    // Test with different node IDs
    let (node1, _temp1) = create_test_node(1).await;
    let (node2, _temp2) = create_test_node(999).await;
    
    assert!(!node1.is_running());
    assert!(!node2.is_running());
    
    // Test with tailscale enabled
    let temp_dir = TempDir::new().unwrap();
    let config = NodeConfig {
        id: 1,
        data_dir: temp_dir.path().to_string_lossy().to_string(),
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        join_addr: Some("127.0.0.1:8000".parse().unwrap()),
        use_tailscale: true,
    };
    
    let node3 = Node::new(config);
    assert!(!node3.is_running());
}

#[tokio::test]
async fn test_node_start_timeout() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    
    node.initialize().await.unwrap();
    
    // Start should complete quickly
    let result = timeout(Duration::from_secs(5), node.start()).await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_ok());
    
    node.stop().await.unwrap();
}

#[tokio::test]
async fn test_vm_status_update_command() {
    let (mut node, _temp_dir) = create_test_node(1).await;
    
    node.initialize().await.unwrap();
    
    let command = VmCommand::UpdateStatus {
        name: "test-vm".to_string(),
        status: VmStatus::Running,
    };
    
    // Should succeed
    let result = node.send_vm_command(command).await;
    assert!(result.is_ok());
    
    // Give command time to process
    tokio::time::sleep(Duration::from_millis(100)).await;
}