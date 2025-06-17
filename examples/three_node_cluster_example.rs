//! Example of setting up a 3-node cluster test
//!
//! This example demonstrates the approach for creating 3-node cluster tests,
//! addressing the known reliability issues documented in TEST_RELIABILITY_ISSUES.md

use std::time::Duration;
use blixard::test_helpers::{TestNode, timing};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Three-Node Cluster Setup Example ===\n");
    
    // Method 1: Manual node creation with explicit coordination
    println!("Method 1: Manual Node Creation");
    println!("------------------------------");
    
    // Create bootstrap node first
    let node1 = TestNode::builder()
        .with_id(1)
        .with_auto_port()
        .build()
        .await?;
    
    println!("✓ Created bootstrap node 1 at {}", node1.addr);
    
    // Wait for bootstrap node to elect itself as leader
    timing::robust_sleep(Duration::from_secs(2)).await;
    
    // Create node 2 that will join node 1
    let node2 = TestNode::builder()
        .with_id(2)
        .with_auto_port()
        .with_join_addr(Some(node1.addr))
        .build()
        .await?;
    
    println!("✓ Created node 2 at {}, joining {}", node2.addr, node1.addr);
    
    // Send join request from node 2
    node2.node.send_join_request().await?;
    
    // Create node 3 that will also join node 1
    let node3 = TestNode::builder()
        .with_id(3)
        .with_auto_port()
        .with_join_addr(Some(node1.addr))
        .build()
        .await?;
    
    println!("✓ Created node 3 at {}, joining {}", node3.addr, node1.addr);
    
    // Send join request from node 3
    node3.node.send_join_request().await?;
    
    // Wait for nodes to process join requests
    timing::robust_sleep(Duration::from_secs(3)).await;
    
    // Verify cluster state
    println!("\nCluster Status:");
    for (id, node) in [(1, &node1), (2, &node2), (3, &node3)] {
        let status = node.shared_state.get_raft_status().await?;
        println!(
            "  Node {}: state={}, leader={:?}, term={}",
            id, status.state, status.leader_id, status.term
        );
    }
    
    // Clean up
    node3.shutdown().await;
    node2.shutdown().await;
    node1.shutdown().await;
    
    println!("\n");
    
    // Method 2: Using TestCluster abstraction (when it works)
    println!("Method 2: TestCluster Abstraction");
    println!("---------------------------------");
    println!("The TestCluster::builder() approach is the preferred method,");
    println!("but currently has reliability issues with 3-node clusters.");
    println!("\nRecommended approach for now:");
    println!("1. Create nodes individually with TestNode::builder()");
    println!("2. Use explicit join_addr parameter for nodes 2 and 3");
    println!("3. Call send_join_request() explicitly after node creation");
    println!("4. Use timing::robust_sleep() instead of hardcoded delays");
    println!("5. Verify cluster state before proceeding with tests");
    
    println!("\n=== Key Patterns for Reliable 3-Node Tests ===");
    println!("1. Bootstrap node 1 first and let it become leader");
    println!("2. Add nodes 2 and 3 sequentially, not in parallel");
    println!("3. Use generous timeouts (30s+) for convergence checks");
    println!("4. Send dummy operations (like health checks) to trigger log replication");
    println!("5. Check cluster status from multiple nodes to ensure agreement");
    
    Ok(())
}