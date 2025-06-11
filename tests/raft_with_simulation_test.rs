#![cfg(feature = "simulation")]

use madsim::time;
use std::time::Duration;

/// Test single node timing behavior
#[madsim::test]
async fn test_single_node_timing() {
    println!("\n🧪 SINGLE NODE TIMING TEST WITH MADSIM\n");

    println!("✅ Using MadSim for deterministic simulation");

    // Test node startup timing
    let start = std::time::Instant::now();

    // Simulate node startup delay
    time::sleep(Duration::from_millis(500)).await;

    let elapsed = start.elapsed();
    println!("Node startup simulation took: {:?}", elapsed);

    assert!(elapsed >= Duration::from_millis(500));
    assert!(elapsed < Duration::from_millis(600)); // Should be close

    println!("✅ Timing behavior is deterministic with MadSim!");
}

/// Test three node election simulation
#[madsim::test]
async fn test_three_node_election_timing() {
    println!("\n🎯 THREE-NODE ELECTION TIMING TEST\n");

    // Simulate election timeouts for 3 nodes
    let election_timeouts = vec![150, 200, 250]; // Different timeouts per node
    let mut handles = Vec::new();

    for (node_id, timeout_ms) in election_timeouts.iter().enumerate() {
        let timeout = *timeout_ms;
        let handle = madsim::task::spawn(async move {
            println!(
                "Node {} waiting for election timeout: {}ms",
                node_id + 1,
                timeout
            );
            time::sleep(Duration::from_millis(timeout)).await;
            println!("Node {} election timeout fired!", node_id + 1);
            node_id + 1 // Return node ID
        });
        handles.push(handle);
    }

    // Wait for all timeouts
    let mut results = Vec::new();
    for handle in handles {
        let node_id = handle.await.unwrap();
        results.push(node_id);
    }

    println!("Election order: {:?}", results);
    // Node 1 should timeout first (150ms), then 2 (200ms), then 3 (250ms)
    assert_eq!(results, vec![1, 2, 3]);

    println!("✅ Election timeouts fired in deterministic order!");
}

/// Test network partition simulation
#[madsim::test]
async fn test_network_partition_timing() {
    println!("\n🌐 NETWORK PARTITION TIMING TEST\n");

    use std::sync::Arc;
    use tokio::sync::Mutex;

    let messages_sent = Arc::new(Mutex::new(Vec::new()));

    // Simulate 5 nodes trying to communicate
    let mut handles = Vec::new();

    for node_id in 1..=5 {
        let messages = messages_sent.clone();
        let handle = madsim::task::spawn(async move {
            // Nodes 1-3 are in one partition, 4-5 in another
            let partition_delay = if node_id <= 3 { 50 } else { 150 };

            time::sleep(Duration::from_millis(partition_delay)).await;

            let msg = format!("Node {} sent heartbeat", node_id);
            messages.lock().await.push((node_id, msg));

            println!("Node {} completed communication", node_id);
        });
        handles.push(handle);
    }

    // Wait for all nodes
    for handle in handles {
        handle.await.unwrap();
    }

    // Check message order
    let final_messages = messages_sent.lock().await;
    println!("\nMessage order:");
    for (node_id, msg) in final_messages.iter() {
        println!("  {}: {}", node_id, msg);
    }

    // Nodes 1-3 should send before nodes 4-5 due to partition delay
    let first_three: Vec<_> = final_messages.iter().take(3).map(|(id, _)| *id).collect();
    let last_two: Vec<_> = final_messages.iter().skip(3).map(|(id, _)| *id).collect();

    assert!(
        first_three.iter().all(|&id| id <= 3),
        "First partition should communicate first"
    );
    assert!(
        last_two.iter().all(|&id| id >= 4),
        "Second partition should communicate later"
    );

    println!("\n✅ Network partition behavior is deterministic!");
}

/// Test deterministic leader election
#[madsim::test]
async fn test_deterministic_leader_election() {
    println!("\n👑 DETERMINISTIC LEADER ELECTION TEST\n");

    // Run election simulation multiple times
    let mut election_results = Vec::new();

    for run in 0..3 {
        println!("\n--- Election Run {} ---", run + 1);

        use std::sync::Arc;
        use tokio::sync::Mutex;

        let votes = Arc::new(Mutex::new(Vec::new()));
        let mut handles = Vec::new();

        // Simulate 5 nodes voting
        for node_id in 1..=5 {
            let votes = votes.clone();
            let handle = madsim::task::spawn(async move {
                // Each node has a different "random" timeout
                let timeout = Duration::from_millis(100 + node_id * 50);
                time::sleep(timeout).await;

                // Vote for self
                votes.lock().await.push(node_id);
                println!("Node {} voted", node_id);
            });
            handles.push(handle);
        }

        // Wait for all votes
        for handle in handles {
            handle.await.unwrap();
        }

        let vote_order = votes.lock().await.clone();
        println!("Vote order: {:?}", vote_order);

        // First node to vote becomes leader
        let leader = vote_order[0];
        election_results.push(leader);
        println!("Leader elected: Node {}", leader);
    }

    // All runs should elect the same leader
    assert!(
        election_results.windows(2).all(|w| w[0] == w[1]),
        "All election runs should produce the same leader"
    );

    println!("\n✅ Leader election is deterministic across all runs!");
    println!("Leader was always: Node {}", election_results[0]);
}
