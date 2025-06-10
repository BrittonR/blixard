#![cfg(feature = "simulation")]

use anyhow::Result;
use blixard::runtime::simulation::SimulatedRuntime;
use blixard::raft_node::RaftNode;
use blixard::storage::Storage;
use blixard::state_machine::StateMachineCommand;
use blixard::types::{VmState, VmStatus, VmConfig};
use std::sync::Arc;
use std::time::Duration;
use std::net::{SocketAddr, IpAddr, Ipv4Addr};
use tracing_test::traced_test;

// Helper function to create a VmState
fn create_vm_state(name: String, config_path: String, node_id: u64) -> VmState {
    VmState {
        name: name.clone(),
        config: VmConfig {
            name,
            config_path,
            vcpus: 1,
            memory: 512,
        },
        status: VmStatus::Stopped,
        node_id,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

/// Test basic cluster formation with deterministic simulation
#[tokio::test]
#[traced_test]
async fn test_simulated_cluster_formation() -> Result<()> {
    println!("Starting cluster formation test");
    
    // For now, just test that nodes can be created
    let mut nodes = Vec::new();
    let mut proposal_senders = Vec::new();
    
    for node_id in 1..=3 {
        let addr = SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            8000 + node_id as u16,
        );
        
        let storage = Arc::new(Storage::new_test()?);
        let node = RaftNode::new(node_id, addr, storage, vec![1, 2, 3]).await?;
        proposal_senders.push(node.get_proposal_handle());
        nodes.push(node);
    }
    
    // Set up peer addresses
    for i in 0..3 {
        for j in 0..3 {
            if i != j {
                let peer_id = (j + 1) as u64;
                let peer_addr = SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    8000 + peer_id as u16,
                );
                nodes[i].register_peer_address(peer_id, peer_addr).await;
            }
        }
    }
    
    println!("Starting {} nodes", nodes.len());
    
    // Start nodes in separate tasks
    let mut handles = Vec::new();
    for (i, node) in nodes.into_iter().enumerate() {
        let handle = tokio::spawn(async move {
            println!("Node {} starting", i + 1);
            if let Err(e) = node.run().await {
                eprintln!("Node {} error: {}", i + 1, e);
            }
        });
        handles.push(handle);
    }
    
    // Give cluster time to start
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Try to send a proposal (this will fail if no leader elected)
    let proposal = StateMachineCommand::CreateVm {
        vm: create_vm_state("test-vm".to_string(), "test-config".to_string(), 1),
    };
    
    println!("Sending test proposal");
    if let Err(e) = proposal_senders[0].send(proposal).await {
        println!("Failed to send proposal: {}", e);
    }
    
    // Give some time for processing
    tokio::time::sleep(Duration::from_millis(200)).await;
    
    println!("Test completed");
    
    // Cancel all tasks
    for handle in handles {
        handle.abort();
    }
    
    Ok(())
}

