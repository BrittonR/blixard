//! MadSim tests for the actual gRPC server implementation
//!
//! This file tests the real gRPC server code using MadSim's tonic fork
//! for deterministic simulation testing.

#![cfg(madsim)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use madsim::{runtime::Handle, time::sleep};
use tonic::transport::Server;

// Import our actual gRPC server and types
use blixard::{
    grpc_server::{BlixardGrpcService, start_grpc_server},
    node::Node,
    types::{NodeConfig, VmConfig},
};

// Import the generated proto types from our simulation crate
use blixard_simulation::{
    cluster_service_client::ClusterServiceClient,
    cluster_service_server::ClusterServiceServer,
    CreateVmRequest, CreateVmResponse,
    ListVmsRequest, ListVmsResponse,
    HealthCheckRequest, HealthCheckResponse,
    GetClusterStatusRequest, GetClusterStatusResponse,
};

/// Test basic gRPC server functionality with a single node
#[madsim::test]
async fn test_grpc_server_basic_operations() {
    let handle = Handle::current();
    
    // Create server node
    let server_addr: SocketAddr = "10.0.0.1:7001".parse().unwrap();
    let server_node = handle.create_node()
        .name("server")
        .ip(server_addr.ip())
        .build();
    
    // Create client node
    let client_addr = "10.0.0.2:7002".parse::<SocketAddr>().unwrap();
    let client_node = handle.create_node()
        .name("client")
        .ip(client_addr.ip())
        .build();
    
    // Start server
    server_node.spawn(async move {
        // Create node configuration
        let config = NodeConfig {
            id: 1,
            bind_addr: server_addr,
            data_dir: "/tmp/test".to_string(),
            join_addr: None,
            use_tailscale: false,
        };
        
        // Create data directory
        std::fs::create_dir_all("/tmp/test").expect("Failed to create data dir");
        
        // Create and initialize node
        let mut node = Node::new(config);
        node.initialize().await.expect("Failed to initialize node");
        node.start().await.expect("Failed to start node");
        
        let node_arc = Arc::new(node);
        
        // Start gRPC server
        start_grpc_server(node_arc, server_addr).await
            .expect("Failed to start gRPC server");
    });
    
    // Give server time to start
    sleep(Duration::from_secs(1)).await;
    
    // Run client tests
    client_node.spawn(async move {
        // Connect to server
        let mut client = ClusterServiceClient::connect("http://10.0.0.1:7001")
            .await
            .expect("Failed to connect to gRPC server");
        
        // Test health check
        let request = tonic::Request::new(HealthCheckRequest {});
        let response = client.health_check(request).await
            .expect("Health check failed");
        let health = response.into_inner();
        assert!(health.healthy);
        assert!(health.message.contains("Node 1"));
        
        // Test create VM
        let request = tonic::Request::new(CreateVmRequest {
            name: "test-vm".to_string(),
            config_path: "/path/to/config".to_string(),
            vcpus: 2,
            memory_mb: 1024,
        });
        let response = client.create_vm(request).await
            .expect("Create VM failed");
        assert!(response.into_inner().success);
        
        // Test list VMs (should be empty since VM creation is not implemented)
        let request = tonic::Request::new(ListVmsRequest {});
        let response = client.list_vms(request).await
            .expect("List VMs failed");
        let vms = response.into_inner().vms;
        assert_eq!(vms.len(), 0);
        
        // Test get cluster status (should fail with not implemented)
        let request = tonic::Request::new(GetClusterStatusRequest {});
        let response = client.get_cluster_status(request).await;
        assert!(response.is_err());
        let error = response.unwrap_err();
        assert_eq!(error.code(), tonic::Code::Unimplemented);
    }).await.unwrap();
}

/// Test multiple clients connecting to the same server
#[madsim::test]
async fn test_grpc_server_multiple_clients() {
    let handle = Handle::current();
    
    // Create server node
    let server_addr: SocketAddr = "10.0.0.1:7001".parse().unwrap();
    let server_node = handle.create_node()
        .name("server")
        .ip(server_addr.ip())
        .build();
    
    // Start server
    server_node.spawn(async move {
        let config = NodeConfig {
            id: 1,
            bind_addr: server_addr,
            data_dir: "/tmp/test".to_string(),
            join_addr: None,
            use_tailscale: false,
        };
        
        std::fs::create_dir_all("/tmp/test").unwrap();
        
        let mut node = Node::new(config);
        node.initialize().await.unwrap();
        node.start().await.unwrap();
        
        let node_arc = Arc::new(node);
        start_grpc_server(node_arc, server_addr).await.unwrap();
    });
    
    sleep(Duration::from_secs(1)).await;
    
    // Create multiple clients
    let mut client_handles = vec![];
    for i in 0..5 {
        let client_addr = format!("10.0.0.{}:7002", i + 2).parse::<SocketAddr>().unwrap();
        let client_node = handle.create_node()
            .name(&format!("client{}", i))
            .ip(client_addr.ip())
            .build();
        
        let handle = client_node.spawn(async move {
            let mut client = ClusterServiceClient::connect("http://10.0.0.1:7001")
                .await
                .expect("Failed to connect");
            
            // Each client performs multiple operations
            for j in 0..10 {
                // Health check
                let request = tonic::Request::new(HealthCheckRequest {});
                let response = client.health_check(request).await.unwrap();
                assert!(response.into_inner().healthy);
                
                // Create VM
                let request = tonic::Request::new(CreateVmRequest {
                    name: format!("vm-{}-{}", i, j),
                    config_path: "/path/to/config".to_string(),
                    vcpus: 1,
                    memory_mb: 512,
                });
                client.create_vm(request).await.unwrap();
                
                sleep(Duration::from_millis(100)).await;
            }
        });
        client_handles.push(handle);
    }
    
    // Wait for all clients to complete
    for handle in client_handles {
        handle.await.unwrap();
    }
}

