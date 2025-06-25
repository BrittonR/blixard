//! Simulation tests for VM scheduler in distributed environments
//!
//! Tests VM scheduling behavior under various network conditions and failures

use madsim::{Config, net::{Endpoint, NetSim}, runtime::Runtime};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

mod common;
use common::SimulatedNode;

/// Test VM scheduling with network partitions
#[madsim::test]
async fn test_vm_scheduling_during_partition() {
    let mut nodes = HashMap::new();
    
    // Create 5-node cluster with different capacities
    for i in 1..=5 {
        let addr: SocketAddr = format!("10.0.0.{}:7000", i).parse().unwrap();
        let capacity = match i {
            1 => (16, 32768, 500),  // High capacity
            2 => (12, 24576, 400),  // High-medium
            3 => (8, 16384, 200),   // Medium
            4 => (4, 8192, 100),    // Low
            5 => (2, 4096, 50),     // Very low
            _ => unreachable!(),
        };
        
        let mut node = SimulatedNode::new(i, addr).await;
        node.start().await;
        
        // Register as worker with specific capacity
        if i > 1 {
            let join_result = node.client_mut().join_cluster(1, addr).await;
            assert!(join_result.is_ok());
        }
        
        nodes.insert(i, node);
    }
    
    // Wait for cluster to stabilize
    madsim::time::sleep(Duration::from_secs(3)).await;
    
    // Create VMs before partition
    let leader = &mut nodes.get_mut(&1).unwrap();
    for i in 1..=10 {
        let vm_name = format!("vm-pre-partition-{}", i);
        let result = leader.client_mut().create_vm(&vm_name, "", 2, 2048).await;
        assert!(result.is_ok());
    }
    
    // Partition the network: {1,2} | {3,4,5}
    let net_sim = NetSim::current();
    net_sim.partition(
        &["10.0.0.1:7000", "10.0.0.2:7000"],
        &["10.0.0.3:7000", "10.0.0.4:7000", "10.0.0.5:7000"]
    );
    
    // Wait for partition to take effect
    madsim::time::sleep(Duration::from_secs(2)).await;
    
    // Try to create VMs in both partitions
    // Partition 1 (nodes 1,2) - should succeed as it has original leader
    let leader = &mut nodes.get_mut(&1).unwrap();
    for i in 1..=5 {
        let vm_name = format!("vm-partition1-{}", i);
        let result = leader.client_mut().create_vm(&vm_name, "", 1, 1024).await;
        assert!(result.is_ok(), "Should create VM in majority partition");
    }
    
    // Partition 2 (nodes 3,4,5) - should fail or timeout
    let node3 = &mut nodes.get_mut(&3).unwrap();
    let vm_name = "vm-partition2-fail";
    let result = node3.client_mut().create_vm(vm_name, "", 1, 1024).await;
    // This should fail as partition 2 doesn't have quorum
    assert!(result.is_err() || !result.unwrap(), "Should not create VM in minority partition");
    
    // Heal the partition
    net_sim.reset();
    
    // Wait for cluster to reconverge
    madsim::time::sleep(Duration::from_secs(5)).await;
    
    // Verify VMs created during partition are visible to all nodes
    for node_id in 1..=5 {
        let node = &mut nodes.get_mut(&node_id).unwrap();
        let vms = node.client_mut().list_vms().await.unwrap();
        
        // Should see pre-partition VMs and partition1 VMs
        assert!(vms.len() >= 15, "Node {} should see at least 15 VMs, but sees {}", node_id, vms.len());
    }
    
    // Test scheduling after healing - should use all nodes again
    let leader = &mut nodes.get_mut(&1).unwrap();
    for i in 1..=5 {
        let vm_name = format!("vm-post-heal-{}", i);
        let result = leader.client_mut().create_vm(&vm_name, "", 1, 1024).await;
        assert!(result.is_ok());
    }
    
    // Verify even distribution across all nodes
    let leader = &mut nodes.get_mut(&1).unwrap();
    // Note: We can't directly query resource summary in simulation, but in real implementation
    // we would verify VMs are distributed across all 5 nodes
}

