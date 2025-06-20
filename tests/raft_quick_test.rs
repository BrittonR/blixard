#![cfg(feature = "test-helpers")]

use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;

use blixard::{
    node::Node,
    types::NodeConfig,
    test_helpers::PortAllocator,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_raft_starts_and_becomes_leader() {
    let temp_dir = TempDir::new().unwrap();
    let port = PortAllocator::next_port();
    let config = NodeConfig {
        id: 99,
        data_dir: temp_dir.path().to_str().unwrap().to_string(),
        bind_addr: format!("127.0.0.1:{}", port).parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
    };
    
    let mut node = Node::new(config);
    
    // Initialize should work
    node.initialize().await.unwrap();
    
    // Bootstrap as single-node cluster should work
    node.join_cluster(None).await.unwrap();
    
    // Verify node becomes leader in single-node cluster
    let mut attempts = 0;
    let max_attempts = 100; // 5 seconds max
    
    loop {
        if let Ok((leader_id, nodes, term)) = node.get_cluster_status().await {
            if leader_id == 99 {
                // Success - node became leader
                assert_eq!(leader_id, 99, "Node should be leader in single-node cluster");
                assert_eq!(nodes.len(), 1, "Should have exactly 1 node");
                assert!(term > 0, "Term should be greater than 0 after election");
                assert!(nodes.contains(&99), "Node list should contain this node");
                break;
            }
        }
        
        attempts += 1;
        if attempts >= max_attempts {
            panic!("Node failed to become leader within 5 seconds");
        }
        
        sleep(Duration::from_millis(50)).await;
    }
    
    // Verify node is functioning properly by checking it remains running
    assert!(node.is_running().await, "Node should remain running after becoming leader");
    
    // Stop should work and complete quickly
    node.stop().await.unwrap();
}