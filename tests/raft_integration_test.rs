use anyhow::Result;
use blixard::storage::Storage;
use blixard::raft_node_v2::RaftNode;
use blixard::runtime::simulation::SimulatedRuntime;
use blixard::runtime_traits::{Runtime, Clock};
use blixard::state_machine::{StateMachineCommand, ServiceInfo, ServiceState};
use std::sync::Arc;
use std::net::SocketAddr;
use std::time::Duration;
use tempfile::TempDir;
use serial_test::serial;

mod common;
use common::get_available_port;

#[tokio::test]
#[serial]
async fn test_single_node_raft_cluster() -> Result<()> {
    // Use simulated runtime for deterministic testing
    let runtime = Arc::new(SimulatedRuntime::new(42));
    
    // Create temporary storage
    let temp_dir = TempDir::new()?;
    let storage = Arc::new(Storage::new(temp_dir.path().join("node1.db"))?);
    
    // Create a single-node Raft cluster with dynamic port
    let port = get_available_port();
    let bind_addr: SocketAddr = format!("127.0.0.1:{}", port).parse()?;
    let raft_node = RaftNode::new(1, bind_addr, storage.clone(), vec![], runtime.clone()).await?;
    let proposal_handle = raft_node.get_proposal_handle();
    
    // Start Raft node in background
    let raft_handle = runtime.spawn(Box::pin(async move {
        raft_node.run().await
    }));
    
    // Give it time to elect itself as leader
    runtime.clock().sleep(Duration::from_secs(2)).await;
    
    // Propose a command to create a service
    let create_service = StateMachineCommand::CreateService {
        info: ServiceInfo {
            name: "test-service".to_string(),
            state: ServiceState::Running,
            node: "node1".to_string(),
            timestamp: chrono::Utc::now(),
        },
    };
    
    proposal_handle.send(create_service).await?;
    
    // Give it time to process
    runtime.clock().sleep(Duration::from_secs(1)).await;
    
    // Check if service was created in storage
    let service = storage.get_service("test-service")?;
    assert!(service.is_some());
    assert_eq!(service.unwrap().state, ServiceState::Running);
    
    // Clean up
    raft_handle.abort();
    
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_three_node_raft_cluster() -> Result<()> {
    // Use simulated runtime for deterministic testing
    let runtime = Arc::new(SimulatedRuntime::new(42));
    
    // Create three nodes with separate storage
    let temp_dir = TempDir::new()?;
    let mut nodes = Vec::new();
    let mut raft_nodes = Vec::new();
    
    // Get ports for all nodes
    let ports: Vec<u16> = (0..3).map(|_| get_available_port()).collect();
    
    // First create all nodes
    for i in 0..3 {
        let node_id = (i + 1) as u64;
        let storage = Arc::new(Storage::new(temp_dir.path().join(format!("node{}.db", node_id)))?);
        let bind_addr: SocketAddr = format!("127.0.0.1:{}", ports[i]).parse()?;
        let peers = vec![1, 2, 3]; // All nodes know about each other
        
        let mut node = RaftNode::new(node_id, bind_addr, storage.clone(), peers, runtime.clone()).await?;
        
        // Register all peer addresses
        for j in 0..3 {
            let peer_id = (j + 1) as u64;
            if peer_id != node_id {
                let peer_addr: SocketAddr = format!("127.0.0.1:{}", ports[j]).parse()?;
                node.register_peer_address(peer_id, peer_addr).await;
            }
        }
        
        raft_nodes.push((storage, node));
    }
    
    // Now start all nodes
    let mut proposal_handles = Vec::new();
    for (storage, node) in raft_nodes {
        proposal_handles.push(node.get_proposal_handle());
        
        // Start each node in background
        let handle = runtime.spawn(Box::pin(async move {
            node.run().await
        }));
        
        nodes.push((storage, handle));
    }
    
    // Give time for cluster formation
    runtime.clock().sleep(Duration::from_secs(3)).await;
    
    // Propose a command through node 1 (should be leader)
    let create_service = StateMachineCommand::CreateService {
        info: ServiceInfo {
            name: "distributed-service".to_string(),
            state: ServiceState::Running,
            node: "node1".to_string(),
            timestamp: chrono::Utc::now(),
        },
    };
    
    proposal_handles[0].send(create_service).await?;
    
    // Give time for replication
    runtime.clock().sleep(Duration::from_secs(2)).await;
    
    // Check that all nodes have the service
    for (i, (storage, _)) in nodes.iter().enumerate() {
        let service = storage.get_service("distributed-service")?;
        println!("Node {} service lookup result: {:?}", i + 1, service);
        assert!(service.is_some(), "Node {} should have the service", i + 1);
        assert_eq!(service.unwrap().state, ServiceState::Running);
    }
    
    // Clean up
    for (_, handle) in nodes {
        handle.abort();
    }
    
    Ok(())
}

#[test]
fn test_storage_persistence() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");
    
    // Create and write data
    {
        let storage = Storage::new(&db_path)?;
        let service = ServiceInfo {
            name: "persistent-service".to_string(),
            state: ServiceState::Running,
            node: "node1".to_string(),
            timestamp: chrono::Utc::now(),
        };
        storage.store_service(&service)?;
    }
    
    // Reopen and verify data persisted
    {
        let storage = Storage::new(&db_path)?;
        let service = storage.get_service("persistent-service")?;
        assert!(service.is_some());
        assert_eq!(service.unwrap().name, "persistent-service");
    }
    
    Ok(())
}