//! Cluster formation and node lifecycle tests

#![cfg(madsim)]

use blixard::node::Node;
use blixard::types::NodeConfig;
use madsim::time::{sleep, Duration, Instant};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, debug};

type SharedNode = Arc<RwLock<Node>>;

/// Helper to create a test node configuration
fn create_node_config(id: u64, port: u16, join_addr: Option<String>) -> NodeConfig {
    NodeConfig {
        id,
        data_dir: format!("/tmp/blixard-test-{}", id),
        bind_addr: format!("127.0.0.1:{}", port).parse().unwrap(),
        join_addr: join_addr.map(|addr| addr.parse().unwrap()),
        use_tailscale: false,
    }
}

/// Spawn a cluster of nodes
async fn spawn_cluster(size: usize) -> Vec<SharedNode> {
    let mut nodes = Vec::new();
    
    for i in 0..size {
        let id = (i + 1) as u64;
        let port = 8080 + i as u16;
        let join_addr = if i > 0 {
            Some(format!("127.0.0.1:{}", 8080))
        } else {
            None
        };
        
        let config = create_node_config(id, port, join_addr);
        let mut node = Node::new(config);
        
        info!("Starting node {} on port {}", id, port);
        node.start().await.expect("Failed to start node");
        
        nodes.push(Arc::new(RwLock::new(node)));
        
        // Give nodes time to discover each other
        sleep(Duration::from_millis(100)).await;
    }
    
    nodes
}

/// Helper to run tests with consistent output format
async fn run_test<F, Fut>(name: &str, test_fn: F) 
where 
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>
{
    println!("\n🧪 Running: {}", name);
    let start = Instant::now();
    
    match test_fn().await {
        Ok(()) => {
            println!("✅ {} passed in {:?}", name, start.elapsed());
        }
        Err(e) => {
            println!("❌ {} failed: {}", name, e);
            panic!("Test failed: {}", e);
        }
    }
}

#[madsim::test]
async fn test_single_node_lifecycle() {
    run_test("single_node_lifecycle", || async {
        let config = create_node_config(1, 8080, None);
        let mut node = Node::new(config);
        
        // Start node
        node.start().await.map_err(|e| format!("Failed to start: {}", e))?;
        if !node.is_running() {
            return Err("Node should be running after start".to_string());
        }
        
        // Let it run
        sleep(Duration::from_millis(500)).await;
        
        // Stop node
        node.stop().await.map_err(|e| format!("Failed to stop: {}", e))?;
        if node.is_running() {
            return Err("Node should not be running after stop".to_string());
        }
        
        Ok(())
    }).await;
}

#[madsim::test]
async fn test_three_node_cluster_formation() {
    run_test("three_node_cluster_formation", || async {
        let nodes = spawn_cluster(3).await;
        
        // Verify all nodes are running
        for (i, node) in nodes.iter().enumerate() {
            let node = node.read().await;
            if !node.is_running() {
                return Err(format!("Node {} is not running", i + 1));
            }
        }
        
        // Let cluster stabilize
        sleep(Duration::from_secs(2)).await;
        
        // TODO: Add cluster membership verification once implemented
        info!("All nodes running successfully");
        
        // Cleanup
        for node in nodes {
            node.write().await.stop().await.ok();
        }
        
        Ok(())
    }).await;
}

#[madsim::test]
async fn test_node_restart() {
    run_test("node_restart", || async {
        let config = create_node_config(1, 8080, None);
        let mut node = Node::new(config);
        
        // Start -> Stop -> Start cycle
        for i in 0..3 {
            info!("Cycle {}: starting node", i + 1);
            node.start().await.map_err(|e| format!("Failed to start: {}", e))?;
            
            sleep(Duration::from_millis(200)).await;
            
            info!("Cycle {}: stopping node", i + 1);
            node.stop().await.map_err(|e| format!("Failed to stop: {}", e))?;
            
            sleep(Duration::from_millis(100)).await;
        }
        
        Ok(())
    }).await;
}

#[madsim::test]
async fn test_concurrent_node_operations() {
    run_test("concurrent_node_operations", || async {
        use tokio::task;
        
        let mut handles = vec![];
        
        // Start 5 nodes concurrently
        for i in 0..5 {
            let handle = task::spawn(async move {
                let config = create_node_config(i + 1, 8080 + i as u16, None);
                let mut node = Node::new(config);
                
                node.start().await.expect("Failed to start node");
                
                // Simulate some work
                sleep(Duration::from_millis(500)).await;
                
                node.stop().await.expect("Failed to stop node");
            });
            
            handles.push(handle);
        }
        
        // Wait for all to complete
        for handle in handles {
            handle.await.map_err(|e| format!("Task failed: {}", e))?;
        }
        
        Ok(())
    }).await;
}

#[madsim::test]
async fn test_large_cluster_formation() {
    run_test("large_cluster_formation", || async {
        let cluster_size = 10;
        let nodes = spawn_cluster(cluster_size).await;
        
        // Verify all nodes started
        if nodes.len() != cluster_size {
            return Err(format!("Expected {} nodes, got {}", cluster_size, nodes.len()));
        }
        
        // Let cluster form
        sleep(Duration::from_secs(5)).await;
        
        // Stop all nodes in reverse order
        for node in nodes.iter().rev() {
            node.write().await.stop().await.ok();
            sleep(Duration::from_millis(50)).await;
        }
        
        Ok(())
    }).await;
}