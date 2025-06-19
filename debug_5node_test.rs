use std::time::Duration;
use blixard::test_helpers::TestCluster;

#[tokio::main]
async fn main() {
    // Enable debug logging
    tracing_subscriber::fmt()
        .with_env_filter("blixard=debug,info")
        .init();

    println!("Starting 5-node cluster formation debug test");
    
    // Create nodes one by one with detailed logging
    let mut cluster = TestCluster::builder()
        .with_nodes(1)
        .build()
        .await
        .expect("Failed to create bootstrap node");
    
    println!("\n=== Bootstrap node created ===");
    dump_cluster_state(&cluster).await;
    
    // Add node 2
    println!("\n=== Adding node 2 ===");
    let node2_id = cluster.add_node().await.expect("Failed to add node 2");
    println!("Node 2 added with ID: {}", node2_id);
    
    // Wait a bit and check state
    tokio::time::sleep(Duration::from_secs(2)).await;
    dump_cluster_state(&cluster).await;
    
    // Add node 3
    println!("\n=== Adding node 3 ===");
    let node3_id = cluster.add_node().await.expect("Failed to add node 3");
    println!("Node 3 added with ID: {}", node3_id);
    
    // Wait and check state again
    tokio::time::sleep(Duration::from_secs(2)).await;
    dump_cluster_state(&cluster).await;
    
    // Add node 4
    println!("\n=== Adding node 4 ===");
    let node4_id = cluster.add_node().await.expect("Failed to add node 4");
    println!("Node 4 added with ID: {}", node4_id);
    
    // Wait and check state
    tokio::time::sleep(Duration::from_secs(2)).await;
    dump_cluster_state(&cluster).await;
    
    // Add node 5
    println!("\n=== Adding node 5 ===");
    let node5_id = cluster.add_node().await.expect("Failed to add node 5");
    println!("Node 5 added with ID: {}", node5_id);
    
    // Final state check
    tokio::time::sleep(Duration::from_secs(2)).await;
    dump_cluster_state(&cluster).await;
    
    println!("\n=== Test complete, shutting down ===");
    cluster.shutdown().await;
}

async fn dump_cluster_state(cluster: &TestCluster) {
    println!("\nCluster State:");
    for (id, node) in cluster.nodes() {
        println!("  Node {}:", id);
        
        // Get cluster status (voters)
        match node.shared_state.get_cluster_status().await {
            Ok((leader_id, voters, term)) => {
                println!("    Leader: {}, Term: {}, Voters: {:?}", leader_id, term, voters);
            }
            Err(e) => {
                println!("    Error getting cluster status: {}", e);
            }
        }
        
        // Get peers
        let peers = node.shared_state.get_peers().await;
        println!("    Peers: {} total", peers.len());
        for peer in peers {
            println!("      - {} at {} (connected: {})", peer.id, peer.address, peer.is_connected);
        }
        
        // Get Raft status
        if let Ok(status) = node.shared_state.get_raft_status().await {
            println!("    Raft: {} (leader_id: {:?})", status.state, status.leader_id);
        }
    }
}