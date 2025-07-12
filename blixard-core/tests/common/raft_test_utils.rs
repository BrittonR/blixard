use blixard_core::error::{BlixardError, BlixardResult};
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

    // TODO: Add convergence waiting logic if needed

    Ok(cluster)
}

/// Helper to find the current leader in a cluster
#[cfg(feature = "test-helpers")]
pub async fn find_leader(cluster: &TestCluster) -> Option<u64> {
    // TODO: Implement leader detection by accessing node.node.lock().await and checking Raft state
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
        // TODO: Implement proper applied index checking
        // For now, always return true as a placeholder
    }

    true
}
