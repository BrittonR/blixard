//! Debug test for join cluster
#![cfg(feature = "test-helpers")]

use blixard_core::test_helpers::TestNode;
use blixard_core::metrics_otel;
use std::time::Duration;

#[tokio::test]
async fn test_join_debug() {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("blixard=debug,blixard_core::node=trace")
        .try_init();
    
    // Initialize metrics
    metrics_otel::init_noop();
    
    println!("=== Creating bootstrap node ===");
    let node1 = TestNode::start(1, 9001).await.expect("Failed to create node 1");
    println!("Bootstrap node 1 created at {}", node1.addr);
    
    // Wait a bit
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Check if bootstrap is accessible
    let bootstrap_url = format!("http://127.0.0.1:10001/bootstrap");
    println!("=== Checking bootstrap endpoint: {} ===", bootstrap_url);
    
    match reqwest::get(&bootstrap_url).await {
        Ok(resp) => {
            println!("Bootstrap endpoint responded with status: {}", resp.status());
            if let Ok(text) = resp.text().await {
                println!("Bootstrap response: {}", text);
            }
        }
        Err(e) => {
            println!("Bootstrap endpoint error: {}", e);
        }
    }
    
    println!("=== Creating node 2 with join address ===");
    println!("Join address will be: {}", node1.addr);
    
    // Try to create node2 with timeout
    let create_result = tokio::time::timeout(
        Duration::from_secs(10),
        TestNode::start_with_join(2, 9002, Some(node1.addr))
    ).await;
    
    match create_result {
        Ok(Ok(node2)) => {
            println!("Node 2 created successfully at {}", node2.addr);
        }
        Ok(Err(e)) => {
            println!("Node 2 creation failed: {:?}", e);
        }
        Err(_) => {
            println!("Node 2 creation timed out after 10 seconds");
        }
    }
    
    println!("Test completed");
}