/// Test gRPC server behavior with network failures
#[madsim::test]
async fn test_grpc_server_with_network_issues() {
    let handle = Handle::current();
    let net = madsim::net::NetSim::current();
    
    // Create nodes
    let server_addr: SocketAddr = "10.0.0.1:7001".parse().unwrap();
    let client_addr: SocketAddr = "10.0.0.2:7002".parse().unwrap();
    
    let server_node = handle.create_node()
        .name("server")
        .ip(server_addr.ip())
        .build();
    
    let client_node = handle.create_node()
        .name("client")
        .ip(client_addr.ip())
        .build();
    
    // Start server
    server_node.spawn(async move {
        let config = NodeConfig {
            id: 1,
            bind_addr: server_addr,
            data_dir: "/tmp/test".to_string(),
            join_addr: None,
            use_tailscale: false,
        };
        
        std::fs::create_dir_all("/tmp/test").unwrap();
        
        let mut node = Node::new(config);
        node.initialize().await.unwrap();
        node.start().await.unwrap();
        
        let node_arc = Arc::new(node);
        start_grpc_server(node_arc, server_addr).await.unwrap();
    });
    
    sleep(Duration::from_secs(1)).await;
    
    // Run client with network issues
    client_node.spawn(async move {
        let mut client = ClusterServiceClient::connect("http://10.0.0.1:7001")
            .await
            .expect("Initial connection failed");
        
        // Perform some successful operations
        for _ in 0..3 {
            let request = tonic::Request::new(HealthCheckRequest {});
            client.health_check(request).await.unwrap();
            sleep(Duration::from_millis(200)).await;
        }
        
        // Simulate network partition
        net.clog_node(server_node.id());
        
        // Operations should fail during partition
        let request = tonic::Request::new(HealthCheckRequest {});
        let result = client.health_check(request).await;
        assert!(result.is_err());
        
        // Heal partition
        sleep(Duration::from_secs(2)).await;
        net.unclog_node(server_node.id());
        
        // Need to reconnect after partition
        let mut client = ClusterServiceClient::connect("http://10.0.0.1:7001")
            .await
            .expect("Reconnection failed");
        
        // Operations should succeed again
        let request = tonic::Request::new(HealthCheckRequest {});
        let response = client.health_check(request).await
            .expect("Health check after partition failed");
        assert!(response.into_inner().healthy);
    }).await.unwrap();
}

/// Test gRPC server crash and recovery
#[madsim::test]
async fn test_grpc_server_crash_recovery() {
    let handle = Handle::current();
    
    let server_addr: SocketAddr = "10.0.0.1:7001".parse().unwrap();
    let client_addr: SocketAddr = "10.0.0.2:7002".parse().unwrap();
    
    // Create server node with restart capability
    let server_node = handle.create_node()
        .name("server")
        .ip(server_addr.ip())
        .init(|| async move {
            let config = NodeConfig {
                id: 1,
                bind_addr: server_addr,
                data_dir: "/tmp/test".to_string(),
                join_addr: None,
                use_tailscale: false,
            };
            
            std::fs::create_dir_all("/tmp/test").unwrap();
            
            let mut node = Node::new(config);
            node.initialize().await.unwrap();
            node.start().await.unwrap();
            
            let node_arc = Arc::new(node);
            start_grpc_server(node_arc, server_addr).await.unwrap();
        })
        .build();
    
    let client_node = handle.create_node()
        .name("client")
        .ip(client_addr.ip())
        .build();
    
    sleep(Duration::from_secs(1)).await;
    
    client_node.spawn(async move {
        // Connect and perform operations
        let mut client = ClusterServiceClient::connect("http://10.0.0.1:7001")
            .await.unwrap();
        
        for i in 0..5 {
            let request = tonic::Request::new(HealthCheckRequest {});
            client.health_check(request).await.unwrap();
            
            // Crash server on iteration 2
            if i == 2 {
                handle.kill(server_node.id());
                sleep(Duration::from_secs(1)).await;
                
                // Operations should fail
                let request = tonic::Request::new(HealthCheckRequest {});
                assert!(client.health_check(request).await.is_err());
                
                // Restart server
                handle.restart(server_node.id());
                sleep(Duration::from_secs(2)).await;
                
                // Reconnect
                client = ClusterServiceClient::connect("http://10.0.0.1:7001")
                    .await.unwrap();
            }
            
            sleep(Duration::from_millis(500)).await;
        }
    }).await.unwrap();
}