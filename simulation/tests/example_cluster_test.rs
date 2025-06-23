//! Example simulation test showing independent testing approach
//!
//! This demonstrates how to test distributed behavior without importing blixard-core

#![cfg(madsim)]

use madsim::net::NetSim;
use madsim::task::spawn;
use madsim::time::{sleep, Duration};
use std::net::SocketAddr;
use tonic::{transport::Server, Request, Response, Status};

use blixard_simulation::proto::{
    cluster_service_server::{ClusterService, ClusterServiceServer},
    HealthCheckRequest, HealthCheckResponse,
    ClusterStatusRequest, ClusterStatusResponse,
};

// Mock implementation of ClusterService for testing
#[derive(Debug, Default)]
struct MockClusterService {
    node_id: u64,
}

#[tonic::async_trait]
impl ClusterService for MockClusterService {
    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse {
            healthy: true,
            message: format!("Node {} is healthy", self.node_id),
        }))
    }

    async fn get_cluster_status(
        &self,
        _request: Request<ClusterStatusRequest>,
    ) -> Result<Response<ClusterStatusResponse>, Status> {
        Ok(Response::new(ClusterStatusResponse {
            leader_id: self.node_id,
            nodes: vec![self.node_id],
            term: 1,
        }))
    }

    // Implement other required methods with minimal functionality
    async fn join_cluster(
        &self,
        _request: Request<blixard_simulation::proto::JoinRequest>,
    ) -> Result<Response<blixard_simulation::proto::JoinResponse>, Status> {
        Err(Status::unimplemented("Not implemented in mock"))
    }

    async fn leave_cluster(
        &self,
        _request: Request<blixard_simulation::proto::LeaveRequest>,
    ) -> Result<Response<blixard_simulation::proto::LeaveResponse>, Status> {
        Err(Status::unimplemented("Not implemented in mock"))
    }

    async fn send_raft_message(
        &self,
        _request: Request<blixard_simulation::proto::RaftMessageRequest>,
    ) -> Result<Response<blixard_simulation::proto::RaftMessageResponse>, Status> {
        Err(Status::unimplemented("Not implemented in mock"))
    }

    async fn submit_task(
        &self,
        _request: Request<blixard_simulation::proto::TaskRequest>,
    ) -> Result<Response<blixard_simulation::proto::TaskResponse>, Status> {
        Err(Status::unimplemented("Not implemented in mock"))
    }

    async fn get_task_status(
        &self,
        _request: Request<blixard_simulation::proto::TaskStatusRequest>,
    ) -> Result<Response<blixard_simulation::proto::TaskStatusResponse>, Status> {
        Err(Status::unimplemented("Not implemented in mock"))
    }

    async fn create_vm(
        &self,
        _request: Request<blixard_simulation::proto::CreateVmRequest>,
    ) -> Result<Response<blixard_simulation::proto::CreateVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented in mock"))
    }

    async fn start_vm(
        &self,
        _request: Request<blixard_simulation::proto::StartVmRequest>,
    ) -> Result<Response<blixard_simulation::proto::StartVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented in mock"))
    }

    async fn stop_vm(
        &self,
        _request: Request<blixard_simulation::proto::StopVmRequest>,
    ) -> Result<Response<blixard_simulation::proto::StopVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented in mock"))
    }

    async fn list_vms(
        &self,
        _request: Request<blixard_simulation::proto::ListVmsRequest>,
    ) -> Result<Response<blixard_simulation::proto::ListVmsResponse>, Status> {
        Err(Status::unimplemented("Not implemented in mock"))
    }

    async fn get_vm_status(
        &self,
        _request: Request<blixard_simulation::proto::GetVmStatusRequest>,
    ) -> Result<Response<blixard_simulation::proto::GetVmStatusResponse>, Status> {
        Err(Status::unimplemented("Not implemented in mock"))
    }
}

#[madsim::test]
async fn test_mock_cluster_health() {
    let net = NetSim::new();
    let handle = madsim::runtime::Handle::current();
    
    // Create nodes with different IPs
    let node1 = handle.create_node()
        .name("node-1")
        .ip("10.0.0.1".parse().unwrap())
        .build();
    
    let node2 = handle.create_node()
        .name("node-2")  
        .ip("10.0.0.2".parse().unwrap())
        .build();
    
    // Start mock service on node 1
    let addr1: SocketAddr = "10.0.0.1:7001".parse().unwrap();
    node1.spawn(async move {
        let service = MockClusterService { node_id: 1 };
        Server::builder()
            .add_service(ClusterServiceServer::new(service))
            .serve(addr1)
            .await
            .unwrap();
    });
    
    // Give service time to start
    sleep(Duration::from_millis(100)).await;
    
    // Connect from node 2 to node 1
    node2.spawn(async move {
        use blixard_simulation::proto::cluster_service_client::ClusterServiceClient;
        
        let mut client = ClusterServiceClient::connect(format!("http://{}", addr1))
            .await
            .expect("Failed to connect");
        
        let response = client.health_check(Request::new(HealthCheckRequest {}))
            .await
            .expect("Health check failed");
        
        assert!(response.into_inner().healthy);
    }).await.unwrap();
}

#[madsim::test]
async fn test_network_partition() {
    // This test would simulate network partitions
    // For now, just a placeholder showing the pattern
    
    let net = NetSim::new();
    let handle = madsim::runtime::Handle::current();
    
    // Create a 3-node cluster
    let nodes: Vec<_> = (1..=3).map(|i| {
        handle.create_node()
            .name(format!("node-{}", i))
            .ip(format!("10.0.0.{}", i).parse().unwrap())
            .build()
    }).collect();
    
    // In a real test, you would:
    // 1. Start services on each node
    // 2. Wait for cluster to form
    // 3. Create network partition
    // 4. Verify split-brain prevention
    // 5. Heal partition
    // 6. Verify convergence
    
    // For now, just verify nodes were created
    assert_eq!(nodes.len(), 3);
}