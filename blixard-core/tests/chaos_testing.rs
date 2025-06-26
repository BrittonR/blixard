//! Chaos testing with random node failures
//! 
//! These tests simulate unpredictable failures during cluster operations
//! to ensure the system maintains consistency and recovers gracefully.
#![cfg(feature = "test-helpers")]

use blixard_core::{
    test_helpers_concurrent::{ConcurrentTestCluster, ConcurrentTestClusterBuilder},
    proto::{ClusterStatusRequest, CreateVmRequest},
    types::{VmConfig, VmCommand},
};
use std::time::{Duration, Instant};
use std::sync::Arc;
use tokio::sync::Mutex;
use rand::Rng;

/// Chaos agent that randomly kills nodes during operations
struct ChaosAgent {
    cluster: ConcurrentTestCluster,
    kill_probability: f64,
    min_alive_nodes: usize,
}

impl ChaosAgent {
    fn new(cluster: ConcurrentTestCluster, kill_probability: f64, min_alive_nodes: usize) -> Self {
        Self {
            cluster,
            kill_probability,
            min_alive_nodes,
        }
    }
    
    async fn maybe_kill_random_node(&self) -> Option<u64> {
        // Generate random values before any await
        let kill_roll = rand::thread_rng().gen::<f64>();
        
        if kill_roll > self.kill_probability {
            return None;
        }
        
        let node_ids = self.cluster.node_ids().await;
        
        // Ensure we keep minimum alive nodes
        if node_ids.len() <= self.min_alive_nodes {
            return None;
        }
        
        // Pick a random node to kill
        let victim_index = rand::thread_rng().gen_range(0..node_ids.len());
        let victim_id = node_ids[victim_index];
        
        // Stop the node
        match self.cluster.stop_node(victim_id).await {
            Ok(_) => {
                println!("🔥 Chaos: Killed node {}", victim_id);
                Some(victim_id)
            }
            Err(e) => {
                println!("⚠️  Chaos: Failed to kill node {}: {:?}", victim_id, e);
                None
            }
        }
    }
    
    async fn run_chaos(&self, duration: Duration) {
        let start = Instant::now();
        let mut killed_nodes = Vec::new();
        
        while start.elapsed() < duration {
            if let Some(node_id) = self.maybe_kill_random_node().await {
                killed_nodes.push(node_id);
            }
            
            // Random sleep between chaos events
            let sleep_ms = rand::thread_rng().gen_range(100..1000);
            tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
        }
        
        println!("🔥 Chaos agent finished. Killed {} nodes total", killed_nodes.len());
    }
}