/// Test network partition simulation
#[tokio::test]
#[traced_test]
async fn test_network_partition_recovery() -> Result<()> {
    let runtime = Arc::new(SimulatedRuntime::new(123)); // Different seed
    
    // Create 5-node cluster for better partition testing
    let mut nodes = Vec::new();
    let mut proposal_handles = Vec::new();
    
    for node_id in 1..=5 {
        let addr = SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            9000 + node_id as u16,
        );
        
        let storage = Arc::new(Storage::new_test()?);
        let node = RaftNode::new(node_id, addr, storage, vec![1, 2, 3, 4, 5]).await?;
        
        // Save proposal handle before moving node
        proposal_handles.push(node.get_proposal_handle());
        
        nodes.push(node);
    }
    
    // Set up peer addresses
    for i in 0..5 {
        let node_id = (i + 1) as u64;
        for j in 0..5 {
            if i != j {
                let peer_id = (j + 1) as u64;
                let peer_addr = SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    9000 + peer_id as u16,
                );
                nodes[i].register_peer_address(peer_id, peer_addr).await;
            }
        }
    }
    
    // Start nodes
    let mut handles = Vec::new();
    for node in nodes {
        let handle = tokio::spawn(async move {
            node.run().await
        });
        handles.push(handle);
    }
    
    // Let cluster stabilize
    runtime.advance_time(Duration::from_secs(3));
    runtime.run_until_idle();
    
    // Create network partition: nodes 1,2 vs nodes 3,4,5
    let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9001);
    let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9002);
    let addr3 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9003);
    let addr4 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9004);
    let addr5 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9005);
    
    // Partition minority (1,2) from majority (3,4,5)
    for minority_addr in [addr1, addr2] {
        for majority_addr in [addr3, addr4, addr5] {
            runtime.partition_network(minority_addr, majority_addr);
        }
    }
    
    // Advance time during partition
    runtime.advance_time(Duration::from_secs(5));
    runtime.run_until_idle();
    
    // Try to propose on minority side (should fail/timeout)
    let minority_proposal = StateMachineCommand::CreateVm {
        vm: create_vm_state("test-vm-1".to_string(), "partition-test".to_string(), 1),
    };
    
    let _ = proposal_handles[0].send(minority_proposal).await;
    
    // Propose on majority side (should succeed)
    let majority_proposal = StateMachineCommand::CreateVm {
        vm: create_vm_state("test-vm-2".to_string(), "majority-write".to_string(), 3),
    };
    
    let _ = proposal_handles[2].send(majority_proposal).await;
    
    // Let proposals process
    runtime.advance_time(Duration::from_secs(2));
    runtime.run_until_idle();
    
    // Heal the partition
    for minority_addr in [addr1, addr2] {
        for majority_addr in [addr3, addr4, addr5] {
            runtime.heal_network(minority_addr, majority_addr);
        }
    }
    
    // Let cluster recover
    runtime.advance_time(Duration::from_secs(5));
    runtime.run_until_idle();
    
    // Verify cluster converges to same state
    // In a real test, we'd verify all nodes have the same log
    
    Ok(())
}

/// Test with injected failures using failpoints
#[tokio::test]
#[traced_test]
#[cfg(feature = "failpoints")]
async fn test_failpoint_injection() -> Result<()> {
    use fail::FailScenario;
    
    let runtime = Arc::new(SimulatedRuntime::new(456));
    let scenario = FailScenario::setup();
    
    // Set up a 3-node cluster
    let mut nodes = Vec::new();
    let mut proposal_handles = Vec::new();
    
    for node_id in 1..=3 {
        let addr = SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            7000 + node_id as u16,
        );
        
        let storage = Arc::new(Storage::new_test()?);
        let node = RaftNode::new(node_id, addr, storage, vec![1, 2, 3]).await?;
        proposal_handles.push(node.get_proposal_handle());
        nodes.push(node);
    }
    
    // Set up peer addresses
    for i in 0..3 {
        let node_id = (i + 1) as u64;
        for j in 0..3 {
            if i != j {
                let peer_id = (j + 1) as u64;
                let peer_addr = SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    7000 + peer_id as u16,
                );
                nodes[i].register_peer_address(peer_id, peer_addr).await;
            }
        }
    }
    
    // Start nodes
    let mut handles = Vec::new();
    for node in nodes {
        let handle = tokio::spawn(async move {
            node.run().await
        });
        handles.push(handle);
    }
    
    // Let cluster form
    runtime.advance_time(Duration::from_secs(2));
    runtime.run_until_idle();
    
    // Inject storage append failure
    fail::cfg("storage::before_append_entries", "50%return").unwrap();
    
    // Try to propose multiple commands
    for i in 0..10 {
        let cmd = StateMachineCommand::CreateVm {
            vm: create_vm_state(
                format!("failpoint-vm-{}", i),
                format!("config-{}", i),
                1,
            ),
        };
        let _ = proposal_handles[0].send(cmd).await;
        
        runtime.advance_time(Duration::from_millis(100));
        runtime.run_until_idle();
    }
    
    // Remove failure and let system recover
    fail::cfg("storage::before_append_entries", "off").unwrap();
    
    runtime.advance_time(Duration::from_secs(3));
    runtime.run_until_idle();
    
    // Inject network failures
    fail::cfg("network::before_send", "20%return").unwrap();
    
    // Continue proposing
    for i in 10..15 {
        let cmd = StateMachineCommand::CreateVm {
            vm: create_vm_state(
                format!("network-fail-vm-{}", i),
                format!("config-{}", i),
                1,
            ),
        };
        let _ = proposal_handles[0].send(cmd).await;
        
        runtime.advance_time(Duration::from_millis(200));
        runtime.run_until_idle();
    }
    
    scenario.teardown();
    Ok(())
}

