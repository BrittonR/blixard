use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tempfile::TempDir;

use blixard::{
    node::Node,
    types::{NodeConfig, VmConfig, VmCommand},
    raft_manager::{TaskSpec, ResourceRequirements},
    grpc_server::start_grpc_server,
    proto::{
        cluster_service_client::ClusterServiceClient,
        TaskRequest, HealthCheckRequest, ClusterStatusRequest,
        CreateVmRequest, ListVmsRequest,
    },
};

// Helper to create test node configuration with temp directory
async fn create_test_node(id: u64, port: u16) -> (Node, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let config = NodeConfig {
        id,
        data_dir: temp_dir.path().to_str().unwrap().to_string(),
        bind_addr: format!("127.0.0.1:{}", port).parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
    };
    
    let mut node = Node::new(config);
    node.initialize().await.unwrap();
    (node, temp_dir)
}

#[tokio::test]
async fn test_single_node_grpc_health_check() {
    let (node, _temp_dir) = create_test_node(1, 7001).await;
    let node_shared = node.shared();
    
    // Start gRPC server
    let grpc_addr = "127.0.0.1:8001".parse().unwrap();
    let server_handle = tokio::spawn(start_grpc_server(node_shared.clone(), grpc_addr));
    
    // Give server time to start
    sleep(Duration::from_millis(1000)).await;
    
    // Connect gRPC client
    let mut client = ClusterServiceClient::connect("http://127.0.0.1:8001")
        .await
        .unwrap();
    
    // Health check
    let response = client.health_check(HealthCheckRequest {})
        .await
        .unwrap();
    
    assert!(response.into_inner().healthy);
    
    // Cleanup
    server_handle.abort();
}

#[tokio::test]
async fn test_cluster_formation_via_grpc() {
    // Create three nodes
    let (mut node1, _temp1) = create_test_node(1, 7001).await;
    let (mut node2, _temp2) = create_test_node(2, 7002).await;
    let (mut node3, _temp3) = create_test_node(3, 7003).await;
    
    // Start all nodes
    node1.start().await.unwrap();
    node2.start().await.unwrap();
    node3.start().await.unwrap();
    
    let node_shareds = vec![
        node1.shared(),
        node2.shared(),
        node3.shared(),
    ];
    
    // Start gRPC servers
    let mut handles = vec![];
    for (i, node_shared) in node_shareds.iter().enumerate() {
        let addr = format!("127.0.0.1:{}", 8001 + i).parse().unwrap();
        let handle = tokio::spawn(start_grpc_server(node_shared.clone(), addr));
        handles.push(handle);
    }
    
    sleep(Duration::from_millis(1000)).await;
    
    // Connect clients with retry
    let mut clients = vec![];
    for i in 0..3 {
        let mut attempts = 0;
        let client = loop {
            match ClusterServiceClient::connect(format!("http://127.0.0.1:{}", 8001 + i)).await {
                Ok(client) => break client,
                Err(e) => {
                    attempts += 1;
                    if attempts >= 10 {
                        panic!("Failed to connect to gRPC server after {} attempts: {}", attempts, e);
                    }
                    sleep(Duration::from_millis(500)).await;
                }
            }
        };
        clients.push(client);
    }
    
    // Check cluster status from each node
    for client in &mut clients {
        let response = client.get_cluster_status(ClusterStatusRequest {})
            .await
            .unwrap();
        
        let status = response.into_inner();
        assert!(!status.nodes.is_empty());
    }
    
    // Cleanup
    for handle in handles {
        handle.abort();
    }
}

#[tokio::test]
async fn test_task_submission_via_grpc() {
    let (mut node, _temp_dir) = create_test_node(1, 7001).await;
    node.start().await.unwrap();
    
    // Register the node as a worker
    node.join_cluster(None).await.unwrap();
    
    let node_shared = node.shared();
    
    // Start gRPC server
    let grpc_addr = "127.0.0.1:8001".parse().unwrap();
    let server_handle = tokio::spawn(start_grpc_server(node_shared.clone(), grpc_addr));
    
    sleep(Duration::from_millis(500)).await;
    
    // Connect client
    let mut client = ClusterServiceClient::connect("http://127.0.0.1:8001")
        .await
        .unwrap();
    
    // Submit a task
    let task_request = TaskRequest {
        task_id: "test-task-1".to_string(),
        command: "echo".to_string(),
        args: vec!["Hello, World!".to_string()],
        cpu_cores: 1,
        memory_mb: 256,
        disk_gb: 1,
        required_features: vec![],
        timeout_secs: 10,
    };
    
    let response = client.submit_task(task_request).await.unwrap();
    let task_resp = response.into_inner();
    
    assert!(task_resp.accepted);
    assert_eq!(task_resp.assigned_node, 1);
    
    // Cleanup
    server_handle.abort();
}

