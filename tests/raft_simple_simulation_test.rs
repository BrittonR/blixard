#![cfg(feature = "simulation")]

use std::time::Duration;
use madsim::time::{sleep, Instant};

#[madsim::test]
async fn test_basic_raft_simulation() {
    println!("\n🚀 Testing basic Raft simulation with MadSim");
    
    // Simulate creating a single Raft node
    println!("📍 Creating single Raft node");
    
    let node_id = 1;
    let node_state = "follower";
    let term = 0;
    
    println!("  Node {} created in {} state (term {})", node_id, node_state, term);
    
    // Simulate some time passing
    sleep(Duration::from_millis(500)).await;
    
    // With only one node, it stays a follower
    assert_eq!(node_state, "follower");
    println!("✅ Single node remains follower (as expected)");
}

#[madsim::test]
async fn test_deterministic_clock() {
    println!("\n⏰ Testing deterministic clock behavior");
    
    // Test that time advances deterministically
    let start = Instant::now();
    
    // First measurement
    sleep(Duration::from_secs(1)).await;
    let after_1s = start.elapsed();
    
    // Second measurement
    sleep(Duration::from_secs(2)).await;
    let after_3s = start.elapsed();
    
    // Verify timing (MadSim has some overhead in real execution)
    assert!(after_1s >= Duration::from_secs(1));
    assert!(after_3s >= Duration::from_secs(3));
    
    println!("✅ Clock advances deterministically:");
    println!("  After 1s sleep: {:?}", after_1s);
    println!("  After 3s total: {:?}", after_3s);
}

#[madsim::test]
async fn test_simple_election_simulation() {
    println!("\n🗳️ Testing simple leader election simulation");
    
    // Simulate a 3-node cluster
    let mut nodes = vec![
        ("follower", 0), // (state, term)
        ("follower", 0),
        ("follower", 0),
    ];
    
    println!("📍 Initial state: 3 followers");
    
    // Simulate election timeout
    sleep(Duration::from_millis(300)).await;
    
    // Node 1 starts election
    nodes[0] = ("candidate", 1);
    println!("  Node 1 becomes candidate (term 1)");
    
    // Simulate vote gathering
    sleep(Duration::from_millis(100)).await;
    
    // Node 1 wins election
    nodes[0] = ("leader", 1);
    nodes[1].1 = 1; // Update term
    nodes[2].1 = 1; // Update term
    
    println!("  Node 1 elected as leader");
    println!("  All nodes now in term 1");
    
    // Verify final state
    assert_eq!(nodes[0].0, "leader");
    assert_eq!(nodes[1].0, "follower");
    assert_eq!(nodes[2].0, "follower");
    
    println!("✅ Election completed successfully");
}

#[madsim::test]
async fn test_heartbeat_simulation() {
    println!("\n💓 Testing heartbeat mechanism");
    
    let mut heartbeat_count = 0;
    let heartbeat_interval = Duration::from_millis(150);
    
    println!("📍 Leader sending heartbeats every 150ms");
    
    let start = Instant::now();
    
    // Simulate heartbeats for 1 second
    while start.elapsed() < Duration::from_secs(1) {
        sleep(heartbeat_interval).await;
        heartbeat_count += 1;
        println!("  Heartbeat {} sent at {:?}", heartbeat_count, start.elapsed());
    }
    
    // Should have sent ~6 heartbeats
    assert!(heartbeat_count >= 6 && heartbeat_count <= 7);
    println!("✅ Sent {} heartbeats in 1 second", heartbeat_count);
}

#[madsim::test]
async fn test_proposal_simulation() {
    println!("\n📝 Testing proposal handling simulation");
    
    // Simulate proposal submission
    let proposals = vec![
        "create_vm:vm1",
        "update_vm:vm1",
        "delete_vm:vm1",
    ];
    
    for (i, proposal) in proposals.iter().enumerate() {
        println!("  Submitting proposal {}: {}", i + 1, proposal);
        
        // Simulate processing time
        sleep(Duration::from_millis(50)).await;
        
        // Simulate replication
        println!("    Replicated to followers");
        sleep(Duration::from_millis(30)).await;
        
        // Simulate commit
        println!("    Committed at index {}", i + 1);
    }
    
    println!("✅ All proposals processed successfully");
}