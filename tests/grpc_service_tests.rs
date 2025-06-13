use std::sync::Arc;
use std::net::SocketAddr;
use tokio::time::{sleep, Duration};
use tonic::transport::Channel;

use blixard::{
    node_shared::SharedNodeState,
    types::NodeConfig,
    proto::{
        blixard_service_client::BlixardServiceClient,
        cluster_service_client::ClusterServiceClient,
        GetRaftStatusRequest, ProposeTaskRequest, Task,
        HealthCheckRequest,
    },
    grpc_server::start_grpc_server,
};

async fn setup_test_server() -> (Arc<SharedNodeState>, SocketAddr, tokio::task::JoinHandle<()>) {
    let node_config = NodeConfig {
        id: 1,
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        data_dir: tempfile::tempdir().unwrap().path().to_string_lossy().to_string(),
        join_addr: None,
        use_tailscale: false,
    };
    
    let shared_state = Arc::new(SharedNodeState::new(node_config));
    
    // Find an available port
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    
    let state_clone = shared_state.clone();
    let handle = tokio::spawn(async move {
        start_grpc_server(state_clone, addr).await.unwrap();
    });
    
    // Give the server time to start
    sleep(Duration::from_millis(100)).await;
    
    (shared_state, addr, handle)
}

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
                sleep(retry_delay).await;
                retry_delay *= 2;
            }
        }
    }
}

#[tokio::test]
async fn test_grpc_server_starts_successfully() {
    let (_state, addr, handle) = setup_test_server().await;
    
    // Try to connect to the server
    let (_blixard_client, mut cluster_client) = create_client(addr).await;
    
    // Test health check
    let response = cluster_client.health_check(HealthCheckRequest {}).await;
    assert!(response.is_ok());
    let health = response.unwrap().into_inner();
    assert!(health.healthy);
    assert!(health.message.contains("Node 1 is healthy"));
    
    // Cleanup
    handle.abort();
}

#[tokio::test]
async fn test_get_raft_status_default_state() {
    let (_state, addr, handle) = setup_test_server().await;
    
    let (mut blixard_client, _cluster_client) = create_client(addr).await;
    
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
    handle.abort();
}

#[tokio::test]
async fn test_get_raft_status_after_update() {
    let (state, addr, handle) = setup_test_server().await;
    
    // Update Raft status
    state.update_raft_status(true, Some(1), 5, "leader".to_string()).await;
    
    let (mut blixard_client, _cluster_client) = create_client(addr).await;
    
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
    handle.abort();
}

#[tokio::test]
async fn test_propose_task_without_raft_manager() {
    let (_state, addr, handle) = setup_test_server().await;
    
    let (mut blixard_client, _cluster_client) = create_client(addr).await;
    
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
    handle.abort();
}

#[tokio::test]
async fn test_propose_task_validation() {
    let (_state, addr, handle) = setup_test_server().await;
    
    let (mut blixard_client, _cluster_client) = create_client(addr).await;
    
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
    handle.abort();
}

#[tokio::test]
async fn test_both_services_accessible() {
    let (_state, addr, handle) = setup_test_server().await;
    
    let (mut blixard_client, mut cluster_client) = create_client(addr).await;
    
    // Test BlixardService
    let raft_response = blixard_client.get_raft_status(GetRaftStatusRequest {}).await;
    assert!(raft_response.is_ok());
    
    // Test ClusterService
    let health_response = cluster_client.health_check(HealthCheckRequest {}).await;
    assert!(health_response.is_ok());
    
    // Cleanup
    handle.abort();
}

#[tokio::test]
async fn test_concurrent_requests() {
    let (_state, addr, handle) = setup_test_server().await;
    
    let (blixard_client, cluster_client) = create_client(addr).await;
    
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
    handle.abort();
}

#[tokio::test]
async fn test_task_proposal_with_various_configs() {
    let (_state, addr, handle) = setup_test_server().await;
    
    let (mut blixard_client, _cluster_client) = create_client(addr).await;
    
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
    handle.abort();
}

#[tokio::test]
async fn test_server_shutdown_gracefully() {
    let (_state, addr, handle) = setup_test_server().await;
    
    let (mut blixard_client, _cluster_client) = create_client(addr).await;
    
    // Verify server is running
    let response = blixard_client.get_raft_status(GetRaftStatusRequest {}).await;
    assert!(response.is_ok());
    
    // Abort the server
    handle.abort();
    
    // Give it time to shut down
    sleep(Duration::from_millis(500)).await;
    
    // Create a new client to test connection - existing client may have cached connection
    let endpoint = format!("http://{}", addr);
    let connect_result = Channel::from_shared(endpoint).unwrap().connect().await;
    assert!(connect_result.is_err());
}

#[tokio::test]
async fn test_error_handling_for_invalid_raft_state() {
    let (state, addr, handle) = setup_test_server().await;
    
    // Set some edge case values
    state.update_raft_status(false, None, u64::MAX, "unknown".to_string()).await;
    
    let (mut blixard_client, _cluster_client) = create_client(addr).await;
    
    // Should still return valid response
    let response = blixard_client.get_raft_status(GetRaftStatusRequest {}).await;
    assert!(response.is_ok());
    
    let status = response.unwrap().into_inner();
    assert!(!status.is_leader);
    assert_eq!(status.leader_id, 0); // None becomes 0
    assert_eq!(status.term, u64::MAX);
    assert_eq!(status.state, "unknown");
    
    // Cleanup
    handle.abort();
}