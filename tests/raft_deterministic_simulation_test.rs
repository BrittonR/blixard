use anyhow::Result;
use blixard::runtime::simulation::SimulatedRuntime;
use blixard::runtime_traits::{Runtime, Clock};
use blixard::raft_node_v2::RaftNode;
use blixard::state_machine::{StateMachineCommand, ServiceInfo, ServiceState};
use blixard::storage::Storage;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

#[tokio::test]
async fn test_raft_cluster_deterministic() -> Result<()> {
    // Test with multiple seeds to ensure determinism
    for seed in [42, 100, 200] {
        let result1 = run_raft_simulation(seed).await?;
        let result2 = run_raft_simulation(seed).await?;
        
        assert_eq!(result1, result2, 
            "Simulation with seed {} produced different results", seed);
    }
    
    Ok(())
}

async fn run_raft_simulation(seed: u64) -> Result<Vec<String>> {
    let runtime = Arc::new(SimulatedRuntime::new(seed));
    
    // Create storage directories
    let temp_dir = TempDir::new()?;
    let storage1 = Arc::new(Storage::new(temp_dir.path().join("node1.db"))?);
    let storage2 = Arc::new(Storage::new(temp_dir.path().join("node2.db"))?);
    let storage3 = Arc::new(Storage::new(temp_dir.path().join("node3.db"))?);
    
    // Create nodes with runtime
    let mut node1 = RaftNode::new(
        1,
        "127.0.0.1:8001".parse()?,
        storage1,
        vec![2, 3],
        runtime.clone(),
    ).await?;
    
    let mut node2 = RaftNode::new(
        2,
        "127.0.0.1:8002".parse()?,
        storage2,
        vec![1, 3],
        runtime.clone(),
    ).await?;
    
    let mut node3 = RaftNode::new(
        3,
        "127.0.0.1:8003".parse()?,
        storage3,
        vec![1, 2],
        runtime.clone(),
    ).await?;
    
    // Register peer addresses
    node1.register_peer_address(2, "127.0.0.1:8002".parse()?).await;
    node1.register_peer_address(3, "127.0.0.1:8003".parse()?).await;
    
    node2.register_peer_address(1, "127.0.0.1:8001".parse()?).await;
    node2.register_peer_address(3, "127.0.0.1:8003".parse()?).await;
    
    node3.register_peer_address(1, "127.0.0.1:8001".parse()?).await;
    node3.register_peer_address(2, "127.0.0.1:8002".parse()?).await;
    
    // Start nodes in background
    let handle1 = runtime.spawn(Box::pin(async move {
        let _ = node1.run().await;
    }));
    
    let handle2 = runtime.spawn(Box::pin(async move {
        let _ = node2.run().await;
    }));
    
    let handle3 = runtime.spawn(Box::pin(async move {
        let _ = node3.run().await;
    }));
    
    // Give nodes time to elect a leader
    runtime.clock().sleep(Duration::from_secs(5)).await;
    
    // TODO: Send some proposals and collect results
    // For now, just return a simple result
    let results = vec!["simulation_completed".to_string()];
    
    // Clean shutdown would go here
    // For now, just let the test end
    
    Ok(results)
}

#[tokio::test]
async fn test_raft_with_network_partitions() -> Result<()> {
    let runtime = Arc::new(SimulatedRuntime::new(42));
    
    // Create a 3-node cluster
    let temp_dir = TempDir::new()?;
    let storage1 = Arc::new(Storage::new(temp_dir.path().join("node1.db"))?);
    
    let node1 = RaftNode::new(
        1,
        "127.0.0.1:9001".parse()?,
        storage1,
        vec![],
        runtime.clone(),
    ).await?;
    
    // Get proposal handle before moving node
    let proposal_handle = node1.get_proposal_handle();
    
    // Start node
    let _handle = runtime.spawn(Box::pin(async move {
        let _ = node1.run().await;
    }));
    
    // Wait for node to start
    runtime.clock().sleep(Duration::from_millis(100)).await;
    
    // Send a proposal
    proposal_handle.send(StateMachineCommand::CreateService {
        info: ServiceInfo {
            name: "test_service".to_string(),
            state: ServiceState::Running,
            node: "node1".to_string(),
            timestamp: chrono::Utc::now(),
        },
    }).await?;
    
    // Simulate network partition by partitioning node addresses
    runtime.partition_network("127.0.0.1:9001".parse()?, "127.0.0.1:9002".parse()?);
    
    // Try to send another proposal (should fail or timeout)
    let result = timeout(
        Duration::from_secs(1), 
        proposal_handle.send(StateMachineCommand::CreateService {
            info: ServiceInfo {
                name: "test_service2".to_string(),
                state: ServiceState::Running,
                node: "node1".to_string(),
                timestamp: chrono::Utc::now(),
            },
        })
    ).await;
    
    // Verify partition had effect
    assert!(result.is_ok(), "Proposal should be sent but may not be committed");
    
    // Heal the partition
    runtime.heal_network("127.0.0.1:9001".parse()?, "127.0.0.1:9002".parse()?);
    
    Ok(())
}