#[tokio::test]
async fn test_node_joins_with_random_failures() {
    let _ = blixard_core::metrics_otel::init_noop();
    
    println!("Testing node joins with random failures...");
    
    // Start with a 3-node cluster
    let cluster = ConcurrentTestClusterBuilder::new()
        .with_nodes(3)
        .build()
        .await
        .expect("Failed to create initial cluster");
    
    // Create chaos agent
    let chaos_agent = ChaosAgent::new(
        cluster.clone(),
        0.2,  // 20% chance to kill a node
        2,    // Keep at least 2 nodes alive
    );
    
    // Start chaos in background
    let chaos_handle = tokio::spawn(async move {
        chaos_agent.run_chaos(Duration::from_secs(10)).await;
    });
    
    // Try to add nodes while chaos is happening
    let mut join_results = Vec::new();
    
    for i in 0..5 {
        println!("\n📥 Attempting to add node {}...", i + 4);
        let start = Instant::now();
        
        match cluster.add_node().await {
            Ok(node_id) => {
                let duration = start.elapsed();
                println!("✅ Node {} joined successfully in {:?}", node_id, duration);
                join_results.push((node_id, true, duration));
            }
            Err(e) => {
                let duration = start.elapsed();
                println!("❌ Failed to add node after {:?}: {:?}", duration, e);
                join_results.push((0, false, duration));
            }
        }
        
        // Random delay between joins
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    
    // Stop chaos
    chaos_handle.abort();
    
    // Wait for cluster to stabilize
    println!("\n⏳ Waiting for cluster to stabilize...");
    tokio::time::sleep(Duration::from_secs(3)).await;
    
    // Check cluster health
    match cluster.leader_client().await {
        Ok(mut leader) => {
            let status = leader
                .get_cluster_status(ClusterStatusRequest {})
                .await
                .expect("Failed to get status")
                .into_inner();
            
            println!("\n📊 Final cluster state:");
            println!("  Leader: Node {}", status.leader_id);
            println!("  Total nodes: {}", status.nodes.len());
            println!("  Node IDs: {:?}", 
                status.nodes.iter().map(|n| n.id).collect::<Vec<_>>());
            
            // At least some nodes should have survived
            assert!(status.nodes.len() >= 2, 
                "Cluster should have at least 2 surviving nodes");
        }
        Err(e) => {
            panic!("No leader after chaos: {:?}", e);
        }
    }
    
    // Analyze results
    let successful_joins = join_results.iter().filter(|(_, success, _)| *success).count();
    println!("\n📈 Join attempts during chaos: {}/{} successful", 
        successful_joins, join_results.len());
    
    cluster.shutdown().await;
}

#[tokio::test]
async fn test_leader_failures_during_operations() {
    let _ = blixard_core::metrics_otel::init_noop();
    
    println!("Testing operations with leader failures...");
    
    // Create a 5-node cluster
    let cluster = ConcurrentTestClusterBuilder::new()
        .with_nodes(5)
        .build()
        .await
        .expect("Failed to create cluster");
    
    // Track operations
    let operations = Arc::new(Mutex::new(Vec::new()));
    let operations_clone = operations.clone();
    
    // Start operations in background
    let ops_handle = {
        let cluster_clone = cluster.clone();
        tokio::spawn(async move {
            for i in 0..20 {
                // Try to create a VM
                let vm_config = VmConfig {
                    name: format!("chaos-vm-{}", i),
                    config_path: format!("/tmp/chaos-{}.nix", i),
                    vcpus: 1,
                    memory: 256,
                };
                
                let start = Instant::now();
                let result = match cluster_clone.leader_client().await {
                    Ok(mut leader) => {
                        let request = CreateVmRequest {
                            name: vm_config.name.clone(),
                            config_path: vm_config.config_path,
                            vcpus: vm_config.vcpus,
                            memory_mb: vm_config.memory,
                        };
                        
                        leader.create_vm(request).await.map(|_| ()).map_err(|e| 
                            blixard_core::error::BlixardError::Internal { message: e.to_string() }
                        )
                    }
                    Err(e) => Err(e),
                };
                let duration = start.elapsed();
                
                let mut ops = operations_clone.lock().await;
                ops.push((i, result.is_ok(), duration));
                
                if result.is_ok() {
                    println!("✅ Operation {} succeeded in {:?}", i, duration);
                } else {
                    println!("❌ Operation {} failed in {:?}", i, duration);
                }
                
                // Small delay between operations
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        })
    };
    
    // Wait a bit for operations to start
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Kill the leader a few times
    for round in 0..3 {
        if let Ok(mut leader_client) = cluster.leader_client().await {
            // Find current leader ID
            let status = leader_client
                .get_cluster_status(ClusterStatusRequest {})
                .await
                .expect("Failed to get status")
                .into_inner();
            
            let leader_id = status.leader_id;
            println!("\n🎯 Round {}: Killing leader node {}", round + 1, leader_id);
            
            // Kill the leader
            if let Err(e) = cluster.stop_node(leader_id).await {
                println!("⚠️  Failed to kill leader: {:?}", e);
            } else {
                println!("💥 Leader node {} killed", leader_id);
            }
            
            // Wait for new leader election
            tokio::time::sleep(Duration::from_secs(2)).await;
            
            // Verify new leader
            if let Ok(mut new_leader) = cluster.leader_client().await {
                let new_status = new_leader
                    .get_cluster_status(ClusterStatusRequest {})
                    .await
                    .expect("Failed to get status")
                    .into_inner();
                
                println!("👑 New leader elected: Node {}", new_status.leader_id);
                assert_ne!(new_status.leader_id, leader_id, 
                    "Should have elected a different leader");
            }
        }
        
        // Wait between leader kills
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    
    // Wait for operations to complete
    let _ = ops_handle.await;
    
    // Analyze results
    let ops = operations.lock().await;
    let successful = ops.iter().filter(|(_, success, _)| *success).count();
    let total = ops.len();
    
    println!("\n📊 Operation results during leader chaos:");
    println!("  Total operations: {}", total);
    println!("  Successful: {} ({:.1}%)", 
        successful, (successful as f64 / total as f64) * 100.0);
    
    // Despite leader failures, most operations should eventually succeed
    assert!(successful as f64 / total as f64 > 0.5, 
        "At least 50% of operations should succeed despite leader failures");
    
    cluster.shutdown().await;
}

#[tokio::test]
async fn test_random_network_delays_and_failures() {
    let _ = blixard_core::metrics_otel::init_noop();
    
    println!("Testing with random network delays (simulated)...");
    
    // Note: This test simulates network issues by adding random delays
    // Real network partition testing would require message interception
    
    let cluster = ConcurrentTestClusterBuilder::new()
        .with_nodes(4)
        .build()
        .await
        .expect("Failed to create cluster");
    
    // Simulate network delays by randomly delaying operations
    let mut handles = Vec::new();
    
    for i in 0..10 {
        let cluster_clone = cluster.clone();
        let handle = tokio::spawn(async move {
            // Random delay to simulate network latency
            let delay_ms = rand::thread_rng().gen_range(0..500);
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            
            let vm_config = VmConfig {
                name: format!("delay-test-vm-{}", i),
                config_path: format!("/tmp/delay-{}.nix", i),
                vcpus: 1,
                memory: 128,
            };
            
            // Pick a random node to send the request to
            let node_ids = cluster_clone.node_ids().await;
            let target_node = node_ids[rand::thread_rng().gen_range(0..node_ids.len())];
            
            let vm_command = VmCommand::Create {
                config: vm_config,
                node_id: target_node,
            };
            
            match cluster_clone.get_node_shared_state(target_node).await {
                Ok(shared_state) => {
                    // Add another random delay
                    let delay_ms = rand::thread_rng().gen_range(0..200);
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    
                    shared_state.create_vm_through_raft(vm_command).await
                }
                Err(e) => Err(e),
            }
        });
        
        handles.push(handle);
    }
    
    // Wait for all operations
    let mut successes = 0;
    let mut failures = 0;
    
    for (i, handle) in handles.into_iter().enumerate() {
        match handle.await {
            Ok(Ok(_)) => {
                println!("✅ Operation {} succeeded despite delays", i);
                successes += 1;
            }
            Ok(Err(e)) => {
                println!("❌ Operation {} failed: {:?}", i, e);
                failures += 1;
            }
            Err(e) => {
                println!("💥 Operation {} panicked: {:?}", i, e);
                failures += 1;
            }
        }
    }
    
    println!("\n📊 Results with network delays: {} successes, {} failures",
        successes, failures);
    
    // Most operations should still succeed
    assert!(successes >= 7, "At least 70% should succeed despite delays");
    
    cluster.shutdown().await;
}

#[tokio::test]
async fn test_cascading_failures() {
    let _ = blixard_core::metrics_otel::init_noop();
    
    println!("Testing cascading node failures...");
    
    // Start with a 7-node cluster
    let cluster = ConcurrentTestClusterBuilder::new()
        .with_nodes(7)
        .build()
        .await
        .expect("Failed to create cluster");
    
    // Get initial state
    let initial_nodes = cluster.node_ids().await;
    println!("Initial cluster: {} nodes", initial_nodes.len());
    
    // Kill nodes in waves
    let waves = vec![
        vec![initial_nodes[0], initial_nodes[1]],  // Kill 2 nodes
        vec![initial_nodes[2]],                      // Kill 1 more
        vec![initial_nodes[3]],                      // Kill 1 more (now at edge of quorum)
    ];
    
    for (wave_num, victims) in waves.iter().enumerate() {
        println!("\n🌊 Wave {}: Killing nodes {:?}", wave_num + 1, victims);
        
        for &victim in victims {
            match cluster.stop_node(victim).await {
                Ok(_) => println!("  💥 Killed node {}", victim),
                Err(e) => println!("  ⚠️  Failed to kill node {}: {:?}", victim, e),
            }
        }
        
        // Wait for cluster to react
        tokio::time::sleep(Duration::from_secs(2)).await;
        
        // Check if cluster is still functional
        match cluster.leader_client().await {
            Ok(mut leader) => {
                let status = leader
                    .get_cluster_status(ClusterStatusRequest {})
                    .await
                    .expect("Failed to get status")
                    .into_inner();
                
                println!("  ✅ Cluster still has leader: Node {}", status.leader_id);
                println!("  📊 Remaining nodes: {}", status.nodes.len());
                
                // Try an operation
                let vm_config = VmConfig {
                    name: format!("wave-{}-test", wave_num),
                    config_path: format!("/tmp/wave-{}.nix", wave_num),
                    vcpus: 1,
                    memory: 128,
                };
                
                let request = CreateVmRequest {
                    name: vm_config.name.clone(),
                    config_path: vm_config.config_path,
                    vcpus: vm_config.vcpus,
                    memory_mb: vm_config.memory,
                };
                
                match leader.create_vm(request).await {
                    Ok(_) => println!("  ✅ Cluster can still process operations"),
                    Err(e) => println!("  ❌ Cluster cannot process operations: {:?}", e),
                }
            }
            Err(e) => {
                println!("  ❌ No leader available: {:?}", e);
                if wave_num < 2 {
                    panic!("Cluster failed too early at wave {}", wave_num + 1);
                }
            }
        }
    }
    
    // Final check - cluster should still be minimally functional
    let remaining_nodes = cluster.node_ids().await;
    println!("\n📊 Final state: {} nodes remaining", remaining_nodes.len());
    
    // Should have at least 3 nodes (quorum from 7)
    assert!(remaining_nodes.len() >= 3, 
        "Should maintain quorum with at least 3 nodes");
    
    cluster.shutdown().await;
}

/// Document chaos testing patterns and requirements
#[test]
fn document_chaos_testing_patterns() {
    println!("\nChaos Testing Patterns:");
    println!("====================");
    println!();
    println!("1. Random Node Failures:");
    println!("   - Kill nodes at random intervals");
    println!("   - Maintain minimum viable cluster size");
    println!("   - Verify cluster recovers and maintains consistency");
    println!();
    println!("2. Leader Targeting:");
    println!("   - Specifically kill leader nodes");
    println!("   - Verify leader election completes");
    println!("   - Operations should retry and eventually succeed");
    println!();
    println!("3. Network Chaos (Simulated):");
    println!("   - Add random delays to operations");
    println!("   - Simulate slow/unreliable networks");
    println!("   - Real network partitions need message interception");
    println!();
    println!("4. Cascading Failures:");
    println!("   - Kill multiple nodes in waves");
    println!("   - Test edge of quorum conditions");
    println!("   - Verify graceful degradation");
    println!();
    println!("5. Recovery Testing:");
    println!("   - Bring killed nodes back online");
    println!("   - Verify state reconciliation");
    println!("   - Check data consistency after recovery");
    println!();
    println!("Future Enhancements:");
    println!("- Integrate with failure injection frameworks");
    println!("- Add Byzantine failure simulation");
    println!("- Implement proper network partition testing");
    println!("- Add resource exhaustion scenarios");
    println!("- Test with clock skew and time jumps");
}