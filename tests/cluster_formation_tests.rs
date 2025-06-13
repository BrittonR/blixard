use std::net::SocketAddr;
use tokio::time::{sleep, Duration, timeout};
use tonic::transport::Channel;

use blixard::{
    test_helpers::TestNode,
    proto::{
        cluster_service_client::ClusterServiceClient,
        JoinRequest, LeaveRequest, ClusterStatusRequest,
        HealthCheckRequest,
    },
};

#[path = "common/mod.rs"]
mod common;
use common::test_timing::wait_for_condition;

async fn create_client(addr: SocketAddr) -> ClusterServiceClient<Channel> {
    let endpoint = format!("http://{}", addr);
    
    // Retry connection with timeout
    let client = timeout(Duration::from_secs(5), async {
        loop {
            match Channel::from_shared(endpoint.clone()).unwrap().connect().await {
                Ok(channel) => return ClusterServiceClient::new(channel),
                Err(_) => sleep(Duration::from_millis(100)).await,
            }
        }
    }).await.expect("Failed to connect to server");
    
    client
}

/// Wait for all nodes in the cluster to converge to the expected member count
async fn wait_for_cluster_convergence(
    clients: Vec<ClusterServiceClient<Channel>>,
    expected_nodes: usize,
    timeout_duration: Duration,
) -> Result<(), String> {
    wait_for_condition(
        move || {
            let clients_clone = clients.clone();
            async move {
                // Check if all clients see the expected number of nodes
                for mut client in clients_clone {
                    if let Ok(status) = client.get_cluster_status(ClusterStatusRequest {}).await {
                        if status.into_inner().nodes.len() != expected_nodes {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
                true
            }
        },
        timeout_duration,
        Duration::from_millis(100),
    )
    .await
}

#[tokio::test]
async fn test_single_node_cluster() {
    let node = TestNode::start(1, 17001).await.unwrap();
    let addr = node.addr;
    
    let mut client = create_client(addr).await;
    
    // Check health
    let health_resp = client.health_check(HealthCheckRequest {}).await.unwrap();
    assert!(health_resp.into_inner().healthy);
    
    // Check cluster status
    let status_resp = client.get_cluster_status(ClusterStatusRequest {}).await.unwrap();
    let status = status_resp.into_inner();
    
    assert_eq!(status.nodes.len(), 1);
    assert_eq!(status.nodes[0].id, 1);
    assert_eq!(status.nodes[0].address, addr.to_string());
    
    // Cleanup
    node.shutdown().await;
}

#[tokio::test]
async fn test_two_node_cluster_formation() {
    // Start first node
    let node1 = TestNode::start(1, 17002).await.unwrap();
    let addr1 = node1.addr;
    
    // Start second node with join_addr pointing to first node
    let node2 = TestNode::start_with_join(2, 17003, Some(addr1)).await.unwrap();
    let addr2 = node2.addr;
    
    // Create clients
    let mut client1 = create_client(addr1).await;
    let mut client2 = create_client(addr2).await;
    
    // Node 2 joins cluster at node 1
    let join_req = JoinRequest {
        node_id: 2,
        bind_address: addr2.to_string(),
    };
    
    let join_resp = client1.join_cluster(join_req).await.unwrap();
    let join_result = join_resp.into_inner();
    
    assert!(join_result.success, "Join failed: {}", join_result.message);
    
    // Give the configuration change some time to be processed
    sleep(Duration::from_millis(500)).await;
    
    // WORKAROUND: Node 2 needs to receive log entries to know it's part of the cluster
    // Send a health check through node 1 to trigger log replication
    let _ = client1.health_check(HealthCheckRequest {}).await;
    
    // Wait for cluster to stabilize - both nodes should see 2 members
    // The configuration change needs to propagate from node 1 to node 2
    let client1_clone = client1.clone();
    let client2_clone = client2.clone();
    
    wait_for_condition(
        move || {
            let mut c1 = client1_clone.clone();
            let mut c2 = client2_clone.clone();
            async move {
                // Check if both nodes see 2 members
                if let Ok(status1) = c1.get_cluster_status(ClusterStatusRequest {}).await {
                    if let Ok(status2) = c2.get_cluster_status(ClusterStatusRequest {}).await {
                        let s1 = status1.into_inner();
                        let s2 = status2.into_inner();
                        return s1.nodes.len() == 2 && s2.nodes.len() == 2;
                    }
                }
                false
            }
        },
        Duration::from_secs(10),
        Duration::from_millis(100),
    )
    .await
    .expect("Cluster failed to converge to 2 nodes");
    
    // Now check the actual status
    let status1 = client1.get_cluster_status(ClusterStatusRequest {}).await.unwrap().into_inner();
    let status2 = client2.get_cluster_status(ClusterStatusRequest {}).await.unwrap().into_inner();
    
    // Verify node IDs
    let node_ids1: Vec<u64> = status1.nodes.iter().map(|n| n.id).collect();
    let node_ids2: Vec<u64> = status2.nodes.iter().map(|n| n.id).collect();
    
    assert!(node_ids1.contains(&1));
    assert!(node_ids1.contains(&2));
    assert!(node_ids2.contains(&1));
    assert!(node_ids2.contains(&2));
    
    // Cleanup
    node1.shutdown().await;
    node2.shutdown().await;
}

#[tokio::test]
async fn test_node_leave_cluster() {
    // Start first node
    let node1 = TestNode::start(1, 17004).await.unwrap();
    let addr1 = node1.addr;
    
    // Start second node with join_addr pointing to first node
    let node2 = TestNode::start_with_join(2, 17005, Some(addr1)).await.unwrap();
    let addr2 = node2.addr;
    
    let mut client1 = create_client(addr1).await;
    
    // Node 2 joins cluster
    let join_req = JoinRequest {
        node_id: 2,
        bind_address: addr2.to_string(),
    };
    client1.join_cluster(join_req).await.unwrap();
    
    // Wait for cluster to stabilize
    wait_for_cluster_convergence(
        vec![client1.clone()],
        2,
        Duration::from_secs(10),
    )
    .await
    .expect("Cluster failed to converge to 2 nodes");
    
    // Verify 2 nodes in cluster
    let status = client1.get_cluster_status(ClusterStatusRequest {}).await.unwrap().into_inner();
    assert_eq!(status.nodes.len(), 2);
    
    // Node 2 leaves cluster
    let leave_req = LeaveRequest { node_id: 2 };
    let leave_resp = client1.leave_cluster(leave_req).await.unwrap();
    let leave_result = leave_resp.into_inner();
    
    assert!(leave_result.success, "Leave failed: {}", leave_result.message);
    
    // Wait for node removal to complete
    wait_for_cluster_convergence(
        vec![client1.clone()],
        1,
        Duration::from_secs(10),
    )
    .await
    .expect("Cluster failed to converge to 1 node after removal");
    
    // Verify only 1 node remains
    let status = client1.get_cluster_status(ClusterStatusRequest {}).await.unwrap().into_inner();
    assert_eq!(status.nodes.len(), 1);
    assert_eq!(status.nodes[0].id, 1);
    
    // Cleanup
    node1.shutdown().await;
    node2.shutdown().await;
}

#[tokio::test]
async fn test_duplicate_join_rejected() {
    let node1 = TestNode::start(1, 17006).await.unwrap();
    let addr1 = node1.addr;
    
    let node2 = TestNode::start_with_join(2, 17007, Some(addr1)).await.unwrap();
    let addr2 = node2.addr;
    
    let mut client1 = create_client(addr1).await;
    
    // First join should succeed
    let join_req = JoinRequest {
        node_id: 2,
        bind_address: addr2.to_string(),
    };
    let resp1 = client1.join_cluster(join_req.clone()).await.unwrap().into_inner();
    assert!(resp1.success);
    
    // Second join should fail
    let resp2 = client1.join_cluster(join_req).await.unwrap().into_inner();
    assert!(!resp2.success);
    assert!(resp2.message.contains("already exists"));
    
    // Cleanup
    node1.shutdown().await;
    node2.shutdown().await;
}

#[tokio::test]
async fn test_leave_non_existent_node() {
    let node = TestNode::start(1, 17008).await.unwrap();
    let addr = node.addr;
    
    let mut client = create_client(addr).await;
    
    // Try to remove non-existent node
    let leave_req = LeaveRequest { node_id: 99 };
    let resp = client.leave_cluster(leave_req).await.unwrap().into_inner();
    
    assert!(!resp.success);
    assert!(resp.message.contains("not found"));
    
    // Cleanup
    node.shutdown().await;
}

#[tokio::test]
async fn test_cannot_remove_self() {
    let node = TestNode::start(1, 17009).await.unwrap();
    let addr = node.addr;
    
    let mut client = create_client(addr).await;
    
    // Try to remove self
    let leave_req = LeaveRequest { node_id: 1 };
    let resp = client.leave_cluster(leave_req).await.unwrap().into_inner();
    
    assert!(!resp.success);
    println!("Remove self response: {}", resp.message);
    assert!(resp.message.contains("Cannot remove self from cluster"));
    
    // Cleanup
    node.shutdown().await;
}

#[tokio::test]
async fn test_join_with_invalid_node_id() {
    let node = TestNode::start(1, 17010).await.unwrap();
    let addr = node.addr;
    
    let mut client = create_client(addr).await;
    
    // Try to join with invalid node ID
    let join_req = JoinRequest {
        node_id: 0,
        bind_address: "127.0.0.1:9999".to_string(),
    };
    let resp = client.join_cluster(join_req).await.unwrap().into_inner();
    
    assert!(!resp.success);
    assert!(resp.message.contains("Invalid node ID"));
    
    // Cleanup
    node.shutdown().await;
}

#[tokio::test]
async fn test_join_with_empty_address() {
    let node = TestNode::start(1, 17011).await.unwrap();
    let addr = node.addr;
    
    let mut client = create_client(addr).await;
    
    // Try to join with empty address
    let join_req = JoinRequest {
        node_id: 2,
        bind_address: "".to_string(),
    };
    let resp = client.join_cluster(join_req).await.unwrap().into_inner();
    
    assert!(!resp.success);
    assert!(resp.message.contains("Bind address cannot be empty"));
    
    // Cleanup
    node.shutdown().await;
}

#[tokio::test]
async fn test_three_node_cluster() {
    // Start three nodes
    let node1 = TestNode::start(1, 17012).await.unwrap();
    let addr1 = node1.addr;
    
    let node2 = TestNode::start_with_join(2, 17013, Some(addr1)).await.unwrap();
    let addr2 = node2.addr;
    
    let node3 = TestNode::start_with_join(3, 17014, Some(addr1)).await.unwrap();
    let addr3 = node3.addr;
    
    let mut client1 = create_client(addr1).await;
    
    // Join node 2
    let join2 = JoinRequest {
        node_id: 2,
        bind_address: addr2.to_string(),
    };
    client1.join_cluster(join2).await.unwrap();
    
    // Join node 3
    let join3 = JoinRequest {
        node_id: 3,
        bind_address: addr3.to_string(),
    };
    client1.join_cluster(join3).await.unwrap();
    
    // Create clients for all nodes
    let clients = vec![
        create_client(addr1).await,
        create_client(addr2).await,
        create_client(addr3).await,
    ];
    
    // Wait for all nodes to see the complete cluster
    wait_for_cluster_convergence(
        clients.clone(),
        3,
        Duration::from_secs(15),
    )
    .await
    .expect("Cluster failed to converge to 3 nodes");
    
    // Verify all nodes see the full cluster
    for mut client in clients {
        let status = client.get_cluster_status(ClusterStatusRequest {}).await.unwrap().into_inner();
        assert_eq!(status.nodes.len(), 3);
        
        let node_ids: Vec<u64> = status.nodes.iter().map(|n| n.id).collect();
        assert!(node_ids.contains(&1));
        assert!(node_ids.contains(&2));
        assert!(node_ids.contains(&3));
    }
    
    // Cleanup
    node1.shutdown().await;
    node2.shutdown().await;
    node3.shutdown().await;
}