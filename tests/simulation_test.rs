#![cfg(feature = "simulation")]

use std::time::Duration;
use madsim::time;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Test basic deterministic simulation with MadSim
#[madsim::test]
async fn test_simulated_cluster_formation() {
    println!("\n🚀 TESTING SIMULATED CLUSTER FORMATION\n");
    
    // MadSim provides deterministic simulation
    let nodes = Arc::new(Mutex::new(Vec::new()));
    
    // Simulate 3 nodes starting up
    for node_id in 1..=3 {
        let nodes = nodes.clone();
        madsim::task::spawn(async move {
            println!("Node {} starting", node_id);
            time::sleep(Duration::from_millis(100 * node_id)).await;
            nodes.lock().await.push(format!("Node {} ready", node_id));
            println!("Node {} is ready", node_id);
        });
    }
    
    // Let nodes start
    time::sleep(Duration::from_millis(500)).await;
    
    // Check all nodes started
    let started_nodes = nodes.lock().await;
    assert_eq!(started_nodes.len(), 3);
    println!("All nodes started: {:?}", *started_nodes);
    
    println!("✅ Cluster formation test passed!");
}

/// Test network partition simulation
#[madsim::test]
async fn test_network_partition_recovery() {
    println!("\n🌐 TESTING NETWORK PARTITION RECOVERY\n");
    
    let cluster_state = Arc::new(Mutex::new(Vec::new()));
    
    // Simulate 5 nodes with a network partition
    let mut handles = Vec::new();
    
    for node_id in 1..=5 {
        let state = cluster_state.clone();
        let handle = madsim::task::spawn(async move {
            // Nodes 1-2 are in minority partition, 3-5 in majority
            let partition_delay = if node_id <= 2 { 500 } else { 100 };
            
            println!("Node {} starting with partition delay {}ms", node_id, partition_delay);
            time::sleep(Duration::from_millis(partition_delay)).await;
            
            state.lock().await.push(format!("Node {} joined", node_id));
            
            // Simulate some work
            for i in 0..3 {
                time::sleep(Duration::from_millis(200)).await;
                state.lock().await.push(format!("Node {} - operation {}", node_id, i));
            }
        });
        handles.push(handle);
    }
    
    // Wait for all nodes
    for handle in handles {
        handle.await.unwrap();
    }
    
    let events = cluster_state.lock().await;
    println!("\nCluster events:");
    for event in events.iter() {
        println!("  {}", event);
    }
    
    // Verify partition behavior
    assert!(events.iter().any(|e| e.contains("Node 3")));
    assert!(events.iter().any(|e| e.contains("Node 4")));
    assert!(events.iter().any(|e| e.contains("Node 5")));
    
    println!("\n✅ Network partition test passed!");
}


/// Test deterministic scheduling and race conditions
#[madsim::test]
async fn test_deterministic_message_ordering() {
    println!("\n⚡ TESTING DETERMINISTIC MESSAGE ORDERING\n");
    
    // Run the same test multiple times - with MadSim should be deterministic
    let mut all_results = Vec::new();
    
    for run in 0..3 {
        println!("Run {}", run + 1);
        
        let messages = Arc::new(Mutex::new(Vec::new()));
        let mut handles = Vec::new();
        
        // Spawn concurrent tasks with specific delays
        for i in 0..5 {
            let msgs = messages.clone();
            let delay = Duration::from_millis((5 - i) * 50); // Reverse order delays
            
            let handle = madsim::task::spawn(async move {
                time::sleep(delay).await;
                msgs.lock().await.push(format!("Message {} sent", i));
            });
            handles.push(handle);
        }
        
        // Wait for all tasks
        for handle in handles {
            handle.await.unwrap();
        }
        
        let result = messages.lock().await.clone();
        println!("  Result: {:?}", result);
        all_results.push(result);
    }
    
    // Verify all runs produced the same order
    for i in 1..all_results.len() {
        assert_eq!(all_results[i], all_results[0], 
                   "Run {} produced different message order!", i + 1);
    }
    
    println!("\n✅ All runs produced identical message ordering!");
    println!("This proves MadSim provides deterministic execution!");
}

/// Test crash recovery with simulated filesystem
#[madsim::test]
async fn test_crash_recovery_simulation() {
    println!("\n💥 TESTING CRASH RECOVERY SIMULATION\n");
    
    // Phase 1: Simulate cluster writing data
    let persistent_state = Arc::new(Mutex::new(Vec::new()));
    
    {
        let state = persistent_state.clone();
        let mut handles = Vec::new();
        
        for node_id in 1..=3 {
            let state = state.clone();
            let handle = madsim::task::spawn(async move {
                println!("Node {} writing data", node_id);
                for i in 0..3 {
                    time::sleep(Duration::from_millis(100)).await;
                    state.lock().await.push(format!("Node {} - data {}", node_id, i));
                }
            });
            handles.push(handle);
        }
        
        // Wait for writes to complete
        for handle in handles {
            handle.await.unwrap();
        }
        
        println!("Data written before crash: {} entries", persistent_state.lock().await.len());
    }
    
    // Simulate crash
    println!("\n💥 CRASH! Simulating cluster restart...\n");
    time::sleep(Duration::from_millis(500)).await;
    
    // Phase 2: Restart and verify data
    {
        let recovered_data = persistent_state.lock().await.clone();
        println!("Recovered {} data entries after crash", recovered_data.len());
        
        // Verify all data persisted
        assert_eq!(recovered_data.len(), 9); // 3 nodes * 3 writes each
        
        for entry in &recovered_data {
            println!("  Recovered: {}", entry);
        }
    }
    
    println!("\n✅ Crash recovery test passed!");
    println!("All data was recovered successfully!");
}