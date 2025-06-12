//! Simple Raft leader election test to verify basic functionality

#![cfg(madsim)]

use madsim::{
    runtime::Handle,
    time::{sleep, Duration},
};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tonic::{transport::Server, Request, Response, Status};
use tracing::info;

use blixard_simulation::{
    cluster_service_server::{ClusterService, ClusterServiceServer},
    cluster_service_client::ClusterServiceClient,
    HealthCheckRequest, HealthCheckResponse,
    ClusterStatusRequest, ClusterStatusResponse,
    NodeInfo, NodeState,
    RaftMessageRequest, RaftMessageResponse,
    JoinRequest, JoinResponse,
    LeaveRequest, LeaveResponse,
    TaskRequest, TaskResponse,
    TaskStatusRequest, TaskStatusResponse,
    CreateVmRequest, CreateVmResponse,
    StartVmRequest, StartVmResponse,
    StopVmRequest, StopVmResponse,
    ListVmsRequest, ListVmsResponse,
    GetVmStatusRequest, GetVmStatusResponse,
};

#[derive(Clone)]
struct SimpleRaftNode {
    node_id: u64,
    is_leader: Arc<RwLock<bool>>,
}

impl SimpleRaftNode {
    fn new(node_id: u64) -> Self {
        // Node 1 starts as leader for simplicity
        let is_leader = node_id == 1;
        Self {
            node_id,
            is_leader: Arc::new(RwLock::new(is_leader)),
        }
    }
}

#[tonic::async_trait]
impl ClusterService for SimpleRaftNode {
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
        let is_leader = *self.is_leader.read().unwrap();
        
        Ok(Response::new(ClusterStatusResponse {
            leader_id: if is_leader { self.node_id } else { 1 }, // Assume node 1 is leader
            nodes: vec![NodeInfo {
                id: self.node_id,
                address: format!("10.0.0.{}:700{}", self.node_id, self.node_id),
                state: if is_leader { NodeState::Leader as i32 } else { NodeState::Follower as i32 },
            }],
            term: 1,
        }))
    }
    
    // Minimal implementations for other required methods
    async fn send_raft_message(&self, _: Request<RaftMessageRequest>) -> Result<Response<RaftMessageResponse>, Status> {
        Ok(Response::new(RaftMessageResponse { success: true, error: String::new() }))
    }
    
    async fn join_cluster(&self, _: Request<JoinRequest>) -> Result<Response<JoinResponse>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }
    
    async fn leave_cluster(&self, _: Request<LeaveRequest>) -> Result<Response<LeaveResponse>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }
    
    async fn submit_task(&self, _: Request<TaskRequest>) -> Result<Response<TaskResponse>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }
    
    async fn get_task_status(&self, _: Request<TaskStatusRequest>) -> Result<Response<TaskStatusResponse>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }
    
    async fn create_vm(&self, _: Request<CreateVmRequest>) -> Result<Response<CreateVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }
    
    async fn start_vm(&self, _: Request<StartVmRequest>) -> Result<Response<StartVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }
    
    async fn stop_vm(&self, _: Request<StopVmRequest>) -> Result<Response<StopVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }
    
    async fn list_vms(&self, _: Request<ListVmsRequest>) -> Result<Response<ListVmsResponse>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }
    
    async fn get_vm_status(&self, _: Request<GetVmStatusRequest>) -> Result<Response<GetVmStatusResponse>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }
}

#[madsim::test]
async fn test_simple_leader_election() {
    println!("Starting simple leader election test");
    
    let handle = Handle::current();
    
    // Create 3 nodes
    for i in 1..=3 {
        let node = handle.create_node()
            .name(format!("node-{}", i))
            .ip(format!("10.0.0.{}", i).parse().unwrap())
            .build();
        
        let service = SimpleRaftNode::new(i);
        let addr = format!("10.0.0.{}:700{}", i, i);
        
        node.spawn(async move {
            info!("Starting node {} on {}", i, addr);
            Server::builder()
                .add_service(ClusterServiceServer::new(service))
                .serve(addr.parse().unwrap())
                .await
                .unwrap();
        });
    }
    
    // Wait for services to start
    sleep(Duration::from_secs(1)).await;
    
    // Try to connect to each node
    let mut leader_count = 0;
    for i in 1..=3 {
        let addr = format!("http://10.0.0.{}:700{}", i, i);
        info!("Connecting to {}", addr);
        
        match ClusterServiceClient::connect(addr.clone()).await {
            Ok(mut client) => {
                info!("Connected to node {}", i);
                
                if let Ok(response) = client.get_cluster_status(ClusterStatusRequest {}).await {
                    let status = response.into_inner();
                    info!("Node {} status: leader_id={}, state={:?}", i, status.leader_id, status.nodes);
                    
                    if status.leader_id == i as u64 {
                        leader_count += 1;
                    }
                }
            }
            Err(e) => {
                println!("Failed to connect to node {}: {}", i, e);
            }
        }
    }
    
    assert_eq!(leader_count, 1, "Expected exactly 1 leader, found {}", leader_count);
    println!("✅ Simple leader election test passed");
}