use anyhow::Result;
use blixard::runtime::simulation::SimulatedRuntime;
use blixard::runtime_traits::{Runtime, Clock};
use blixard::raft_node_v2::RaftNode;
use blixard::storage::Storage;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test]
async fn test_raft_node_with_simulation() -> Result<()> {
    let runtime = Arc::new(SimulatedRuntime::new(42));
    
    // Create storage
    let temp_dir = TempDir::new()?;
    let storage = Arc::new(Storage::new(temp_dir.path().join("node1.db"))?);
    
    // Create a single node
    let node = RaftNode::new(
        1,
        "127.0.0.1:8001".parse()?,
        storage,
        vec![],
        runtime.clone(),
    ).await?;
    
    // Verify node was created
    assert!(!node.is_leader()); // Should not be leader yet
    
    Ok(())
}

#[tokio::test]
async fn test_deterministic_clock() -> Result<()> {
    // Test that the simulated clock is deterministic
    let runtime1 = Arc::new(SimulatedRuntime::new(42));
    let runtime2 = Arc::new(SimulatedRuntime::new(42));
    
    let time1_before = runtime1.clock().now();
    let time2_before = runtime2.clock().now();
    
    // Advance both clocks
    runtime1.clock().sleep(Duration::from_secs(1)).await;
    runtime2.clock().sleep(Duration::from_secs(1)).await;
    
    let time1_after = runtime1.clock().now();
    let time2_after = runtime2.clock().now();
    
    // Times should advance by the same amount
    let delta1 = time1_after.duration_since(time1_before);
    let delta2 = time2_after.duration_since(time2_before);
    
    assert_eq!(delta1, delta2, "Clock advancement should be deterministic");
    
    Ok(())
}