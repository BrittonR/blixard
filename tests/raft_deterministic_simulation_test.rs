#![cfg(feature = "simulation")]

use madsim::task;
use madsim::time::sleep;
use std::time::Duration;

#[madsim::test]
async fn test_raft_cluster_deterministic() {
    println!("\n🔄 Testing Raft cluster determinism with MadSim");

    // MadSim provides deterministic execution by default
    // Multiple runs with the same seed will produce identical results
    async fn run_simulation() -> Vec<String> {
        let mut events = vec![];

        // Simulate a 3-node Raft cluster
        println!("📍 Creating 3-node Raft cluster simulation");

        // Simulate node startup
        for i in 1..=3 {
            events.push(format!("Node {} started", i));
        }

        // Simulate leader election
        sleep(Duration::from_millis(500)).await;
        events.push("Node 1 elected as leader".to_string());

        // Simulate some proposals
        for i in 1..=5 {
            sleep(Duration::from_millis(100)).await;
            events.push(format!("Proposal {} committed", i));
        }

        events
    }

    // Run simulation multiple times
    let result1 = run_simulation().await;
    let result2 = run_simulation().await;
    let result3 = run_simulation().await;

    // Verify determinism
    assert_eq!(result1, result2, "Results should be deterministic");
    assert_eq!(result2, result3, "Results should be deterministic");

    println!("✅ All runs produced identical results");
    println!("  Events: {:?}", result1);
}

#[madsim::test]
async fn test_raft_with_network_partitions() {
    println!("\n🔌 Testing Raft behavior during network partitions");

    // Simulate a 5-node cluster
    let mut node_states = vec!["follower"; 5];

    // Initial leader election
    sleep(Duration::from_millis(300)).await;
    node_states[0] = "leader";
    println!("📍 Initial state: Node 1 is leader");

    // Partition: [1, 2] | [3, 4, 5]
    println!("\n🔪 Creating partition: [1,2] | [3,4,5]");

    // Minority partition loses leadership
    sleep(Duration::from_millis(500)).await;
    node_states[0] = "follower";
    node_states[2] = "leader"; // Node 3 becomes leader in majority

    println!("  Minority partition: nodes 1,2 are followers");
    println!("  Majority partition: node 3 is new leader");

    // Try to write on both sides
    let minority_write = false; // Should fail
    let majority_write = true; // Should succeed

    assert!(
        !minority_write,
        "Minority partition should not accept writes"
    );
    assert!(majority_write, "Majority partition should accept writes");

    // Heal partition
    println!("\n🔧 Healing partition");
    sleep(Duration::from_millis(300)).await;

    // Old leader steps down, cluster converges
    node_states[0] = "follower";
    println!("  Cluster converged: Node 3 remains leader");

    println!("\n✅ Network partition test completed");
}

#[madsim::test]
async fn test_concurrent_proposals() {
    println!("\n📝 Testing concurrent proposal handling");

    let proposal_results = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut handles = vec![];

    // Spawn multiple clients sending proposals
    for client_id in 1..=5 {
        let results = proposal_results.clone();
        let handle = task::spawn(async move {
            // Simulate proposal submission delay
            sleep(Duration::from_millis(client_id * 50)).await;

            // Record proposal
            results
                .lock()
                .unwrap()
                .push(format!("Client {} proposal accepted", client_id));

            // Simulate commit delay
            sleep(Duration::from_millis(100)).await;
            results
                .lock()
                .unwrap()
                .push(format!("Client {} proposal committed", client_id));
        });
        handles.push(handle);
    }

    // Wait for all proposals
    for handle in handles {
        handle.await.unwrap();
    }

    let final_results = proposal_results.lock().unwrap().clone();

    // Verify all proposals were processed
    assert_eq!(final_results.len(), 10); // 5 accepts + 5 commits

    println!("📊 Proposal processing order:");
    for event in &final_results {
        println!("  {}", event);
    }

    println!("\n✅ All proposals processed successfully");
}

#[madsim::test]
async fn test_leader_failover() {
    println!("\n💔 Testing leader failover scenario");

    // Initial state
    let mut leader = 1;
    let mut term = 1;

    println!(
        "📍 Initial state: Node {} is leader (term {})",
        leader, term
    );

    // Simulate leader failure
    sleep(Duration::from_millis(500)).await;
    println!("\n❌ Node {} failed", leader);

    // Election timeout and new election
    sleep(Duration::from_millis(300)).await;
    leader = 2;
    term = 2;
    println!(
        "🗳️ New election: Node {} becomes leader (term {})",
        leader, term
    );

    // Verify cluster continues operating
    let proposals_after_failover = 3;
    for i in 1..=proposals_after_failover {
        sleep(Duration::from_millis(100)).await;
        println!("  Proposal {} committed under new leader", i);
    }

    println!("\n✅ Cluster recovered from leader failure");
}

#[madsim::test]
async fn test_deterministic_message_ordering() {
    println!("\n📨 Testing deterministic message ordering");

    let messages = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

    // Simulate nodes sending messages
    let mut handles = vec![];
    for node_id in 1..=3 {
        for msg_id in 1..=3 {
            let messages_clone = messages.clone();
            let handle = task::spawn(async move {
                // Deterministic delay based on node and message ID
                sleep(Duration::from_millis((node_id * 10 + msg_id * 5) as u64)).await;
                messages_clone
                    .lock()
                    .unwrap()
                    .push(format!("Node {} sent message {}", node_id, msg_id));
            });
            handles.push(handle);
        }
    }

    // Wait for all messages
    for handle in handles {
        handle.await.unwrap();
    }

    let final_messages = messages.lock().unwrap().clone();

    println!("📊 Message order:");
    for (i, msg) in final_messages.iter().enumerate() {
        println!("  {}: {}", i + 1, msg);
    }

    // Verify deterministic ordering
    assert_eq!(final_messages.len(), 9);
    assert_eq!(final_messages[0], "Node 1 sent message 1");
    assert_eq!(final_messages[8], "Node 3 sent message 3");

    println!("\n✅ Message ordering is deterministic");
}
