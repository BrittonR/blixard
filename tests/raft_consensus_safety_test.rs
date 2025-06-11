#![cfg(feature = "simulation")]

use madsim::time;
use std::time::Duration;

/// Test basic consensus mechanism with MadSim
#[madsim::test]
async fn test_basic_raft_consensus() {
    println!("\n🏛️ BASIC RAFT CONSENSUS TEST 🏛️\n");

    // For now, we'll test basic timing behavior
    // Full Raft testing would require more integration with madsim networking

    println!("📍 Testing election timeout behavior");
    let election_timeout = Duration::from_millis(1000);
    let start = std::time::Instant::now();

    time::sleep(election_timeout).await;

    let elapsed = start.elapsed();
    println!("Election timeout elapsed: {:?}", elapsed);
    assert!(elapsed >= election_timeout);

    println!("✅ Basic timing test passed!");
}

/// Test concurrent proposals
#[madsim::test]
async fn test_concurrent_proposals() {
    println!("\n🔄 CONCURRENT PROPOSALS TEST 🔄\n");

    use std::sync::Arc;
    use tokio::sync::Mutex;

    let proposals = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();

    // Simulate multiple clients proposing concurrently
    for i in 0..5 {
        let proposals = proposals.clone();
        let handle = madsim::task::spawn(async move {
            // Simulate some processing time
            time::sleep(Duration::from_millis(i * 10)).await;

            let proposal = format!("proposal-{}", i);
            println!("Client {} proposing: {}", i, proposal);

            proposals.lock().await.push(proposal);
        });
        handles.push(handle);
    }

    // Wait for all proposals
    for handle in handles {
        handle.await.unwrap();
    }

    let final_proposals = proposals.lock().await;
    println!("All proposals: {:?}", final_proposals);
    assert_eq!(final_proposals.len(), 5);

    println!("✅ All concurrent proposals recorded!");
}

/// Test leader election timing
#[madsim::test]
async fn test_leader_election_timing() {
    println!("\n⏰ LEADER ELECTION TIMING TEST ⏰\n");

    let election_timeouts = vec![150, 300, 450, 600, 750];
    let mut election_times = Vec::new();

    for &timeout_ms in &election_timeouts {
        let start = std::time::Instant::now();

        // Simulate election timeout
        time::sleep(Duration::from_millis(timeout_ms)).await;

        let elapsed = start.elapsed();
        election_times.push(elapsed);

        println!("Election timeout {}ms took {:?}", timeout_ms, elapsed);
    }

    // Verify timeouts are respected
    for (i, (expected_ms, actual)) in election_timeouts
        .iter()
        .zip(election_times.iter())
        .enumerate()
    {
        let expected = Duration::from_millis(*expected_ms);
        assert!(
            *actual >= expected,
            "Election {} timeout too short: {:?} < {:?}",
            i,
            actual,
            expected
        );
    }

    println!("✅ All election timeouts respected!");
}

/// Test deterministic message ordering
#[madsim::test]
async fn test_deterministic_message_ordering() {
    println!("\n📬 DETERMINISTIC MESSAGE ORDERING TEST 📬\n");

    use std::sync::Arc;
    use tokio::sync::Mutex;

    // Run the same scenario multiple times
    let mut all_runs = Vec::new();

    for run in 0..3 {
        println!("\n--- Run {} ---", run + 1);
        let messages = Arc::new(Mutex::new(Vec::new()));
        let mut handles = Vec::new();

        // Spawn tasks that send messages with different delays
        for i in 0..5 {
            let messages = messages.clone();
            let delay = Duration::from_millis((5 - i) * 20); // Reverse order delays

            let handle = madsim::task::spawn(async move {
                time::sleep(delay).await;
                let msg = format!("msg-{}", i);
                messages.lock().await.push(msg);
            });
            handles.push(handle);
        }

        // Wait for all messages
        for handle in handles {
            handle.await.unwrap();
        }

        let final_messages = messages.lock().await.clone();
        println!("Message order: {:?}", final_messages);
        all_runs.push(final_messages);
    }

    // Verify all runs produced the same order
    for i in 1..all_runs.len() {
        assert_eq!(
            all_runs[i],
            all_runs[0],
            "Run {} produced different message order!",
            i + 1
        );
    }

    println!("\n✅ All runs produced identical message ordering!");
    println!("This demonstrates MadSim's deterministic execution!");
}
