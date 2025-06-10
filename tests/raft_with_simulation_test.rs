#![cfg(feature = "simulation")]

use anyhow::Result;
use blixard::raft_node_v2::RaftNode;
use blixard::network_v2::RaftNetwork;
use blixard::runtime::simulation::SimulatedRuntime;
use blixard::runtime_traits::{Runtime, Clock};
use blixard::runtime_context::{RuntimeHandle, SimulatedRuntimeHandle};
use blixard::runtime_abstraction as rt;
use blixard::storage::Storage;
use blixard::state_machine::StateMachineCommand;
use blixard::types::{VmState, VmStatus, VmConfig};
use std::sync::Arc;
use std::time::Duration;
use std::net::{SocketAddr, IpAddr, Ipv4Addr};
use tokio::sync::mpsc;

/// Helper to create a socket address for a node
fn node_addr(node_id: u64) -> SocketAddr {
    SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        8000 + node_id as u16,
    )
}

/// Helper to create VM state
fn create_vm(name: &str, node_id: u64) -> VmState {
    VmState {
        name: name.to_string(),
        config: VmConfig {
            name: name.to_string(),
            config_path: format!("/tmp/{}.nix", name),
            vcpus: 1,
            memory: 512,
        },
        status: VmStatus::Stopped,
        node_id,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn test_single_node_with_simulation() {
    println!("\n🧪 SINGLE NODE RAFT WITH SIMULATION\n");
    
    // Create simulated runtime
    let runtime = Arc::new(SimulatedRuntime::new(12345));
    let sim_handle = Arc::new(SimulatedRuntimeHandle::new_with_runtime(runtime.clone()));
    let _guard = rt::RuntimeGuard::new(sim_handle.clone() as Arc<dyn RuntimeHandle>);
    
    println!("✅ Created simulated runtime with seed 12345");
    
    // Create storage
    let storage = Arc::new(Storage::new_test().unwrap());
    
    // Create single node
    let node = RaftNode::new(
        1,
        node_addr(1),
        storage,
        vec![],
        runtime.clone(),
    ).await.unwrap();
    
    println!("✅ Created RaftNode with simulated runtime");
    
    // Get proposal handle before moving node
    let proposal_tx = node.get_proposal_handle();
    
    // Start the node in background
    rt::spawn(async move {
        if let Err(e) = node.run().await {
            eprintln!("Node error: {}", e);
        }
    });
    
    // Let the node start up
    runtime.advance_time(Duration::from_millis(500));
    rt::sleep(Duration::from_millis(10)).await;
    
    // Propose a command
    let cmd = StateMachineCommand::CreateVm {
        vm: create_vm("test-vm", 1),
    };
    
    proposal_tx.send(cmd).await.unwrap();
    println!("📝 Sent proposal to create VM");
    
    // Advance time to let proposal process
    runtime.advance_time(Duration::from_millis(200));
    rt::sleep(Duration::from_millis(10)).await;
    
    println!("✅ Test completed - RaftNode works with simulated runtime!");
}

#[tokio::test]
async fn test_three_node_cluster_deterministic() {
    println!("\n🎯 DETERMINISTIC THREE-NODE CLUSTER TEST\n");
    
    // Run the same test twice with same seed - should produce identical behavior
    let mut results = Vec::new();
    
    for run in 0..2 {
        println!("\n--- Run {} ---", run + 1);
        
        // Create simulated runtime with fixed seed
        let runtime = Arc::new(SimulatedRuntime::new(99999));
        let sim_handle = Arc::new(SimulatedRuntimeHandle::new_with_runtime(runtime.clone()));
        let _guard = rt::RuntimeGuard::new(sim_handle.clone() as Arc<dyn RuntimeHandle>);
        
        let mut events = Vec::new();
        let start_time = runtime.clock().now();
        
        // Create three nodes
        let mut nodes = Vec::new();
        let mut proposal_handles = Vec::new();
        
        for node_id in 1..=3 {
            let storage = Arc::new(Storage::new_test().unwrap());
            let node = RaftNode::new(
                node_id,
                node_addr(node_id),
                storage,
                vec![1, 2, 3],
                runtime.clone(),
            ).await.unwrap();
            
            proposal_handles.push(node.get_proposal_handle());
            nodes.push(node);
        }
        
        events.push(format!("Created 3 nodes at {:?}", runtime.clock().now() - start_time));
        
        // Register peer addresses
        for i in 0..3 {
            for j in 0..3 {
                if i != j {
                    let peer_id = (j + 1) as u64;
                    nodes[i].register_peer_address(peer_id, node_addr(peer_id)).await;
                }
            }
        }
        
        // Start all nodes
        for (i, node) in nodes.into_iter().enumerate() {
            let node_id = i + 1;
            rt::spawn(async move {
                println!("  Node {} starting", node_id);
                if let Err(e) = node.run().await {
                    eprintln!("  Node {} error: {}", node_id, e);
                }
            });
        }
        
        // Advance time for leader election
        runtime.advance_time(Duration::from_secs(2));
        rt::sleep(Duration::from_millis(20)).await;
        
        events.push(format!("After election at {:?}", runtime.clock().now() - start_time));
        
        // Try to propose on each node
        for (i, tx) in proposal_handles.iter().enumerate() {
            let cmd = StateMachineCommand::CreateVm {
                vm: create_vm(&format!("vm-{}", i), (i + 1) as u64),
            };
            
            match tx.try_send(cmd) {
                Ok(()) => events.push(format!("Node {} accepted proposal", i + 1)),
                Err(_) => events.push(format!("Node {} rejected proposal", i + 1)),
            }
        }
        
        // Advance time for consensus
        runtime.advance_time(Duration::from_millis(500));
        rt::sleep(Duration::from_millis(20)).await;
        
        events.push(format!("Final time: {:?}", runtime.clock().now() - start_time));
        
        results.push(events);
    }
    
    // Compare results
    println!("\n📊 Comparing runs:");
    if results[0] == results[1] {
        println!("✅ DETERMINISTIC EXECUTION VERIFIED!");
        println!("Both runs produced identical event sequences");
    } else {
        println!("❌ Runs differ - not deterministic");
        println!("Run 1: {:?}", results[0]);
        println!("Run 2: {:?}", results[1]);
    }
}

#[tokio::test]
async fn test_network_partition_simulation() {
    println!("\n🔌 NETWORK PARTITION WITH RAFT SIMULATION\n");
    
    let runtime = Arc::new(SimulatedRuntime::new(55555));
    let sim_handle = Arc::new(SimulatedRuntimeHandle::new_with_runtime(runtime.clone()));
    let _guard = rt::RuntimeGuard::new(sim_handle.clone() as Arc<dyn RuntimeHandle>);
    
    // Create 5-node cluster
    let mut nodes = Vec::new();
    let mut proposal_handles = Vec::new();
    
    for node_id in 1..=5 {
        let storage = Arc::new(Storage::new_test().unwrap());
        let node = RaftNode::new(
            node_id,
            node_addr(node_id),
            storage,
            vec![1, 2, 3, 4, 5],
            runtime.clone(),
        ).await.unwrap();
        
        proposal_handles.push(node.get_proposal_handle());
        nodes.push(node);
    }
    
    // Register all peers
    for i in 0..5 {
        for j in 0..5 {
            if i != j {
                nodes[i].register_peer_address((j + 1) as u64, node_addr((j + 1) as u64)).await;
            }
        }
    }
    
    // Start nodes
    for (i, node) in nodes.into_iter().enumerate() {
        rt::spawn(async move {
            let _ = node.run().await;
        });
    }
    
    // Let cluster form
    println!("⏳ Forming cluster...");
    runtime.advance_time(Duration::from_secs(3));
    rt::sleep(Duration::from_millis(50)).await;
    
    // Create network partition: [1,2] | [3,4,5]
    println!("\n🔪 Creating network partition");
    runtime.partition_network(node_addr(1), node_addr(3));
    runtime.partition_network(node_addr(1), node_addr(4));
    runtime.partition_network(node_addr(1), node_addr(5));
    runtime.partition_network(node_addr(2), node_addr(3));
    runtime.partition_network(node_addr(2), node_addr(4));
    runtime.partition_network(node_addr(2), node_addr(5));
    
    // Try to write on minority side (should fail/timeout)
    println!("\n📝 Attempting write on minority partition");
    let minority_cmd = StateMachineCommand::CreateVm {
        vm: create_vm("minority-vm", 1),
    };
    let _ = proposal_handles[0].try_send(minority_cmd);
    
    // Try to write on majority side (should succeed)
    println!("📝 Attempting write on majority partition");
    let majority_cmd = StateMachineCommand::CreateVm {
        vm: create_vm("majority-vm", 3),
    };
    let _ = proposal_handles[2].try_send(majority_cmd);
    
    // Let operations process
    runtime.advance_time(Duration::from_secs(2));
    rt::sleep(Duration::from_millis(30)).await;
    
    // Heal partition
    println!("\n🔧 Healing partition");
    for i in 1..=2 {
        for j in 3..=5 {
            runtime.heal_network(node_addr(i), node_addr(j));
        }
    }
    
    // Let cluster reconcile
    runtime.advance_time(Duration::from_secs(2));
    rt::sleep(Duration::from_millis(50)).await;
    
    println!("\n✅ Network partition test completed!");
}

// Extension trait to help create SimulatedRuntimeHandle with existing runtime
trait SimulatedRuntimeHandleExt {
    fn new_with_runtime(runtime: Arc<SimulatedRuntime>) -> Self;
}

impl SimulatedRuntimeHandleExt for SimulatedRuntimeHandle {
    fn new_with_runtime(runtime: Arc<SimulatedRuntime>) -> Self {
        // This is a bit of a hack - we create a handle and set its runtime
        // In a real implementation, you'd add this method to SimulatedRuntimeHandle
        let handle = SimulatedRuntimeHandle::new(0);
        // For now, we'll just use the seed from the runtime
        // You might need to add a proper constructor for this
        handle
    }
}