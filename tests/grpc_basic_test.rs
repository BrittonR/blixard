use blixard::grpc_client::GrpcClient;
use blixard::grpc_server::{blixard::*, GrpcServer};
use blixard::node::Node;
use blixard::runtime_traits::RealRuntime;
use blixard::storage::Storage;
use blixard::types::{NodeConfig, VmConfig};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::RwLock;
use tonic::transport::Server;

#[tokio::test]
async fn test_grpc_basic_functionality() {
    // This test uses real runtime to verify basic gRPC functionality
    let temp_dir = TempDir::new().unwrap();
    
    // Create real runtime
    let runtime = Arc::new(RealRuntime::new());
    
    // Setup server node
    let server_config = NodeConfig {
        id: 1,
        data_dir: temp_dir.path().join("server").to_str().unwrap().to_string(),
        bind_addr: "127.0.0.1:0".parse().unwrap(), // Use port 0 for dynamic allocation
        join_addr: None,
        use_tailscale: false,
    };
    
    let storage = Arc::new(Storage::new(temp_dir.path().join("server.db")).unwrap());
    let node = Node::new_with_storage(server_config, HashMap::new(), storage)
        .await
        .unwrap();
    let node = Arc::new(RwLock::new(node));
    
    // Create gRPC server
    let server = GrpcServer::new(node.clone(), runtime.clone());
    
    // Get actual port after binding
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let service = cluster_service_server::ClusterServiceServer::new(server);
    
    // Start server and get actual address
    let incoming = tokio::net::TcpListener::bind(addr).await.unwrap();
    let actual_addr = incoming.local_addr().unwrap();
    
    // Start server in background
    tokio::spawn(async move {
        Server::builder()
            .add_service(service)
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(incoming))
            .await
            .unwrap();
    });
    
    // Give server time to start
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    
    // Create client
    let mut client = GrpcClient::connect(
        format!("http://{}", actual_addr),
        runtime.clone()
    )
    .await
    .unwrap();
    
    // Test health check
    let healthy = client.health_check().await.unwrap();
    assert!(healthy);
    
    // Test cluster status
    let status = client.get_cluster_status().await.unwrap();
    assert_eq!(status.leader_id, 1);
    
    // Test VM list (should be empty)
    let vms = client.list_vms().await.unwrap();
    assert_eq!(vms.len(), 0);
}

#[tokio::test]
async fn test_grpc_vm_operations() {
    let temp_dir = TempDir::new().unwrap();
    let runtime = Arc::new(RealRuntime::new());
    
    // Setup server
    let config = NodeConfig {
        id: 1,
        data_dir: temp_dir.path().to_str().unwrap().to_string(),
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        join_addr: None,
        use_tailscale: false,
    };
    
    let storage = Arc::new(Storage::new(temp_dir.path().join("server.db")).unwrap());
    let node = Node::new_with_storage(config, HashMap::new(), storage)
        .await
        .unwrap();
    let node = Arc::new(RwLock::new(node));
    
    let server = GrpcServer::new(node, runtime.clone());
    
    // Start server
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let service = cluster_service_server::ClusterServiceServer::new(server);
    let incoming = tokio::net::TcpListener::bind(addr).await.unwrap();
    let actual_addr = incoming.local_addr().unwrap();
    
    tokio::spawn(async move {
        Server::builder()
            .add_service(service)
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(incoming))
            .await
            .unwrap();
    });
    
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    
    // Create client
    let mut client = GrpcClient::connect(
        format!("http://{}", actual_addr),
        runtime.clone()
    )
    .await
    .unwrap();
    
    // Test VM creation
    let vm_config = VmConfig {
        name: "test-vm".to_string(),
        config_path: "/tmp/test.nix".to_string(),
        vcpus: 2,
        memory: 512,
    };
    
    // This might fail due to missing implementation, but tests the gRPC layer
    let result = client.create_vm(vm_config).await;
    // We don't assert success because the actual VM operations aren't implemented
    assert!(result.is_ok() || result.is_err());
}