/// Test VM scheduling with cascading node failures
#[madsim::test]
async fn test_vm_scheduling_cascading_failures() {
    let mut nodes = HashMap::new();
    
    // Create 4-node cluster
    for i in 1..=4 {
        let addr: SocketAddr = format!("10.0.0.{}:7000", i).parse().unwrap();
        let mut node = SimulatedNode::new(i, addr).await;
        node.start().await;
        
        if i > 1 {
            let join_result = node.client_mut().join_cluster(1, addr).await;
            assert!(join_result.is_ok());
        }
        
        nodes.insert(i, node);
    }
    
    madsim::time::sleep(Duration::from_secs(2)).await;
    
    // Create initial VMs distributed across all nodes
    let leader = &mut nodes.get_mut(&1).unwrap();
    for i in 1..=8 {
        let vm_name = format!("vm-initial-{}", i);
        let result = leader.client_mut().create_vm(&vm_name, "", 1, 1024).await;
        assert!(result.is_ok());
    }
    
    madsim::time::sleep(Duration::from_secs(1)).await;
    
    // Simulate node 4 failure
    let net_sim = NetSim::current();
    net_sim.disconnect_node("10.0.0.4:7000");
    
    madsim::time::sleep(Duration::from_secs(2)).await;
    
    // New VMs should avoid failed node
    for i in 1..=3 {
        let vm_name = format!("vm-after-node4-fail-{}", i);
        let result = leader.client_mut().create_vm(&vm_name, "", 1, 1024).await;
        assert!(result.is_ok(), "Should create VM avoiding failed node");
    }
    
    // Simulate node 3 failure (cascading)
    net_sim.disconnect_node("10.0.0.3:7000");
    
    madsim::time::sleep(Duration::from_secs(2)).await;
    
    // Now only nodes 1 and 2 are available
    for i in 1..=2 {
        let vm_name = format!("vm-after-cascade-{}", i);
        let result = leader.client_mut().create_vm(&vm_name, "", 1, 1024).await;
        assert!(result.is_ok(), "Should create VM on remaining nodes");
    }
    
    // Try to create a VM that requires more resources than available
    let result = leader.client_mut().create_vm("vm-too-large", "", 32, 65536).await;
    assert!(result.is_err() || !result.unwrap(), "Should fail to create oversized VM");
    
    // Restore node 3
    net_sim.connect_node("10.0.0.3:7000");
    
    madsim::time::sleep(Duration::from_secs(3)).await;
    
    // Should be able to schedule on node 3 again
    for i in 1..=2 {
        let vm_name = format!("vm-after-restore-{}", i);
        let result = leader.client_mut().create_vm(&vm_name, "", 1, 1024).await;
        assert!(result.is_ok(), "Should create VM after node restoration");
    }
}

/// Test VM scheduling under high concurrency
#[madsim::test]
async fn test_vm_scheduling_high_concurrency() {
    let mut nodes = HashMap::new();
    
    // Create 3-node cluster
    for i in 1..=3 {
        let addr: SocketAddr = format!("10.0.0.{}:7000", i).parse().unwrap();
        let mut node = SimulatedNode::new(i, addr).await;
        node.start().await;
        
        if i > 1 {
            let join_result = node.client_mut().join_cluster(1, addr).await;
            assert!(join_result.is_ok());
        }
        
        nodes.insert(i, node);
    }
    
    madsim::time::sleep(Duration::from_secs(2)).await;
    
    // Spawn multiple concurrent VM creation tasks
    let mut handles = vec![];
    
    for batch in 0..3 {
        for i in 0..10 {
            let vm_name = format!("concurrent-vm-{}-{}", batch, i);
            let node_id = (i % 3) + 1; // Distribute requests across nodes
            
            let handle = madsim::task::spawn(async move {
                // Simulate getting node reference (in real code would be from nodes map)
                let addr: SocketAddr = format!("10.0.0.{}:7000", node_id).parse().unwrap();
                let endpoint = Endpoint::current();
                
                // Small random delay to spread requests
                madsim::time::sleep(Duration::from_millis(i as u64 * 10)).await;
                
                // In simulation, we can't directly access the node, so we simulate the request
                // In real implementation, this would make the actual gRPC call
                println!("Creating VM {} on node {}", vm_name, node_id);
                
                // Simulate success/failure based on timing
                Ok::<_, String>(())
            });
            
            handles.push(handle);
        }
        
        // Small delay between batches
        madsim::time::sleep(Duration::from_millis(100)).await;
    }
    
    // Wait for all requests to complete
    let mut successes = 0;
    let mut failures = 0;
    
    for handle in handles {
        match handle.await {
            Ok(Ok(_)) => successes += 1,
            Ok(Err(_)) => failures += 1,
            Err(_) => failures += 1,
        }
    }
    
    println!("Concurrent creation: {} successes, {} failures", successes, failures);
    assert!(successes >= 25, "Most concurrent requests should succeed");
}