#[tokio::test]
async fn test_vm_operations_via_grpc() {
    let (mut node, _temp_dir) = create_test_node(1, 7001).await;
    node.start().await.unwrap();
    
    let node_shared = node.shared();
    
    // Start gRPC server
    let grpc_addr = "127.0.0.1:8001".parse().unwrap();
    let server_handle = tokio::spawn(start_grpc_server(node_shared.clone(), grpc_addr));
    
    sleep(Duration::from_millis(500)).await;
    
    // Connect client
    let mut client = ClusterServiceClient::connect("http://127.0.0.1:8001")
        .await
        .unwrap();
    
    // Create a VM
    let create_request = CreateVmRequest {
        name: "test-vm".to_string(),
        config_path: "/path/to/config".to_string(),
        vcpus: 2,
        memory_mb: 1024,
    };
    
    let response = client.create_vm(create_request).await.unwrap();
    assert!(response.into_inner().success);
    
    // List VMs
    let list_response = client.list_vms(ListVmsRequest {}).await.unwrap();
    let vms = list_response.into_inner().vms;
    
    assert_eq!(vms.len(), 1);
    assert_eq!(vms[0].name, "test-vm");
    
    // Cleanup
    server_handle.abort();
}

#[tokio::test]
async fn test_concurrent_task_submissions() {
    let (mut node, _temp_dir) = create_test_node(1, 7001).await;
    node.start().await.unwrap();
    node.join_cluster(None).await.unwrap();
    
    let node_shared = node.shared();
    
    // Start gRPC server
    let grpc_addr = "127.0.0.1:8001".parse().unwrap();
    let server_handle = tokio::spawn(start_grpc_server(node_shared.clone(), grpc_addr));
    
    sleep(Duration::from_millis(500)).await;
    
    // Create multiple clients
    let mut handles = vec![];
    
    for i in 0..10 {
        let handle = tokio::spawn(async move {
            let mut client = ClusterServiceClient::connect("http://127.0.0.1:8001")
                .await
                .unwrap();
            
            let task_request = TaskRequest {
                task_id: format!("concurrent-task-{}", i),
                command: "echo".to_string(),
                args: vec![format!("Task {}", i)],
                cpu_cores: 1,
                memory_mb: 256,
                disk_gb: 1,
                required_features: vec![],
                timeout_secs: 10,
            };
            
            client.submit_task(task_request).await
        });
        
        handles.push(handle);
    }
    
    // Collect results
    let mut success_count = 0;
    for handle in handles {
        if let Ok(Ok(response)) = handle.await {
            if response.into_inner().accepted {
                success_count += 1;
            }
        }
    }
    
    // All tasks should be accepted
    assert_eq!(success_count, 10);
    
    // Cleanup
    server_handle.abort();
}

#[tokio::test]
async fn test_cluster_state_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path().to_str().unwrap().to_string();
    
    // Phase 1: Create node, add some state
    {
        let config = NodeConfig {
            id: 1,
            data_dir: data_dir.clone(),
            bind_addr: "127.0.0.1:7001".parse().unwrap(),
            join_addr: None,
            use_tailscale: false,
        };
        
        let mut node = Node::new(config);
        node.initialize().await.unwrap();
        node.start().await.unwrap();
        node.join_cluster(None).await.unwrap();
        
        // Add a VM
        node.send_vm_command(VmCommand::Create {
            config: VmConfig {
                name: "persistent-vm".to_string(),
                config_path: "/test/path".to_string(),
                vcpus: 4,
                memory: 2048,
            },
            node_id: 1,
        }).await.unwrap();
        
        // Note: Task submission requires Raft to be fully operational
        // For now, we'll skip this part in the test
        
        // Stop node
        node.stop().await.unwrap();
    }
    
    // Phase 2: Restart node and verify state
    {
        let config = NodeConfig {
            id: 1,
            data_dir,
            bind_addr: "127.0.0.1:7001".parse().unwrap(),
            join_addr: None,
            use_tailscale: false,
        };
        
        let mut node = Node::new(config);
        node.initialize().await.unwrap();
        
        // Check VM exists
        let vms = node.list_vms().await.unwrap();
        assert_eq!(vms.len(), 1);
        assert_eq!(vms[0].0.name, "persistent-vm");
        
        // Task checking would go here once Raft is operational
        
        node.stop().await.unwrap();
    }
}

#[tokio::test]
async fn test_grpc_error_handling() {
    let (mut node, _temp_dir) = create_test_node(1, 7001).await;
    node.start().await.unwrap();
    
    let node_shared = node.shared();
    
    // Start gRPC server
    let grpc_addr = "127.0.0.1:8001".parse().unwrap();
    let server_handle = tokio::spawn(start_grpc_server(node_shared.clone(), grpc_addr));
    
    sleep(Duration::from_millis(500)).await;
    
    // Connect client
    let mut client = ClusterServiceClient::connect("http://127.0.0.1:8001")
        .await
        .unwrap();
    
    // Try to create VM with empty name (should fail)
    let create_request = CreateVmRequest {
        name: "".to_string(),
        config_path: "/path/to/config".to_string(),
        vcpus: 2,
        memory_mb: 1024,
    };
    
    let response = client.create_vm(create_request).await.unwrap();
    let create_resp = response.into_inner();
    
    assert!(!create_resp.success);
    assert!(create_resp.message.contains("empty"));
    
    // Try to submit task without available workers (should fail)
    let task_request = TaskRequest {
        task_id: "impossible-task".to_string(),
        command: "test".to_string(),
        args: vec![],
        cpu_cores: 1000, // Impossible requirement
        memory_mb: 1000000,
        disk_gb: 100000,
        required_features: vec!["quantum-computing".to_string()],
        timeout_secs: 10,
    };
    
    let response = client.submit_task(task_request).await.unwrap();
    let task_resp = response.into_inner();
    
    assert!(!task_resp.accepted);
    assert!(task_resp.message.contains("Failed"));
    
    // Cleanup
    server_handle.abort();
}