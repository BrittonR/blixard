#![cfg(feature = "test-helpers")]

use std::time::Duration;
use tempfile::TempDir;
use std::net::SocketAddr;

use blixard::{
    node::Node,
    types::{NodeConfig, VmConfig, VmCommand},
    grpc_server::start_grpc_server,
    proto::{
        cluster_service_client::ClusterServiceClient,
        TaskRequest, HealthCheckRequest, ClusterStatusRequest,
        CreateVmRequest, ListVmsRequest,
    },
    test_helpers::PortAllocator,
};

mod common;
use common::test_timing::*;

// Helper to get a free port
fn get_free_port() -> u16 {
    PortAllocator::next_port()
}

// Helper to create test node configuration with temp directory and dynamic port
async fn create_test_node_with_port(id: u64, port: u16) -> (Node, TempDir) {
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

// Helper to create test node with automatic port allocation
async fn create_test_node(id: u64) -> (Node, TempDir, u16) {
    let port = get_free_port();
    let (node, temp_dir) = create_test_node_with_port(id, port).await;
    (node, temp_dir, port)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_single_node_grpc_health_check() {
    let (node, _temp_dir, _node_port) = create_test_node(1).await;
    let node_shared = node.shared();
    
    // Start gRPC server with dynamic port
    let grpc_port = get_free_port();
    let grpc_addr: SocketAddr = format!("127.0.0.1:{}", grpc_port).parse().unwrap();
    let server_handle = tokio::spawn(start_grpc_server(node_shared.clone(), grpc_addr));
    
    // Wait for server to be ready
    wait_for_service_ready(
        || async {
            ClusterServiceClient::connect(format!("http://127.0.0.1:{}", grpc_port))
                .await
                .map(|_| true)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
        },
        "gRPC server",
        Duration::from_secs(5),
    )
    .await
    .unwrap();
    
    // Connect gRPC client with retry
    let mut client = connect_with_retry(
        || ClusterServiceClient::connect(format!("http://127.0.0.1:{}", grpc_port)),
        "gRPC client",
    )
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cluster_formation_via_grpc() {
    // Create three nodes with dynamic ports
    let (mut node1, _temp1, _port1) = create_test_node(1).await;
    let (mut node2, _temp2, _port2) = create_test_node(2).await;
    let (mut node3, _temp3, _port3) = create_test_node(3).await;
    
    // Start all nodes
    node1.start().await.unwrap();
    node2.start().await.unwrap();
    node3.start().await.unwrap();
    
    let node_shareds = vec![
        node1.shared(),
        node2.shared(),
        node3.shared(),
    ];
    
    // Start gRPC servers with dynamic ports
    let mut handles = vec![];
    let mut grpc_ports = vec![];
    for node_shared in node_shareds.iter() {
        let port = get_free_port();
        grpc_ports.push(port);
        let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
        let handle = tokio::spawn(start_grpc_server(node_shared.clone(), addr));
        handles.push(handle);
    }
    
    // Wait for all servers to be ready
    for port in &grpc_ports {
        wait_for_service_ready(
            || async {
                ClusterServiceClient::connect(format!("http://127.0.0.1:{}", port))
                    .await
                    .map(|_| true)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
            },
            &format!("gRPC server on port {}", port),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
    }
    
    // Connect clients with retry
    let mut clients = vec![];
    for port in &grpc_ports {
        let client = connect_with_retry(
            || ClusterServiceClient::connect(format!("http://127.0.0.1:{}", port)),
            &format!("gRPC client on port {}", port),
        )
        .await
        .unwrap();
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_task_submission_via_grpc() {
    // Use TestNode from test_helpers for better setup
    use blixard::test_helpers::TestNode;
    
    // Create a properly initialized single-node cluster
    let test_node = TestNode::builder()
        .with_id(1)
        .with_auto_port()
        .build()
        .await
        .expect("Failed to create test node");
    
    // The TestNode builder already:
    // 1. Initializes the node (which registers it as a worker in bootstrap mode)
    // 2. Starts the gRPC server 
    // 3. Sets up proper state management
    
    // Wait for the node to become leader (single-node should auto-elect)
    wait_for_condition(
        || async {
            let raft_status = test_node.shared_state.get_raft_status().await.ok();
            match raft_status {
                Some(status) => {
                    if !status.is_leader {
                        eprintln!("Node not yet leader, state: {:?}", status.state);
                    }
                    status.is_leader
                },
                None => {
                    eprintln!("No raft status available yet");
                    false
                }
            }
        },
        scaled_timeout(Duration::from_secs(10)),  // More time for bootstrap
        Duration::from_millis(100),
    )
    .await
    .expect("Single node should become leader");
    
    // Get a client connected to this node
    let mut client = test_node.client().await
        .expect("Failed to get client");
    
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
    
    if !task_resp.accepted {
        eprintln!("Task submission failed: {}", task_resp.message);
    }
    
    assert!(task_resp.accepted);
    assert_eq!(task_resp.assigned_node, 1);
    
    // Cleanup
    test_node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_vm_operations_via_grpc() {
    let (mut node, _temp_dir, _node_port) = create_test_node(1).await;
    node.start().await.unwrap();
    
    let node_shared = node.shared();
    
    // Start gRPC server with dynamic port
    let grpc_port = get_free_port();
    let grpc_addr: SocketAddr = format!("127.0.0.1:{}", grpc_port).parse().unwrap();
    let server_handle = tokio::spawn(start_grpc_server(node_shared.clone(), grpc_addr));
    
    // Wait for server to be ready
    wait_for_service_ready(
        || async {
            ClusterServiceClient::connect(format!("http://127.0.0.1:{}", grpc_port))
                .await
                .map(|_| true)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
        },
        "gRPC server",
        Duration::from_secs(3),
    )
    .await
    .unwrap();
    
    // Connect client with retry
    let mut client = connect_with_retry(
        || ClusterServiceClient::connect(format!("http://127.0.0.1:{}", grpc_port)),
        "gRPC client",
    )
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_concurrent_task_submissions() {
    let (mut node, _temp_dir, _node_port) = create_test_node(1).await;
    node.start().await.unwrap();
    node.join_cluster(None).await.unwrap();
    
    let node_shared = node.shared();
    
    // Start gRPC server with dynamic port
    let grpc_port = get_free_port();
    let grpc_addr: SocketAddr = format!("127.0.0.1:{}", grpc_port).parse().unwrap();
    let server_handle = tokio::spawn(start_grpc_server(node_shared.clone(), grpc_addr));
    
    // Wait for Raft leader election with proper timeout
    wait_for_condition(
        || async {
            // In single-node mode, the node should quickly become leader
            let raft_status = node_shared.get_raft_status().await.ok();
            match raft_status {
                Some(status) => status.is_leader,
                None => false
            }
        },
        scaled_timeout(Duration::from_secs(3)),
        Duration::from_millis(100),
    )
    .await
    .unwrap_or_else(|_| {
        tracing::warn!("Raft leader election timeout - continuing anyway");
    });
    
    // Create multiple clients
    let mut handles = vec![];
    
    for i in 0..10 {
        let grpc_url = format!("http://127.0.0.1:{}", grpc_port);
        let handle = tokio::spawn(async move {
            let mut client = connect_with_retry(
                || ClusterServiceClient::connect(grpc_url.clone()),
                "concurrent gRPC client",
            )
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
            let resp = response.into_inner();
            if resp.accepted {
                success_count += 1;
            } else {
                eprintln!("Task submission failed: {}", resp.message);
            }
        }
    }
    
    // All tasks should be accepted
    assert_eq!(success_count, 10);
    
    // Cleanup
    server_handle.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cluster_state_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path().to_str().unwrap().to_string();
    let node_port = get_free_port();
    
    // Phase 1: Create node, add some state
    {
        let config = NodeConfig {
            id: 1,
            data_dir: data_dir.clone(),
            bind_addr: format!("127.0.0.1:{}", node_port).parse().unwrap(),
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
        
        // Explicitly drop the node to ensure all resources are released
        drop(node);
    }
    
    // Ensure database is fully released with proper delay
    robust_sleep(Duration::from_millis(500)).await;
    
    // Phase 2: Restart node and verify state
    {
        let config = NodeConfig {
            id: 1,
            data_dir,
            bind_addr: format!("127.0.0.1:{}", node_port).parse().unwrap(),
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_grpc_error_handling() {
    let (mut node, _temp_dir, _node_port) = create_test_node(1).await;
    node.start().await.unwrap();
    
    let node_shared = node.shared();
    
    // Start gRPC server with dynamic port
    let grpc_port = get_free_port();
    let grpc_addr: SocketAddr = format!("127.0.0.1:{}", grpc_port).parse().unwrap();
    let server_handle = tokio::spawn(start_grpc_server(node_shared.clone(), grpc_addr));
    
    // Wait for server to be ready
    wait_for_service_ready(
        || async {
            ClusterServiceClient::connect(format!("http://127.0.0.1:{}", grpc_port))
                .await
                .map(|_| true)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
        },
        "gRPC server",
        Duration::from_secs(3),
    )
    .await
    .unwrap();
    
    // Connect client with retry
    let mut client = connect_with_retry(
        || ClusterServiceClient::connect(format!("http://127.0.0.1:{}", grpc_port)),
        "gRPC client",
    )
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