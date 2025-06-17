use std::time::Duration;

use blixard::{
    test_helpers::{TestCluster, TestNode, timing},
    proto::{
        LeaveRequest, ClusterStatusRequest,
        HealthCheckRequest,
    },
};

#[tokio::test]
async fn test_single_node_health_check() {
    // Use the new TestNode builder
    let node = TestNode::builder()
        .with_id(1)
        .with_auto_port()
        .build()
        .await
        .expect("Failed to create node");
    
    // Get client with built-in retry
    let mut client = node.client().await.expect("Failed to get client");
    
    // Health check
    let response = client.health_check(HealthCheckRequest {})
        .await
        .expect("Health check failed");
    
    assert!(response.into_inner().healthy);
    
    // Clean shutdown
    node.shutdown().await;
}

#[tokio::test]
async fn test_two_node_cluster_formation() {
    // Create first node
    let node1 = TestNode::builder()
        .with_id(1)
        .with_auto_port()
        .build()
        .await
        .expect("Failed to create node 1");
    
    let node1_addr = node1.addr;
    
    // Create second node that will join the first
    let node2 = TestNode::builder()
        .with_id(2)
        .with_auto_port()
        .with_join_addr(Some(node1_addr))
        .build()
        .await
        .expect("Failed to create node 2");
    
    // Send join request
    node2.node.send_join_request()
        .await
        .expect("Failed to send join request");
    
    // Wait for cluster convergence using timing utilities
    timing::wait_for_condition_with_backoff(
        || async {
            // Check both nodes see each other
            let status1 = node1.shared_state.get_cluster_status().await;
            let status2 = node2.shared_state.get_cluster_status().await;
            
            if let (Ok((_, nodes1, _)), Ok((_, nodes2, _))) = (status1, status2) {
                nodes1.len() == 2 && nodes2.len() == 2
            } else {
                false
            }
        },
        Duration::from_secs(10),
        Duration::from_millis(100),
    )
    .await
    .expect("Cluster failed to converge");
    
    // Verify both nodes are healthy
    let mut client1 = node1.client().await.expect("Failed to get client 1");
    let mut client2 = node2.client().await.expect("Failed to get client 2");
    
    assert!(client1.health_check(HealthCheckRequest {}).await.unwrap().into_inner().healthy);
    assert!(client2.health_check(HealthCheckRequest {}).await.unwrap().into_inner().healthy);
    
    // Clean shutdown
    node2.shutdown().await;
    node1.shutdown().await;
}

#[tokio::test]
#[ignore = "Three-node clusters have known issues"]
async fn test_three_node_cluster_with_helper() {
    // Use the high-level TestCluster abstraction
    let mut cluster = TestCluster::builder()
        .with_nodes(3)
        .with_convergence_timeout(Duration::from_secs(30))
        .build()
        .await
        .expect("Failed to create cluster");
    
    // Cluster builder waits for convergence automatically
    
    // Get leader client
    let mut leader_client = cluster.leader_client()
        .await
        .expect("Failed to get leader client");
    
    // Verify leader is healthy
    let health = leader_client.health_check(HealthCheckRequest {})
        .await
        .expect("Health check failed");
    assert!(health.into_inner().healthy);
    
    // Verify all nodes see the same cluster state
    for i in 1..=3 {
        let mut client = cluster.client(i).await.expect("Failed to get client");
        let status = client.get_cluster_status(ClusterStatusRequest {})
            .await
            .expect("Failed to get cluster status");
        
        assert_eq!(status.into_inner().nodes.len(), 3);
    }
    
    // Test adding a fourth node dynamically
    let new_node_id = cluster.add_node().await.expect("Failed to add node");
    assert_eq!(new_node_id, 4);
    
    // Wait for the new node to be seen by all
    timing::wait_for_condition_with_backoff(
        || async {
            let mut client = cluster.client(1).await.unwrap();
            let status = client.get_cluster_status(ClusterStatusRequest {}).await;
            status.map(|s| s.into_inner().nodes.len() == 4).unwrap_or(false)
        },
        Duration::from_secs(10),
        Duration::from_millis(100),
    )
    .await
    .expect("New node not recognized");
    
    // Clean shutdown
    cluster.shutdown().await;
}

#[tokio::test]
async fn test_node_leave_cluster() {
    // Create a two-node cluster manually
    let node1 = TestNode::builder()
        .with_id(1)
        .with_auto_port()
        .build()
        .await
        .expect("Failed to create node 1");
    
    let node2 = TestNode::builder()
        .with_id(2)
        .with_auto_port()
        .with_join_addr(Some(node1.addr))
        .build()
        .await
        .expect("Failed to create node 2");
    
    // Join the cluster
    node2.node.send_join_request().await.expect("Join failed");
    
    // Wait for convergence
    timing::robust_sleep(Duration::from_secs(2)).await;
    
    // Node 2 leaves the cluster
    let mut client2 = node2.client().await.expect("Failed to get client");
    let leave_response = client2.leave_cluster(LeaveRequest { node_id: 2 })
        .await
        .expect("Leave request failed");
    
    assert!(leave_response.into_inner().success);
    
    // Verify node 1 only sees itself
    timing::wait_for_condition_with_backoff(
        || async {
            let status = node1.shared_state.get_cluster_status().await;
            status.map(|(_, nodes, _)| nodes.len() == 1).unwrap_or(false)
        },
        Duration::from_secs(5),
        Duration::from_millis(100),
    )
    .await
    .expect("Node removal not reflected");
    
    // Clean shutdown
    node2.shutdown().await;
    node1.shutdown().await;
}

#[tokio::test]
async fn test_automatic_port_allocation() {
    // Create multiple nodes with automatic port allocation
    let mut nodes = Vec::new();
    
    for i in 1..=5 {
        let node = TestNode::builder()
            .with_id(i)
            .with_auto_port()  // Automatic port allocation
            .build()
            .await
            .expect("Failed to create node");
        
        nodes.push(node);
    }
    
    // Verify all nodes have different ports
    let mut ports = nodes.iter().map(|n| n.addr.port()).collect::<Vec<_>>();
    ports.sort_unstable();
    ports.dedup();
    assert_eq!(ports.len(), 5, "Port collision detected");
    
    // Clean shutdown
    for node in nodes {
        node.shutdown().await;
    }
}

#[cfg(test)]
mod ci_aware_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_with_ci_aware_timeouts() {
        // This test automatically adjusts timeouts based on environment
        let base_timeout = Duration::from_secs(5);
        let scaled_timeout = timing::scaled_timeout(base_timeout);
        
        println!("Base timeout: {:?}, Scaled timeout: {:?}", base_timeout, scaled_timeout);
        
        // In CI, this might be 15 seconds instead of 5
        assert!(scaled_timeout >= base_timeout);
    }
}