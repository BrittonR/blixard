#![cfg(feature = "test-helpers")]

//! Tests to verify proper resource cleanup and test isolation

use std::time::Duration;
use tokio::time::sleep;
use blixard::test_helpers::{TestNode, TestCluster};

/// Test that TestNode properly cleans up all resources
#[tokio::test]
async fn test_node_cleanup_releases_resources() {
    // Create and shutdown multiple nodes
    let mut ports = Vec::new();
    
    for i in 1..=3 {
        let node = TestNode::builder()
            .with_id(i)
            .with_auto_port()
            .build()
            .await
            .expect("Failed to create node");
        
        // Record the port used
        ports.push(node.addr.port());
        
        // Verify node is running
        assert!(node.shared_state.is_running().await);
        
        // Shutdown the node
        node.shutdown().await;
        
        // Give OS time to release resources
        sleep(Duration::from_millis(100)).await;
    }
    
    // Verify we got different ports for each node
    assert_eq!(ports.len(), 3);
    let unique_ports: std::collections::HashSet<_> = ports.iter().collect();
    assert_eq!(unique_ports.len(), 3, "Each node should have gotten a unique port");
}

/// Test that background tasks are properly terminated
#[tokio::test]
async fn test_background_tasks_cleanup() {
    // Get initial task count (approximate)
    let initial_tasks = tokio::runtime::Handle::current().metrics().num_alive_tasks();
    
    // Create a node with peer connector
    let node = TestNode::builder()
        .with_id(1)
        .with_auto_port()
        .build()
        .await
        .expect("Failed to create node");
    
    // Wait for background tasks to start
    sleep(Duration::from_millis(500)).await;
    
    // Get task count with node running
    let running_tasks = tokio::runtime::Handle::current().metrics().num_alive_tasks();
    assert!(running_tasks > initial_tasks, "Should have spawned background tasks");
    
    // Shutdown the node
    node.shutdown().await;
    
    // Wait for tasks to terminate
    sleep(Duration::from_millis(500)).await;
    
    // Verify tasks were cleaned up (allow some tolerance for test framework tasks)
    let final_tasks = tokio::runtime::Handle::current().metrics().num_alive_tasks();
    assert!(final_tasks <= initial_tasks + 2, 
        "Background tasks should be terminated. Initial: {}, Final: {}", 
        initial_tasks, final_tasks);
}

/// Test that multiple nodes can use the same ports after cleanup
#[tokio::test]
async fn test_port_reuse_after_cleanup() {
    // Use a specific high port to avoid conflicts
    let test_port = 50000;
    
    // First node using the port
    {
        let node1 = TestNode::builder()
            .with_id(1)
            .with_port(test_port)
            .build()
            .await
            .expect("Failed to create first node");
        
        // Verify it's running
        assert!(node1.shared_state.is_running().await);
        
        // Shutdown
        node1.shutdown().await;
        
        // Give OS time to release the port
        sleep(Duration::from_millis(200)).await;
    }
    
    // Second node should be able to use the same port
    {
        let node2 = TestNode::builder()
            .with_id(2)
            .with_port(test_port)
            .build()
            .await
            .expect("Failed to create second node - port should be available");
        
        // Verify it's running
        assert!(node2.shared_state.is_running().await);
        
        // Shutdown
        node2.shutdown().await;
    }
}

/// Test that database files are properly closed
#[tokio::test]
async fn test_database_cleanup() {
    use tempfile::TempDir;
    
    // Create a temporary directory for the test
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path().to_path_buf();
    
    // Create first node with specific data directory
    {
        let node1 = TestNode::builder()
            .with_id(1)
            .with_auto_port()
            .with_data_dir(data_dir.to_string_lossy().to_string())
            .build()
            .await
            .expect("Failed to create first node");
        
        // Perform some database operations
        if let Ok(status) = node1.node.get_cluster_status().await {
            println!("Node 1 status: {:?}", status);
        }
        
        // Shutdown
        node1.shutdown().await;
        
        // Give time for file handles to be released
        sleep(Duration::from_millis(100)).await;
    }
    
    // Second node should be able to use the same database directory
    {
        let node2 = TestNode::builder()
            .with_id(2)
            .with_auto_port()
            .with_data_dir(data_dir.to_string_lossy().to_string())
            .build()
            .await
            .expect("Failed to create second node - database should be available");
        
        // Verify it's running
        assert!(node2.shared_state.is_running().await);
        
        // Shutdown
        node2.shutdown().await;
    }
}

/// Test that TestCluster properly cleans up all nodes
#[tokio::test]
async fn test_cluster_cleanup() {
    // Create a 3-node cluster
    let cluster = TestCluster::builder()
        .with_nodes(3)
        .build()
        .await
        .expect("Failed to create cluster");
    
    // Verify all nodes are running
    for node in cluster.nodes().values() {
        assert!(node.shared_state.is_running().await);
    }
    
    // Shutdown the cluster
    cluster.shutdown().await;
    
    // Give time for cleanup
    sleep(Duration::from_millis(200)).await;
    
    // All nodes should be stopped (verified by shutdown method)
    // If we get here without hanging, cleanup was successful
}

/// Test rapid node creation and destruction
#[tokio::test]
async fn test_rapid_node_lifecycle() {
    // Rapidly create and destroy nodes to test for resource leaks
    for i in 1..=10 {
        let node = TestNode::builder()
            .with_id(i)
            .with_auto_port()
            .build()
            .await
            .expect(&format!("Failed to create node {}", i));
        
        // Minimal operations
        let _ = node.shared_state.get_raft_status().await;
        
        // Immediate shutdown
        node.shutdown().await;
        
        // Very short delay
        sleep(Duration::from_millis(10)).await;
    }
    
    // If we complete without errors or hangs, cleanup is working
}

/// Test that peer connector background tasks are terminated
#[tokio::test]
async fn test_peer_connector_task_cleanup() {
    // Create a node - this will start peer connector background tasks
    let node = TestNode::builder()
        .with_id(1)
        .with_auto_port()
        .build()
        .await
        .expect("Failed to create node");
    
    // The peer connector starts background tasks even without peers
    // Wait a bit to ensure tasks are started
    sleep(Duration::from_millis(200)).await;
    
    // Verify node is running
    assert!(node.shared_state.is_running().await);
    
    // Get approximate task count before shutdown
    let metrics = tokio::runtime::Handle::current().metrics();
    let tasks_before_shutdown = metrics.num_alive_tasks();
    
    // Shutdown the node - this should stop all peer connector tasks
    node.shutdown().await;
    
    // Give time for all background tasks to terminate
    sleep(Duration::from_millis(500)).await;
    
    // Verify task count decreased (allow some tolerance)
    let tasks_after_shutdown = metrics.num_alive_tasks();
    assert!(
        tasks_after_shutdown <= tasks_before_shutdown,
        "Task count should decrease after shutdown. Before: {}, After: {}",
        tasks_before_shutdown, tasks_after_shutdown
    );
}