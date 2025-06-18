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
async fn test_raft_starts_and_stops() {
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
    
    // Give it minimal time
    sleep(Duration::from_millis(50)).await;
    
    // Stop should work and complete quickly
    node.stop().await.unwrap();
    
    // Test passes if we get here
}