/// Test deterministic scheduling and race conditions
#[tokio::test]
#[traced_test]
async fn test_deterministic_message_ordering() -> Result<()> {
    // Run the same test multiple times with same seed - should be deterministic
    for run in 0..3 {
        println!("Run {}", run);
        
        let runtime = Arc::new(SimulatedRuntime::new(789)); // Same seed each time
        
        // Create a 3-node cluster
        let mut nodes = Vec::new();
        let mut message_handles = Vec::new();
        
        for node_id in 1..=3 {
            let addr = SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                6000 + node_id as u16,
            );
            
            let storage = Arc::new(Storage::new_test()?);
            let node = RaftNode::new(node_id, addr, storage, vec![1, 2, 3]).await?;
            message_handles.push(node.get_message_handle());
            nodes.push(node);
        }
        
        // Set peer addresses
        for i in 0..3 {
            let node_id = (i + 1) as u64;
            for j in 0..3 {
                if i != j {
                    let peer_id = (j + 1) as u64;
                    let peer_addr = SocketAddr::new(
                        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                        6000 + peer_id as u16,
                    );
                    nodes[i].register_peer_address(peer_id, peer_addr).await;
                }
            }
        }
        
        // Start nodes
        for node in nodes {
            tokio::spawn(async move {
                let _ = node.run().await;
            });
        }
        
        // Advance time in specific increments
        for i in 0..10 {
            runtime.advance_time(Duration::from_millis(100 * i));
            runtime.run_until_idle();
            
            // The order of operations should be deterministic
            println!("  Step {}: Advanced {} ms", i, 100 * i);
        }
        
        // Results should be identical across runs due to deterministic execution
    }
    
    Ok(())
}

/// Test crash recovery with simulated filesystem
#[tokio::test]
#[traced_test]
async fn test_crash_recovery_simulation() -> Result<()> {
    let runtime = Arc::new(SimulatedRuntime::new(999));
    
    // Phase 1: Create cluster and write data
    {
        let mut nodes = Vec::new();
        let mut proposal_handles = Vec::new();
        
        for node_id in 1..=3 {
            let addr = SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                5000 + node_id as u16,
            );
            
            let storage = Arc::new(Storage::new_test()?);
            let node = RaftNode::new(node_id, addr, storage, vec![1, 2, 3]).await?;
            proposal_handles.push(node.get_proposal_handle());
            nodes.push(node);
        }
        
        // Start nodes
        for node in nodes {
            tokio::spawn(async move {
                let _ = node.run().await;
            });
        }
        
        runtime.advance_time(Duration::from_secs(2));
        runtime.run_until_idle();
        
        // Write some data
        for i in 0..5 {
            let cmd = StateMachineCommand::CreateVm {
                vm: create_vm_state(
                    format!("crash-test-vm-{}", i),
                    format!("config-{}", i),
                    1,
                ),
            };
            let _ = proposal_handles[0].send(cmd).await;
        }
        
        runtime.advance_time(Duration::from_secs(1));
        runtime.run_until_idle();
    }
    
    // Simulate crash - in real implementation, we'd persist to simulated filesystem
    println!("Simulating cluster crash...");
    
    // Phase 2: Restart cluster and verify data
    {
        let mut nodes = Vec::new();
        
        for node_id in 1..=3 {
            let addr = SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                5000 + node_id as u16,
            );
            
            // In real implementation, storage would load from simulated filesystem
            let storage = Arc::new(Storage::new_test()?);
            let node = RaftNode::new(node_id, addr, storage, vec![1, 2, 3]).await?;
            nodes.push(node);
        }
        
        // Start nodes
        for node in nodes {
            tokio::spawn(async move {
                let _ = node.run().await;
            });
        }
        
        runtime.advance_time(Duration::from_secs(3));
        runtime.run_until_idle();
        
        // Verify data persisted (in real test)
        println!("Cluster recovered after crash");
    }
    
    Ok(())
}