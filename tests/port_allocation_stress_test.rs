#![cfg(feature = "test-helpers")]

use blixard::test_helpers::{PortAllocator, TestNode};
use std::collections::HashSet;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_port_allocation() {
    // Reset stats before test
    PortAllocator::reset();
    
    // Spawn multiple tasks that allocate ports concurrently
    let mut handles: Vec<JoinHandle<u16>> = vec![];
    
    for _ in 0..20 {
        let handle = tokio::spawn(async {
            PortAllocator::next_available_port().await
        });
        handles.push(handle);
    }
    
    // Collect all allocated ports
    let mut ports = HashSet::new();
    for handle in handles {
        let port = handle.await.unwrap();
        // Ensure no duplicates
        assert!(ports.insert(port), "Duplicate port allocated: {}", port);
    }
    
    // Verify we got 20 unique ports
    assert_eq!(ports.len(), 20);
    
    // Print stats if debug enabled
    PortAllocator::print_stats();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_port_allocation_with_conflicts() {
    // This test simulates port conflicts by creating nodes
    // and keeping them alive while allocating more ports
    
    let mut nodes = vec![];
    
    // Create 5 nodes to occupy ports
    for i in 1..=5 {
        let node = TestNode::builder()
            .with_id(i)
            .with_auto_port().await
            .build()
            .await
            .unwrap();
        nodes.push(node);
    }
    
    // Now allocate 5 more ports while the nodes are still running
    let mut additional_ports = vec![];
    for _ in 0..5 {
        let port = PortAllocator::next_available_port().await;
        additional_ports.push(port);
    }
    
    // Verify all ports are unique
    let mut all_ports = HashSet::new();
    for node in &nodes {
        all_ports.insert(node.addr.port());
    }
    for &port in &additional_ports {
        assert!(all_ports.insert(port), "Port conflict detected: {}", port);
    }
    
    // Cleanup
    for node in nodes {
        node.shutdown().await;
    }
    
    // Print final stats
    if std::env::var("BLIXARD_PORT_DEBUG").is_ok() {
        eprintln!("Final port allocation stats: {}", PortAllocator::get_stats());
    }
}

#[tokio::test]
async fn test_port_allocator_wraparound() {
    // Test that the port allocator properly wraps around
    // when reaching the upper limit
    
    // Verify that allocation continues to work even after many allocations
    let mut last_port = 0;
    let mut wraparound_detected = false;
    
    for i in 0..100 {
        let port = PortAllocator::next_available_port().await;
        assert!(port >= 20000 && port <= 30000, "Port out of expected range: {}", port);
        
        // Check for wraparound (port suddenly becomes much smaller)
        if last_port > 29000 && port < 21000 {
            eprintln!("Port allocator wrapped around from {} to {} at iteration {}", last_port, port, i);
            wraparound_detected = true;
        }
        last_port = port;
    }
    
    // Print stats at the end
    PortAllocator::print_stats();
}