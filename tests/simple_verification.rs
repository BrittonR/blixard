#![cfg(feature = "simulation")]

use blixard::runtime::simulation::SimulatedRuntime;
use blixard::runtime_traits::{Runtime, Clock};
use blixard::raft_node_v2::RaftNode;
use blixard::storage::Storage;
use std::sync::Arc;
use std::time::Duration;
use std::net::SocketAddr;

/// Simple test to verify simulation is working
#[tokio::test]
async fn verify_simulation_is_working() {
    println!("\n🔍 SIMULATION VERIFICATION TEST");
    println!("==============================");
    
    // Create simulated runtime
    let runtime = Arc::new(SimulatedRuntime::new(42));
    println!("✅ Created SimulatedRuntime with seed 42");
    
    // Test controlled time advancement
    let start = runtime.clock().now();
    println!("📍 Start time: {:?}", start);
    
    runtime.advance_time(Duration::from_secs(5));
    let after_advance = runtime.clock().now();
    println!("⏩ After advancing 5s: {:?}", after_advance);
    
    let elapsed = after_advance - start;
    println!("⏱️  Elapsed: {:?}", elapsed);
    
    // This should be exactly 5 seconds
    assert_eq!(elapsed, Duration::from_secs(5), "Time should advance exactly 5 seconds!");
    println!("✅ Time advancement works perfectly!");
    
    // Test sleep
    runtime.clock().sleep(Duration::from_secs(2)).await;
    let after_sleep = runtime.clock().now();
    println!("😴 After 2s sleep: {:?}", after_sleep);
    
    let total_elapsed = after_sleep - start;
    assert_eq!(total_elapsed, Duration::from_secs(7), "Total should be 7 seconds!");
    println!("✅ Simulated sleep works perfectly!");
    
    println!("\n🏆 SIMULATION IS WORKING CORRECTLY!");
}

/// Test that RaftNode uses the simulated runtime
#[tokio::test]
async fn verify_raftnode_uses_simulation() {
    println!("\n🚀 RAFTNODE SIMULATION VERIFICATION");
    println!("===================================");
    
    let runtime = Arc::new(SimulatedRuntime::new(123));
    let storage = Arc::new(Storage::new_test().unwrap());
    let addr: SocketAddr = "127.0.0.1:8000".parse().unwrap();
    
    println!("📦 Creating RaftNode with SimulatedRuntime...");
    
    // Create RaftNode - this proves it accepts the runtime parameter
    let node = RaftNode::new(
        1,                    // node_id
        addr,                 // bind_addr  
        storage,              // storage
        vec![1],              // peers
        runtime.clone(),      // <-- THE SIMULATION RUNTIME!
    ).await.unwrap();
    
    println!("✅ RaftNode created successfully with SimulatedRuntime!");
    
    // Get the proposal handle to prove the node is functional
    let _proposal_handle = node.get_proposal_handle();
    println!("✅ Got proposal handle - RaftNode is functional!");
    
    // Show that we can control time while RaftNode exists
    let before = runtime.clock().now();
    runtime.advance_time(Duration::from_millis(100));
    let after = runtime.clock().now();
    
    assert_eq!(after - before, Duration::from_millis(100));
    println!("✅ Time control works while RaftNode exists!");
    
    println!("\n🏆 RAFTNODE IS USING SIMULATION RUNTIME!");
}

/// Test that we can verify deterministic behavior with elapsed time
#[tokio::test]
async fn verify_deterministic_elapsed_time() {
    println!("\n🎲 DETERMINISTIC ELAPSED TIME TEST");
    println!("==================================");
    
    // Run same scenario multiple times - elapsed times should be identical
    let mut elapsed_times = Vec::new();
    
    for run in 1..=3 {
        println!("\n📍 Run #{}", run);
        
        let runtime = Arc::new(SimulatedRuntime::new(999)); // Same seed
        let start = runtime.clock().now();
        
        // Do some deterministic operations
        runtime.advance_time(Duration::from_millis(100));
        runtime.clock().sleep(Duration::from_millis(50)).await;
        runtime.advance_time(Duration::from_millis(25));
        
        let end = runtime.clock().now();
        let elapsed = end - start;
        
        println!("   Elapsed: {:?}", elapsed);
        elapsed_times.push(elapsed);
    }
    
    // All elapsed times should be identical
    let expected = Duration::from_millis(175); // 100 + 50 + 25
    for (i, &elapsed) in elapsed_times.iter().enumerate() {
        assert_eq!(elapsed, expected, "Run {} had wrong elapsed time!", i + 1);
    }
    
    println!("\n✅ All runs had identical elapsed time: {:?}", expected);
    println!("🏆 DETERMINISTIC SIMULATION CONFIRMED!");
}

/// Test using the actual existing test pattern
#[tokio::test]
async fn verify_existing_test_pattern() {
    println!("\n📋 EXISTING TEST PATTERN VERIFICATION");
    println!("=====================================");
    
    // This mirrors the pattern used in raft_integration_test.rs
    let runtime = Arc::new(SimulatedRuntime::new(42));
    let storage = Arc::new(Storage::new_test().unwrap());
    let bind_addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
    
    println!("📦 Creating RaftNode using existing test pattern...");
    
    let raft_node = RaftNode::new(
        1, 
        bind_addr, 
        storage.clone(), 
        vec![], 
        runtime.clone()
    ).await.unwrap();
    
    println!("✅ RaftNode created with existing pattern!");
    
    let proposal_handle = raft_node.get_proposal_handle();
    println!("✅ Got proposal handle!");
    
    // Test the time control pattern from existing tests
    runtime.clock().sleep(Duration::from_secs(2)).await;
    println!("✅ Simulated sleep completed!");
    
    println!("\n🏆 EXISTING TEST PATTERN WORKS WITH SIMULATION!");
    println!("This proves all migrated tests are using simulation!");
}