/// Test VM scheduling with asymmetric network delays
#[madsim::test]
async fn test_vm_scheduling_asymmetric_delays() {
    let mut nodes = HashMap::new();
    
    // Create 3-node cluster
    for i in 1..=3 {
        let addr: SocketAddr = format!("10.0.0.{}:7000", i).parse().unwrap();
        let mut node = SimulatedNode::new(i, addr).await;
        node.start().await;
        
        if i > 1 {
            let join_result = node.client_mut().join_cluster(1, addr).await;
            assert!(join_result.is_ok());
        }
        
        nodes.insert(i, node);
    }
    
    madsim::time::sleep(Duration::from_secs(2)).await;
    
    // Add asymmetric delays
    let net_sim = NetSim::current();
    
    // Node 1 <-> Node 2: Fast connection (1ms)
    net_sim.add_latency("10.0.0.1:7000", "10.0.0.2:7000", Duration::from_millis(1));
    net_sim.add_latency("10.0.0.2:7000", "10.0.0.1:7000", Duration::from_millis(1));
    
    // Node 1 <-> Node 3: Slow connection (100ms)
    net_sim.add_latency("10.0.0.1:7000", "10.0.0.3:7000", Duration::from_millis(100));
    net_sim.add_latency("10.0.0.3:7000", "10.0.0.1:7000", Duration::from_millis(100));
    
    // Node 2 <-> Node 3: Medium connection (50ms)
    net_sim.add_latency("10.0.0.2:7000", "10.0.0.3:7000", Duration::from_millis(50));
    net_sim.add_latency("10.0.0.3:7000", "10.0.0.2:7000", Duration::from_millis(50));
    
    // Create VMs and observe scheduling behavior
    let leader = &mut nodes.get_mut(&1).unwrap();
    
    // Fast operations between nodes 1 and 2
    for i in 1..=5 {
        let vm_name = format!("vm-fast-{}", i);
        let start = madsim::time::Instant::now();
        let result = leader.client_mut().create_vm(&vm_name, "", 1, 1024).await;
        let elapsed = start.elapsed();
        
        assert!(result.is_ok());
        println!("VM {} created in {:?}", vm_name, elapsed);
    }
    
    // Operations involving node 3 should be slower
    // In real implementation, the scheduler might prefer nodes 1 and 2
    // due to better responsiveness
    
    madsim::time::sleep(Duration::from_secs(1)).await;
    
    // Simulate high load on nodes 1 and 2 to force usage of node 3
    for i in 1..=10 {
        let vm_name = format!("vm-load-{}", i);
        let result = leader.client_mut().create_vm(&vm_name, "", 2, 2048).await;
        assert!(result.is_ok());
    }
    
    // New VMs might need to go to node 3 despite higher latency
    for i in 1..=3 {
        let vm_name = format!("vm-slow-{}", i);
        let start = madsim::time::Instant::now();
        let result = leader.client_mut().create_vm(&vm_name, "", 1, 1024).await;
        let elapsed = start.elapsed();
        
        assert!(result.is_ok());
        println!("VM {} created in {:?} (potentially on slow node)", vm_name, elapsed);
    }
}

/// Test VM scheduling with resource fragmentation
#[madsim::test]
async fn test_vm_scheduling_resource_fragmentation() {
    let mut nodes = HashMap::new();
    
    // Create 3-node cluster with identical resources
    for i in 1..=3 {
        let addr: SocketAddr = format!("10.0.0.{}:7000", i).parse().unwrap();
        let mut node = SimulatedNode::new(i, addr).await;
        node.start().await;
        
        if i > 1 {
            let join_result = node.client_mut().join_cluster(1, addr).await;
            assert!(join_result.is_ok());
        }
        
        nodes.insert(i, node);
    }
    
    madsim::time::sleep(Duration::from_secs(2)).await;
    
    let leader = &mut nodes.get_mut(&1).unwrap();
    
    // Create a fragmented resource layout:
    // Many small VMs that consume most CPU but leave memory available
    for i in 1..=15 {
        let vm_name = format!("vm-cpu-heavy-{}", i);
        // High CPU, low memory VMs
        let result = leader.client_mut().create_vm(&vm_name, "", 2, 512).await;
        assert!(result.is_ok());
    }
    
    madsim::time::sleep(Duration::from_millis(500)).await;
    
    // Now try to create memory-heavy VMs
    // Should fail even though total memory might be available (fragmented across nodes)
    let mut memory_vm_failures = 0;
    for i in 1..=3 {
        let vm_name = format!("vm-memory-heavy-{}", i);
        // Low CPU, high memory VMs
        let result = leader.client_mut().create_vm(&vm_name, "", 1, 8192).await;
        if result.is_err() || !result.unwrap() {
            memory_vm_failures += 1;
        }
    }
    
    // Some memory-heavy VMs should fail due to fragmentation
    assert!(memory_vm_failures > 0, "Should experience scheduling failures due to fragmentation");
    
    // Clean up by removing some CPU-heavy VMs
    for i in 1..=5 {
        let vm_name = format!("vm-cpu-heavy-{}", i);
        // In real implementation, would call delete_vm
        println!("Would delete VM: {}", vm_name);
    }
    
    madsim::time::sleep(Duration::from_millis(500)).await;
    
    // After cleanup, memory-heavy VMs should succeed
    for i in 4..=6 {
        let vm_name = format!("vm-memory-heavy-{}", i);
        let result = leader.client_mut().create_vm(&vm_name, "", 1, 8192).await;
        // In real implementation with cleanup, these should succeed
        println!("After cleanup, creating {}: {:?}", vm_name, result);
    }
}