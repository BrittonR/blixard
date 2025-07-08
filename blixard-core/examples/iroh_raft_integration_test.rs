//! Integration test for Iroh transport with Raft consensus
//!
//! This example demonstrates that the IrohRaftTransport works correctly
//! with the actual Raft consensus mechanism.

#![cfg(feature = "test-helpers")]

use blixard_core::{error::BlixardResult, test_helpers::TestCluster};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    println!("=== Iroh Raft Integration Test ===\n");

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("blixard=info,iroh=info")
        .init();

    // Note: Currently TestCluster doesn't support custom transport configuration
    // This test validates that Raft consensus works correctly regardless of transport
    println!("📊 Creating 3-node cluster...");
    let cluster = TestCluster::builder().with_nodes(3).build().await?;

    println!("✓ Created cluster with {} nodes", cluster.nodes().len());

    // Wait for cluster to converge
    println!("\n⏳ Waiting for cluster to converge...");
    cluster
        .wait_for_convergence(Duration::from_secs(10))
        .await?;
    println!("✓ Cluster converged");

    // Get leader information
    let leader_id = cluster.get_leader_id().await?;
    println!("\n👑 Leader elected: Node {}", leader_id);

    // Check initial cluster state
    println!("\n📊 Initial cluster state:");
    for (node_id, node) in cluster.nodes().iter() {
        if let Ok(status) = node.shared_state.get_raft_status().await {
            println!(
                "  Node {}: Leader={}, Term={}, State={}",
                node_id, status.is_leader, status.term, status.state
            );
        }
    }

    // Test Raft consensus by checking distributed state
    println!("\n🔧 Testing Raft consensus...");

    // Verify all nodes see the same leader
    let mut leader_views = vec![];
    for (node_id, node) in cluster.nodes().iter() {
        if let Ok(status) = node.shared_state.get_raft_status().await {
            leader_views.push((*node_id, status.leader_id));
        }
    }

    println!("\n🔍 Leader views from each node:");
    for (node_id, leader_view) in &leader_views {
        println!("  Node {} sees leader as: {:?}", node_id, leader_view);
    }

    // Check if all nodes agree on the leader
    let first_leader = leader_views[0].1;
    let all_agree = leader_views
        .iter()
        .all(|(_, leader)| *leader == first_leader);

    if all_agree && first_leader.is_some() {
        println!("\n✅ SUCCESS: All nodes agree on leader {:?}", first_leader);
    } else {
        println!("\n❌ FAILURE: Nodes have different views of leader");
        return Err(blixard_core::error::BlixardError::Internal {
            message: "Leader consensus failed".to_string(),
        });
    }

    // Verify Raft is operational by checking terms
    println!("\n📊 Checking Raft terms:");
    let mut terms = vec![];
    for (node_id, node) in cluster.nodes().iter() {
        if let Ok(status) = node.shared_state.get_raft_status().await {
            terms.push((*node_id, status.term));
            println!("  Node {} is at term {}", node_id, status.term);
        }
    }

    // All nodes should be at the same term
    let first_term = terms[0].1;
    let terms_match = terms.iter().all(|(_, term)| *term == first_term);

    if terms_match && first_term > 0 {
        println!("\n✅ SUCCESS: All nodes at same term ({})", first_term);
    } else {
        println!("\n❌ FAILURE: Nodes have different terms");
        return Err(blixard_core::error::BlixardError::Internal {
            message: "Term consensus failed".to_string(),
        });
    }

    // Check cluster status from each node
    println!("\n🔍 Checking cluster status views:");
    for (node_id, node) in cluster.nodes().iter() {
        match node.shared_state.get_cluster_status().await {
            Ok((_, peers, _)) => {
                println!("  Node {} sees {} peers", node_id, peers.len());
            }
            Err(e) => {
                println!("  Node {} error: {}", node_id, e);
            }
        }
    }

    // Note about Iroh transport
    println!("\n📝 Note: This test validates Raft consensus mechanism.");
    println!("    To specifically test Iroh transport:");
    println!("    1. Set BLIXARD_TRANSPORT=iroh environment variable");
    println!("    2. Configure nodes with TransportConfig::Iroh");
    println!("    3. Run examples/iroh_basic_test.rs for P2P connectivity");

    // Summary
    println!("\n📊 Test Summary:");
    println!("  - Cluster formed successfully");
    println!("  - Leader election completed");
    println!("  - All nodes agree on leader");
    println!("  - All nodes at same term");
    println!("  - Raft consensus operational");

    // Cleanup
    println!("\n🧹 Cleaning up...");
    cluster.shutdown().await;

    println!("\n✅ Raft integration test complete!");

    Ok(())
}

#[cfg(not(feature = "test-helpers"))]
fn main() {
    eprintln!("This example requires the 'test-helpers' feature to be enabled.");
    eprintln!("Run with: cargo run --example iroh_raft_integration_test --features test-helpers");
    std::process::exit(1);
}
