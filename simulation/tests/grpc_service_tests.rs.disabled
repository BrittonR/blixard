#![cfg(feature = "test-helpers")]

use std::net::SocketAddr;
use tokio::time::Duration;
use tonic::transport::Channel;

use blixard_core::{
    proto::{
        blixard_service_client::BlixardServiceClient,
        cluster_service_client::ClusterServiceClient,
        GetRaftStatusRequest, ProposeTaskRequest, Task,
        HealthCheckRequest,
    },
    test_helpers::{TestNode, timing},
};

async fn create_client(addr: SocketAddr) -> (BlixardServiceClient<Channel>, ClusterServiceClient<Channel>) {
    let endpoint = format!("http://{}", addr);
    
    // Retry connection with exponential backoff
    let mut retry_delay = Duration::from_millis(10);
    let mut attempts = 0;
    
    loop {
        match Channel::from_shared(endpoint.clone()).unwrap().connect().await {
            Ok(channel) => {
                let blixard_client = BlixardServiceClient::new(channel.clone());
                let cluster_client = ClusterServiceClient::new(channel);
                return (blixard_client, cluster_client);
            }
            Err(e) => {
                attempts += 1;
                if attempts > 10 {
                    panic!("Failed to connect to gRPC server after 10 attempts: {}", e);
                }
                timing::robust_sleep(retry_delay).await;
                retry_delay *= 2;
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_grpc_server_starts_successfully() {
    let node = TestNode::builder()
        .with_id(1)
        .with_auto_port().await
        .build()
        .await
        .unwrap();
    
    // Try to connect to the server
    let (_blixard_client, mut cluster_client) = create_client(node.addr).await;
    
    // Test health check
    let response = cluster_client.health_check(HealthCheckRequest {}).await;
    assert!(response.is_ok());
    let health = response.unwrap().into_inner();
    assert!(health.healthy);
    assert!(health.message.contains("Node 1 is healthy"));
    
    // Cleanup
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_get_raft_status_default_state() {
    let node = TestNode::builder()
        .with_id(1)
        .with_auto_port().await
        .build()
        .await
        .unwrap();
    
    let (mut blixard_client, _cluster_client) = create_client(node.addr).await;
    
    // Get Raft status
    let response = blixard_client.get_raft_status(GetRaftStatusRequest {}).await;
    assert!(response.is_ok());
    
    let status = response.unwrap().into_inner();
    assert!(!status.is_leader);
    assert_eq!(status.node_id, 1);
    assert_eq!(status.leader_id, 0); // No leader initially
    assert_eq!(status.term, 0);
    assert_eq!(status.state, "follower");
    
    // Cleanup
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_get_raft_status_after_update() {
    let node = TestNode::builder()
        .with_id(1)
        .with_auto_port().await
        .build()
        .await
        .unwrap();
    
    // Update Raft status
    let status = blixard_core::node_shared::RaftStatus {
        is_leader: true,
        node_id: 1,
        leader_id: Some(1),
        term: 5,
        state: "leader".to_string(),
    };
    node.shared_state.update_raft_status(status).await;
    
    let (mut blixard_client, _cluster_client) = create_client(node.addr).await;
    
    // Get Raft status
    let response = blixard_client.get_raft_status(GetRaftStatusRequest {}).await;
    assert!(response.is_ok());
    
    let status = response.unwrap().into_inner();
    assert!(status.is_leader);
    assert_eq!(status.node_id, 1);
    assert_eq!(status.leader_id, 1);
    assert_eq!(status.term, 5);
    assert_eq!(status.state, "leader");
    
    // Cleanup
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_propose_task_without_raft_manager() {
    let node = TestNode::builder()
        .with_id(1)
        .with_auto_port().await
        .build()
        .await
        .unwrap();
    
    let (mut blixard_client, _cluster_client) = create_client(node.addr).await;
    
    // Propose a task (will fail without Raft manager)
    let task = Task {
        id: "test-task-1".to_string(),
        command: "echo".to_string(),
        args: vec!["hello".to_string()],
        cpu_cores: 1,
        memory_mb: 512,
    };
    
    let response = blixard_client.propose_task(ProposeTaskRequest {
        task: Some(task),
    }).await;
    
    assert!(response.is_ok());
    let result = response.unwrap().into_inner();
    assert!(!result.success);
    assert!(result.message.contains("Failed to propose task"));
    
    // Cleanup
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_propose_task_validation() {
    let node = TestNode::builder()
        .with_id(1)
        .with_auto_port().await
        .build()
        .await
        .unwrap();
    
    let (mut blixard_client, _cluster_client) = create_client(node.addr).await;
    
    // Test with missing task
    let response = blixard_client.propose_task(ProposeTaskRequest {
        task: None,
    }).await;
    
    assert!(response.is_err());
    let error = response.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(error.message().contains("Task is required"));
    
    // Test with empty task ID
    let task = Task {
        id: "".to_string(),
        command: "echo".to_string(),
        args: vec![],
        cpu_cores: 1,
        memory_mb: 512,
    };
    
    let response = blixard_client.propose_task(ProposeTaskRequest {
        task: Some(task),
    }).await;
    
    assert!(response.is_ok());
    let result = response.unwrap().into_inner();
    assert!(!result.success);
    assert_eq!(result.message, "Task ID cannot be empty");
    
    // Cleanup
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_both_services_accessible() {
    let node = TestNode::builder()
        .with_id(1)
        .with_auto_port().await
        .build()
        .await
        .unwrap();
    
    let (mut blixard_client, mut cluster_client) = create_client(node.addr).await;
    
    // Test BlixardService
    let raft_response = blixard_client.get_raft_status(GetRaftStatusRequest {}).await;
    assert!(raft_response.is_ok());
    
    // Test ClusterService
    let health_response = cluster_client.health_check(HealthCheckRequest {}).await;
    assert!(health_response.is_ok());
    
    // Cleanup
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_concurrent_requests() {
    let node = TestNode::builder()
        .with_id(1)
        .with_auto_port().await
        .build()
        .await
        .unwrap();
    
    let (blixard_client, cluster_client) = create_client(node.addr).await;
    
    // Spawn multiple concurrent requests
    let mut handles = vec![];
    
    for i in 0..10 {
        let mut blixard_clone = blixard_client.clone();
        let mut cluster_clone = cluster_client.clone();
        
        let h = tokio::spawn(async move {
            // Alternate between services
            if i % 2 == 0 {
                let response = blixard_clone.get_raft_status(GetRaftStatusRequest {}).await;
                assert!(response.is_ok());
            } else {
                let response = cluster_clone.health_check(HealthCheckRequest {}).await;
                assert!(response.is_ok());
            }
        });
        
        handles.push(h);
    }
    
    // Wait for all requests to complete
    for h in handles {
        h.await.unwrap();
    }
    
    // Cleanup
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_task_proposal_with_various_configs() {
    let node = TestNode::builder()
        .with_id(1)
        .with_auto_port().await
        .build()
        .await
        .unwrap();
    
    let (mut blixard_client, _cluster_client) = create_client(node.addr).await;
    
    // Test various task configurations
    let test_cases = vec![
        ("simple-task", "ls", vec![], 1, 256),
        ("complex-task", "python", vec!["-m", "http.server"], 2, 1024),
        ("resource-heavy", "cargo", vec!["build", "--release"], 4, 2048),
    ];
    
    for (id, cmd, args, cpu, mem) in test_cases {
        let task = Task {
            id: id.to_string(),
            command: cmd.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
            cpu_cores: cpu,
            memory_mb: mem,
        };
        
        let response = blixard_client.propose_task(ProposeTaskRequest {
            task: Some(task.clone()),
        }).await;
        
        assert!(response.is_ok());
        let result = response.unwrap().into_inner();
        
        // Without Raft manager, all should fail but with proper error message
        assert!(!result.success);
        assert!(result.message.contains("Failed to propose task"));
    }
    
    // Cleanup
    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_server_shutdown_gracefully() {
    let node = TestNode::builder()
        .with_id(1)
        .with_auto_port().await
        .build()
        .await
        .unwrap();
    
    let addr = node.addr;
    let (mut blixard_client, _cluster_client) = create_client(addr).await;
    
    // Verify server is running
    let response = blixard_client.get_raft_status(GetRaftStatusRequest {}).await;
    assert!(response.is_ok());
    
    // Shutdown the node
    node.shutdown().await;
    
    // Give it time to shut down
    timing::robust_sleep(Duration::from_millis(500)).await;
    
    // Create a new client to test connection - existing client may have cached connection
    let endpoint = format!("http://{}", addr);
    let connect_result = Channel::from_shared(endpoint).unwrap().connect().await;
    assert!(connect_result.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_error_handling_for_invalid_raft_state() {
    let node = TestNode::builder()
        .with_id(1)
        .with_auto_port().await
        .build()
        .await
        .unwrap();
    
    // Set some edge case values
    let status = blixard_core::node_shared::RaftStatus {
        is_leader: false,
        node_id: 1,
        leader_id: None,
        term: u64::MAX,
        state: "unknown".to_string(),
    };
    node.shared_state.update_raft_status(status).await;
    
    let (mut blixard_client, _cluster_client) = create_client(node.addr).await;
    
    // Should still return valid response
    let response = blixard_client.get_raft_status(GetRaftStatusRequest {}).await;
    assert!(response.is_ok());
    
    let status = response.unwrap().into_inner();
    assert!(!status.is_leader);
    assert_eq!(status.leader_id, 0); // None becomes 0
    assert_eq!(status.term, u64::MAX);
    assert_eq!(status.state, "unknown");
    
    // Cleanup
    node.shutdown().await;
}