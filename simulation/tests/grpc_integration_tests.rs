//! Integration tests for gRPC server using MadSim
//!
//! This tests the gRPC API by creating a minimal service implementation
//! that can work with madsim-tonic.

#![cfg(madsim)]

use std::net::SocketAddr;
// use std::sync::Arc;  // Currently unused
use std::time::Duration;

use madsim::{runtime::Handle, time::sleep};
use tonic::{transport::Server, Request, Response, Status};

// Import the generated proto types
use blixard_simulation::{
    cluster_service_client::ClusterServiceClient,
    cluster_service_server::{ClusterService, ClusterServiceServer},
    CreateVmRequest, CreateVmResponse,
    ListVmsRequest, ListVmsResponse,
    HealthCheckRequest, HealthCheckResponse,
    ClusterStatusRequest, ClusterStatusResponse,
    JoinRequest, JoinResponse,
    LeaveRequest, LeaveResponse,
    StartVmRequest, StartVmResponse,
    StopVmRequest, StopVmResponse,
    GetVmStatusRequest, GetVmStatusResponse,
    NodeInfo, NodeState,
    RaftMessageRequest, RaftMessageResponse,
    TaskRequest, TaskResponse,
    TaskStatusRequest, TaskStatusResponse, TaskStatus,
};

/// A minimal implementation of ClusterService for testing
#[derive(Debug, Default)]
pub struct TestClusterService {
    node_id: u64,
}

#[tonic::async_trait]
impl ClusterService for TestClusterService {
    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse {
            healthy: true,
            message: format!("Node {} is healthy", self.node_id),
        }))
    }

    async fn create_vm(
        &self,
        request: Request<CreateVmRequest>,
    ) -> Result<Response<CreateVmResponse>, Status> {
        let req = request.into_inner();
        Ok(Response::new(CreateVmResponse {
            success: true,
            message: format!("VM '{}' created", req.name),
            vm_id: req.name,
        }))
    }

    async fn list_vms(
        &self,
        _request: Request<ListVmsRequest>,
    ) -> Result<Response<ListVmsResponse>, Status> {
        Ok(Response::new(ListVmsResponse {
            vms: vec![],
        }))
    }

    async fn get_cluster_status(
        &self,
        _request: Request<ClusterStatusRequest>,
    ) -> Result<Response<ClusterStatusResponse>, Status> {
        Ok(Response::new(ClusterStatusResponse {
            leader_id: self.node_id,
            nodes: vec![NodeInfo {
                id: self.node_id,
                address: "10.0.0.1:7001".to_string(),
                state: NodeState::Leader as i32,
            }],
            term: 1,
        }))
    }

    async fn join_cluster(
        &self,
        _request: Request<JoinRequest>,
    ) -> Result<Response<JoinResponse>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }

    async fn leave_cluster(
        &self,
        _request: Request<LeaveRequest>,
    ) -> Result<Response<LeaveResponse>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }

    async fn start_vm(
        &self,
        _request: Request<StartVmRequest>,
    ) -> Result<Response<StartVmResponse>, Status> {
        Ok(Response::new(StartVmResponse {
            success: true,
            message: "VM started".to_string(),
        }))
    }

    async fn stop_vm(
        &self,
        _request: Request<StopVmRequest>,
    ) -> Result<Response<StopVmResponse>, Status> {
        Ok(Response::new(StopVmResponse {
            success: true,
            message: "VM stopped".to_string(),
        }))
    }

    async fn get_vm_status(
        &self,
        request: Request<GetVmStatusRequest>,
    ) -> Result<Response<GetVmStatusResponse>, Status> {
        let _req = request.into_inner();
        Ok(Response::new(GetVmStatusResponse {
            found: false,
            vm_info: None,
        }))
    }

    async fn send_raft_message(
        &self,
        _request: Request<RaftMessageRequest>,
    ) -> Result<Response<RaftMessageResponse>, Status> {
        Ok(Response::new(RaftMessageResponse {
            success: true,
            error: String::new(),
        }))
    }

    async fn submit_task(
        &self,
        _request: Request<TaskRequest>,
    ) -> Result<Response<TaskResponse>, Status> {
        Ok(Response::new(TaskResponse {
            accepted: true,
            message: "Task accepted".to_string(),
            assigned_node: self.node_id,
        }))
    }

    async fn get_task_status(
        &self,
        _request: Request<TaskStatusRequest>,
    ) -> Result<Response<TaskStatusResponse>, Status> {
        Ok(Response::new(TaskStatusResponse {
            found: false,
            status: TaskStatus::Unknown as i32,
            output: String::new(),
            error: String::new(),
            execution_time_ms: 0,
        }))
    }
}

