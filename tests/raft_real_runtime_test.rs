use anyhow::Result;
use blixard::storage::Storage;
use blixard::raft_node_v2::RaftNode;
use blixard::runtime_traits::RealRuntime;
use blixard::state_machine::{StateMachineCommand, ServiceInfo, ServiceState};
use std::sync::Arc;
use std::net::SocketAddr;
use tempfile::TempDir;
use tokio::time::{sleep, Duration};
use serial_test::serial;

mod common;
use common::get_available_port;

#[tokio::test]
#[serial]
async fn test_single_node_raft_with_real_runtime() -> Result<()> {
    // Use real runtime (not simulation)
    let runtime = Arc::new(RealRuntime::new());
    
    // Create temporary storage
    let temp_dir = TempDir::new()?;
    let storage = Arc::new(Storage::new(temp_dir.path().join("node1.db"))?);
    
    // Create a single-node Raft cluster with dynamic port
    let port = get_available_port();
    let bind_addr: SocketAddr = format!("127.0.0.1:{}", port).parse()?;
    let raft_node = RaftNode::new(1, bind_addr, storage.clone(), vec![], runtime).await?;
    let proposal_handle = raft_node.get_proposal_handle();
    
    // Start Raft node in background
    let raft_handle = tokio::spawn(async move {
        raft_node.run().await
    });
    
    // Give it time to elect itself as leader
    sleep(Duration::from_secs(2)).await;
    
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
    sleep(Duration::from_secs(1)).await;
    
    // Check if service was created in storage
    let service = storage.get_service("test-service")?;
    assert!(service.is_some(), "Service should be created");
    assert_eq!(service.unwrap().name, "test-service");
    
    // Clean up
    raft_handle.abort();
    
    Ok(())
}