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
    
    // Give time for cluster to stabilize
    sleep(Duration::from_millis(500)).await;
    
    // Check cluster status from both nodes
    let status1 = client1.get_cluster_status(ClusterStatusRequest {}).await.unwrap().into_inner();
    let status2 = client2.get_cluster_status(ClusterStatusRequest {}).await.unwrap().into_inner();
    
    // Both should see 2 nodes
    assert_eq!(status1.nodes.len(), 2);
    assert_eq!(status2.nodes.len(), 2);
    
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
    
    sleep(Duration::from_millis(500)).await;
    
    // Verify 2 nodes in cluster
    let status = client1.get_cluster_status(ClusterStatusRequest {}).await.unwrap().into_inner();
    assert_eq!(status.nodes.len(), 2);
    
    // Node 2 leaves cluster
    let leave_req = LeaveRequest { node_id: 2 };
    let leave_resp = client1.leave_cluster(leave_req).await.unwrap();
    let leave_result = leave_resp.into_inner();
    
    assert!(leave_result.success, "Leave failed: {}", leave_result.message);
    
    sleep(Duration::from_millis(500)).await;
    
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
    
    sleep(Duration::from_millis(1000)).await;
    
    // Check all nodes see the full cluster
    let clients = vec![
        create_client(addr1).await,
        create_client(addr2).await,
        create_client(addr3).await,
    ];
    
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