#[madsim::test]
async fn test_grpc_basic_operations() {
    let handle = Handle::current();
    
    // Create server node
    let server_addr: SocketAddr = "10.0.0.1:7001".parse().unwrap();
    let server_node = handle.create_node()
        .name("server")
        .ip(server_addr.ip())
        .build();
    
    // Start server
    server_node.spawn(async move {
        let service = TestClusterService { node_id: 1 };
        let server = ClusterServiceServer::new(service);
        
        Server::builder()
            .add_service(server)
            .serve(server_addr)
            .await
            .unwrap();
    });
    
    // Give server time to start
    sleep(Duration::from_secs(1)).await;
    
    // Create client node
    let client_node = handle.create_node()
        .name("client")
        .ip("10.0.0.2".parse().unwrap())
        .build();
    
    // Run client tests
    client_node.spawn(async move {
        let mut client = ClusterServiceClient::connect("http://10.0.0.1:7001")
            .await
            .expect("Failed to connect");
        
        // Test health check
        let response = client.health_check(HealthCheckRequest {})
            .await
            .expect("Health check failed");
        assert!(response.into_inner().healthy);
        
        // Test create VM
        let response = client.create_vm(CreateVmRequest {
            name: "test-vm".to_string(),
            config_path: "/path/to/config".to_string(),
            vcpus: 2,
            memory_mb: 1024,
        })
        .await
        .expect("Create VM failed");
        assert!(response.into_inner().success);
        
        // Test list VMs
        let response = client.list_vms(ListVmsRequest {})
            .await
            .expect("List VMs failed");
        assert_eq!(response.into_inner().vms.len(), 0);
        
        // Test cluster status
        let response = client.get_cluster_status(ClusterStatusRequest {})
            .await
            .expect("Get cluster status failed");
        let status = response.into_inner();
        assert_eq!(status.leader_id, 1);
        assert_eq!(status.nodes.len(), 1);
    }).await.unwrap();
}

#[madsim::test]
async fn test_grpc_concurrent_clients() {
    let handle = Handle::current();
    
    let server_addr: SocketAddr = "10.0.0.1:7001".parse().unwrap();
    let server_node = handle.create_node()
        .name("server")
        .ip(server_addr.ip())
        .build();
    
    // Start server
    server_node.spawn(async move {
        let service = TestClusterService { node_id: 1 };
        Server::builder()
            .add_service(ClusterServiceServer::new(service))
            .serve(server_addr)
            .await
            .unwrap();
    });
    
    sleep(Duration::from_secs(1)).await;
    
    // Create multiple clients
    let mut client_handles = vec![];
    for i in 0..5 {
        let client_addr = format!("10.0.0.{}:0", i + 2).parse::<SocketAddr>().unwrap();
        let client_node = handle.create_node()
            .name(&format!("client{}", i))
            .ip(client_addr.ip())
            .build();
        
        let handle = client_node.spawn(async move {
            let mut client = ClusterServiceClient::connect("http://10.0.0.1:7001")
                .await
                .expect("Failed to connect");
            
            // Perform multiple operations
            for j in 0..10 {
                let response = client.health_check(HealthCheckRequest {})
                    .await
                    .unwrap();
                assert!(response.into_inner().healthy);
                
                let response = client.create_vm(CreateVmRequest {
                    name: format!("vm-{}-{}", i, j),
                    config_path: "/config".to_string(),
                    vcpus: 1,
                    memory_mb: 512,
                })
                .await
                .unwrap();
                assert!(response.into_inner().success);
                
                sleep(Duration::from_millis(50)).await;
            }
        });
        client_handles.push(handle);
    }
    
    // Wait for all clients
    for handle in client_handles {
        handle.await.unwrap();
    }
}

#[madsim::test]
async fn test_grpc_network_partition() {
    let handle = Handle::current();
    let net = madsim::net::NetSim::current();
    
    let server_addr: SocketAddr = "10.0.0.1:7001".parse().unwrap();
    let server_node = handle.create_node()
        .name("server")
        .ip(server_addr.ip())
        .build();
    
    let client_node = handle.create_node()
        .name("client")
        .ip("10.0.0.2".parse().unwrap())
        .build();
    
    // Start server
    server_node.spawn(async move {
        let service = TestClusterService { node_id: 1 };
        Server::builder()
            .add_service(ClusterServiceServer::new(service))
            .serve(server_addr)
            .await
            .unwrap();
    });
    
    sleep(Duration::from_secs(1)).await;
    
    // Run client with network issues
    client_node.spawn(async move {
        let mut client = ClusterServiceClient::connect("http://10.0.0.1:7001")
            .await
            .expect("Initial connection failed");
        
        // Verify connection works
        let response = client.health_check(HealthCheckRequest {}).await.unwrap();
        assert!(response.into_inner().healthy);
        
        // Create network partition
        net.clog_node(server_node.id());
        
        // Operations should fail
        let result = client.health_check(HealthCheckRequest {}).await;
        assert!(result.is_err());
        
        // Heal partition
        sleep(Duration::from_secs(1)).await;
        net.unclog_node(server_node.id());
        sleep(Duration::from_secs(1)).await;
        
        // Reconnect and verify
        let mut client = ClusterServiceClient::connect("http://10.0.0.1:7001")
            .await
            .expect("Reconnection failed");
        
        let response = client.health_check(HealthCheckRequest {}).await.unwrap();
        assert!(response.into_inner().healthy);
    }).await.unwrap();
}