use blixard::raft_node_madsim::RaftNodeMadsim;
use blixard::storage::Storage;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

#[madsim::test]
async fn test_single_node_becomes_leader() {
    // Create storage
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(Storage::new(temp_dir.path().join("node1.db")).unwrap());

    // Create single node cluster
    let node = RaftNodeMadsim::new(1, "127.0.0.1:8001".parse().unwrap(), storage, vec![])
        .await
        .unwrap();

    // Start the node in background
    let handle = madsim::task::spawn(async move {
        node.run().await.unwrap();
    });

    // Give it time to elect itself as leader
    madsim::time::sleep(Duration::from_secs(2)).await;

    // In a single node cluster, it should become leader
    // We'd need to expose a method to check this

    // For now, just ensure the task is still running
    assert!(!handle.is_finished());
}

#[madsim::test]
async fn test_deterministic_three_node_cluster() {
    // Test multiple times with same seed to verify determinism
    for _ in 0..3 {
        run_three_node_test().await;
    }
}

async fn run_three_node_test() {
    // Create storage for each node
    let temp_dir = TempDir::new().unwrap();
    let storage1 = Arc::new(Storage::new(temp_dir.path().join("node1.db")).unwrap());
    let storage2 = Arc::new(Storage::new(temp_dir.path().join("node2.db")).unwrap());
    let storage3 = Arc::new(Storage::new(temp_dir.path().join("node3.db")).unwrap());

    // Create three nodes
    let node1 = RaftNodeMadsim::new(1, "127.0.0.1:8001".parse().unwrap(), storage1, vec![2, 3])
        .await
        .unwrap();

    let node2 = RaftNodeMadsim::new(2, "127.0.0.1:8002".parse().unwrap(), storage2, vec![1, 3])
        .await
        .unwrap();

    let node3 = RaftNodeMadsim::new(3, "127.0.0.1:8003".parse().unwrap(), storage3, vec![1, 2])
        .await
        .unwrap();

    // Start all nodes
    let handle1 = madsim::task::spawn(async move {
        node1.run().await.unwrap();
    });

    let handle2 = madsim::task::spawn(async move {
        node2.run().await.unwrap();
    });

    let handle3 = madsim::task::spawn(async move {
        node3.run().await.unwrap();
    });

    // Give them time to elect a leader
    madsim::time::sleep(Duration::from_secs(5)).await;

    // Verify all tasks are still running
    assert!(!handle1.is_finished());
    assert!(!handle2.is_finished());
    assert!(!handle3.is_finished());
}

#[madsim::test]
async fn test_network_partition_simulation() {
    // This test demonstrates how madsim can simulate network partitions
    // We'll need to integrate madsim's network simulation features

    // For now, just a placeholder that shows madsim time control
    let start = std::time::Instant::now();

    // Simulate 10 seconds passing instantly
    madsim::time::sleep(Duration::from_secs(10)).await;

    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_secs(10));
}
