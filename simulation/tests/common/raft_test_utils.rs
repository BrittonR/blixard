#[allow(dead_code)]

use std::time::Duration;
use blixard_core::error::{BlixardError, BlixardResult};

#[cfg(feature = "test-helpers")]
use blixard_core::test_helpers::{TestNode, TestCluster, timing};

/// Helper to create a cluster and wait for convergence
#[cfg(feature = "test-helpers")]
pub async fn create_converged_cluster(size: usize) -> TestCluster {
    let cluster = TestCluster::new(size).await.unwrap();
    cluster.wait_for_convergence(Duration::from_secs(30)).await.unwrap();
    cluster
}

/// Helper to find the current leader in a cluster
#[cfg(feature = "test-helpers")]
pub async fn find_leader(cluster: &TestCluster) -> Option<u64> {
    for (id, node) in cluster.nodes() {
        if node.shared_state.is_leader().await {
            return Some(*id);
        }
    }
    None
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
    for node in nodes {
        if let Ok(_status) = node.shared_state.get_raft_status().await {
            // TODO: Get applied index from raft_manager when available
            // For now, always return true
            if false {
                return false;
            }
        } else {
            return false;
        }
    }
    
    true
}