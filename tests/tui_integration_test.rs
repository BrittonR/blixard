#![cfg(feature = "test-helpers")]

use blixard::tui::{app::App, vm_client::VmClient};
use blixard_core::test_helpers::{TestCluster, wait_for_condition};
use std::time::Duration;

#[tokio::test]
async fn test_tui_auto_discovery() {
    // Start a test cluster
    let cluster = TestCluster::new(3).await;
    cluster.wait_for_cluster_formation(Duration::from_secs(10)).await.unwrap();
    
    // Create TUI app
    let mut app = App::new().await.unwrap();
    
    // Test auto-discovery
    app.auto_discover_nodes().await.unwrap();
    
    // Verify nodes were discovered
    assert!(!app.nodes.is_empty(), "Should discover nodes");
    assert!(app.vm_client.is_some(), "Should connect to cluster");
    
    // Verify status message indicates discovery
    assert!(app.status_message.as_ref()
        .map(|s| s.contains("Discovery complete"))
        .unwrap_or(false));
}

#[tokio::test]
async fn test_tui_quick_add_node() {
    let cluster = TestCluster::new(1).await;
    cluster.wait_for_cluster_formation(Duration::from_secs(10)).await.unwrap();
    
    let mut app = App::new().await.unwrap();
    
    // Connect to cluster first
    app.vm_client = Some(VmClient::new("127.0.0.1:7001").await.unwrap());
    
    // Get initial node count
    let initial_count = app.nodes.len();
    
    // Quick add a node
    app.quick_add_node().await.unwrap();
    
    // Wait for node to be added
    wait_for_condition(
        || async {
            app.refresh_node_list().await.ok();
            app.nodes.len() > initial_count
        },
        Duration::from_secs(5),
    ).await.unwrap();
    
    assert_eq!(app.nodes.len(), initial_count + 1, "Should add one node");
}

#[tokio::test]
async fn test_tui_batch_node_creation() {
    let cluster = TestCluster::new(1).await;
    cluster.wait_for_cluster_formation(Duration::from_secs(10)).await.unwrap();
    
    let mut app = App::new().await.unwrap();
    app.vm_client = Some(VmClient::new("127.0.0.1:7001").await.unwrap());
    
    // Set batch count
    app.batch_node_count = "3".to_string();
    
    let initial_count = app.nodes.len();
    
    // Batch add nodes
    app.batch_add_nodes().await.unwrap();
    
    // Wait for nodes to be added
    wait_for_condition(
        || async {
            app.refresh_node_list().await.ok();
            app.nodes.len() >= initial_count + 3
        },
        Duration::from_secs(10),
    ).await.unwrap();
    
    assert!(app.nodes.len() >= initial_count + 3, "Should add 3 nodes");
}

#[tokio::test]
async fn test_tui_vm_operations() {
    let cluster = TestCluster::new(1).await;
    cluster.wait_for_cluster_formation(Duration::from_secs(10)).await.unwrap();
    
    let mut app = App::new().await.unwrap();
    app.vm_client = Some(VmClient::new("127.0.0.1:7001").await.unwrap());
    
    // Create a VM
    app.create_vm_form.name = "test-vm".to_string();
    app.create_vm_form.vcpus = "2".to_string();
    app.create_vm_form.memory = "1024".to_string();
    
    app.submit_vm_form().await.unwrap();
    
    // Refresh VM list
    app.refresh_vm_list().await.unwrap();
    
    // Verify VM was created
    assert!(!app.vms.is_empty(), "Should have created VM");
    assert_eq!(app.vms[0].name, "test-vm");
    assert_eq!(app.vms[0].vcpus, 2);
    assert_eq!(app.vms[0].memory, 1024);
}

#[tokio::test]
async fn test_tui_connection_status() {
    let mut app = App::new().await.unwrap();
    
    // Initially not connected
    assert!(app.vm_client.is_none());
    assert!(app.connection_status.contains("Not connected"));
    
    // Start a test node
    let cluster = TestCluster::new(1).await;
    cluster.wait_for_cluster_formation(Duration::from_secs(10)).await.unwrap();
    
    // Try to connect
    app.try_connect().await;
    
    // Should be connected now
    assert!(app.vm_client.is_some());
    assert!(app.connection_status.contains("Connected"));
}

#[tokio::test]
async fn test_tui_cluster_metrics() {
    let cluster = TestCluster::new(3).await;
    cluster.wait_for_cluster_formation(Duration::from_secs(10)).await.unwrap();
    
    let mut app = App::new().await.unwrap();
    app.vm_client = Some(VmClient::new("127.0.0.1:7001").await.unwrap());
    
    // Refresh cluster metrics
    app.refresh_cluster_metrics().await.unwrap();
    
    // Verify metrics
    assert!(app.cluster_metrics.total_nodes >= 3);
    assert!(app.cluster_metrics.healthy_nodes >= 3);
    assert!(app.cluster_metrics.leader_id.is_some());
}

#[tokio::test]
async fn test_tui_resource_usage_graphs() {
    let cluster = TestCluster::new(2).await;
    cluster.wait_for_cluster_formation(Duration::from_secs(10)).await.unwrap();
    
    let mut app = App::new().await.unwrap();
    app.vm_client = Some(VmClient::new("127.0.0.1:7001").await.unwrap());
    
    // Create some VMs to generate resource usage
    app.create_vm_form.name = "test-vm-1".to_string();
    app.create_vm_form.vcpus = "2".to_string();
    app.create_vm_form.memory = "1024".to_string();
    app.submit_vm_form().await.unwrap();
    
    // Refresh all data multiple times to build history
    for _ in 0..5 {
        app.refresh_all_data().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    // Verify resource history is populated
    assert!(!app.cpu_history.is_empty(), "CPU history should be populated");
    assert!(!app.memory_history.is_empty(), "Memory history should be populated");
    assert!(!app.network_history.is_empty(), "Network history should be populated");
    
    // Verify health monitoring
    app.update_cluster_health();
    assert_ne!(app.cluster_health.status, blixard::tui::app::HealthStatus::Unknown);
    
    // Verify per-node health history
    assert!(!app.node_health_history.is_empty(), "Node health history should be populated");
    
    // Check alerts are generated for high resource usage
    app.check_health_alerts();
    // May or may not have alerts depending on simulated resource usage
}

#[tokio::test] 
async fn test_tui_vm_templates() {
    let mut app = App::new().await.unwrap();
    
    // Verify VM templates are initialized
    assert!(!app.vm_templates.is_empty(), "VM templates should be initialized");
    
    // Test template selection
    let template = &app.vm_templates[0];
    assert_eq!(template.name, "Micro Instance");
    assert_eq!(template.vcpus, 1);
    assert_eq!(template.memory, 512);
}