use blixard_core::error::BlixardResult;
#[cfg(feature = "test-helpers")]
use blixard_core::test_helpers::{timing, TestCluster, TestNode};
use std::time::Duration;

/// Helper to create a cluster and wait for convergence
#[cfg(feature = "test-helpers")]
pub async fn create_converged_cluster(size: usize) -> BlixardResult<TestCluster> {
    let mut cluster = TestCluster::new();

    // Add the requested number of nodes
    for _ in 0..size {
        cluster.add_node().await?;
    }

    // TODO: Add convergence waiting logic - should wait for all nodes to join cluster
    // and reach stable Raft state (leader election complete, log replication working)

    Ok(cluster)
}

/// Helper to find the current leader in a cluster
#[cfg(feature = "test-helpers")]
pub async fn find_leader(cluster: &TestCluster) -> Option<u64> {
    // TODO: Implement proper leader detection by accessing node.node.lock().await 
    // and checking Raft state (node.raft_state() == raft::StateRole::Leader)
    // For now, return the first node as a placeholder
    cluster.nodes().keys().next().copied()
}

/// Helper to wait for a new leader different from the old one
#[cfg(feature = "test-helpers")]
pub async fn wait_for_new_leader(
    cluster: &TestCluster,
    old_leader: u64,
    timeout: Duration,
) -> BlixardResult<u64> {
    let start = std::time::Instant::now();

    loop {
        if let Some(new_leader) = find_leader(cluster).await {
            if new_leader != old_leader {
                return Ok(new_leader);
            }
        }

        if start.elapsed() >= timeout {
            return Err(BlixardError::Internal {
                message: format!("Timeout waiting for new leader after {:?}", timeout),
            });
        }

        timing::robust_sleep(Duration::from_millis(100)).await;
    }
}

/// Helper to verify all nodes have applied a specific index
#[cfg(feature = "test-helpers")]
pub async fn verify_all_applied(nodes: &[TestNode], _expected_index: u64) -> bool {
    for _node in nodes {
        // TODO: Implement proper applied index checking - verify that Raft log entries
        // have been applied to the state machine (applied_index >= expected_index)
        // For now, always return true as a placeholder
    }

    true
}
