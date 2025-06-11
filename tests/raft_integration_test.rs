use anyhow::Result;
use blixard::raft_node_v2::RaftNode;
use blixard::state_machine::{ServiceInfo, ServiceState, StateMachineCommand};
use blixard::storage::Storage;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test]
async fn test_single_node_raft_cluster() -> Result<()> {
    use blixard::runtime_traits::RealRuntime;

    // Use real runtime for integration tests
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("node1.db");
    let storage = Arc::new(Storage::new(&db_path)?);
    let runtime = Arc::new(RealRuntime::new());

    // Create a single-node Raft cluster
    let bind_addr: SocketAddr = "127.0.0.1:0".parse()?; // Use port 0 for dynamic allocation
    let raft_node = RaftNode::new(1, bind_addr, storage.clone(), vec![], runtime).await?;
    let proposal_handle = raft_node.get_proposal_handle();

    // Start the node in background
    let raft_handle = tokio::spawn(async move {
        if let Err(e) = raft_node.run().await {
            eprintln!("Node error: {}", e);
        }
    });

    // Give it time to become leader (single node should become leader quickly)
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Propose a command
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
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Check if service was created in storage
    let service = storage.get_service("test-service")?;
    assert!(service.is_some());
    assert_eq!(service.unwrap().state, ServiceState::Running);

    // Clean up
    raft_handle.abort();

    Ok(())
}

#[tokio::test]
async fn test_three_node_raft_cluster() -> Result<()> {
    use blixard::runtime_traits::RealRuntime;
    use tempfile::TempDir;

    // Use real runtime for integration tests
    let runtime = Arc::new(RealRuntime::new());
    let temp_dir = TempDir::new()?;

    // Create three nodes with separate storage
    let mut handles = Vec::new();
    let mut raft_nodes = Vec::new();

    // Use fixed ports for testing
    let base_port = 50000;

    // First create all nodes
    for i in 0..3 {
        let node_id = (i + 1) as u64;
        let db_path = temp_dir.path().join(format!("node{}.db", node_id));
        let storage = Arc::new(Storage::new(&db_path)?);
        let bind_addr: SocketAddr = format!("127.0.0.1:{}", base_port + i).parse()?;
        let peers = vec![1, 2, 3]; // All nodes know about each other

        let mut node =
            RaftNode::new(node_id, bind_addr, storage.clone(), peers, runtime.clone()).await?;

        // Register all peer addresses
        for j in 0..3 {
            let peer_id = (j + 1) as u64;
            if peer_id != node_id {
                let peer_addr: SocketAddr = format!("127.0.0.1:{}", base_port + j).parse()?;
                node.register_peer_address(peer_id, peer_addr).await;
            }
        }

        raft_nodes.push((storage, node));
    }

    // Now start all nodes
    let mut proposal_handles = Vec::new();
    let mut storages = Vec::new();

    for (storage, node) in raft_nodes {
        proposal_handles.push(node.get_proposal_handle());
        storages.push(storage);

        // Start each node in background
        let handle = tokio::spawn(async move {
            if let Err(e) = node.run().await {
                eprintln!("Node error: {}", e);
            }
        });

        handles.push(handle);
    }

    // Give time for cluster formation
    tokio::time::sleep(Duration::from_secs(2)).await;

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
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Check that all nodes have the service
    for (i, storage) in storages.iter().enumerate() {
        let service = storage.get_service("distributed-service")?;
        println!("Node {} service lookup result: {:?}", i + 1, service);
        assert!(service.is_some(), "Node {} should have the service", i + 1);
        assert_eq!(service.unwrap().state, ServiceState::Running);
    }

    // Clean up
    for handle in handles {
        handle.abort();
    }

    Ok(())
}

// This test requires real file I/O and can't run with simulation
// #[test]
// fn test_storage_persistence() -> Result<()> {
//     let temp_dir = TempDir::new()?;
//     let db_path = temp_dir.path().join("test.db");
//
//     // Create and write data
//     {
//         let storage = Storage::new(&db_path)?;
//         let service = ServiceInfo {
//             name: "persistent-service".to_string(),
//             state: ServiceState::Running,
//             node: "node1".to_string(),
//             timestamp: chrono::Utc::now(),
//         };
//         storage.store_service(&service)?;
//     }
//
//     // Reopen and verify data persisted
//     {
//         let storage = Storage::new(&db_path)?;
//         let service = storage.get_service("persistent-service")?;
//         assert!(service.is_some());
//         assert_eq!(service.unwrap().name, "persistent-service");
//     }
//
//     Ok(())
// }
