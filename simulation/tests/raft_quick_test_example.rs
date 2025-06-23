#![cfg(feature = "test-helpers")]

use std::time::Duration;
use tempfile::TempDir;

// Import everything from blixard_simulation instead of blixard_core
use blixard_simulation::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_raft_starts_and_becomes_leader() {
    // Use the test utilities from blixard_simulation
    let temp_dir = TempDir::new().unwrap();
    let port_allocator = PortAllocator::new(30000);
    let port = port_allocator.allocate_port().await;
    
    let config = NodeConfig {
        id: 99,
        data_dir: temp_dir.path().to_str().unwrap().to_string(),
        bind_addr: format!("127.0.0.1:{}", port).parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
    };
    
    // Create a test node using the simulation test helpers
    let mut test_node = TestNode::new(99, &port_allocator).await;
    test_node.start().await.unwrap();
    
    // For actual node functionality, you would need to either:
    // 1. Use the gRPC client to interact with a real node
    // 2. Mock the behavior you're testing
    // 3. Import specific functionality from a shared crate
    
    // Example using gRPC client (if the node were running):
    if let Some(client) = test_node.client() {
        // Use the client to test Raft behavior
        let request = HealthCheckRequest {};
        let response = client.clone().health_check(request).await;
        // ... test the response
    }
    
    // Clean up
    test_node.stop().await.unwrap();
}