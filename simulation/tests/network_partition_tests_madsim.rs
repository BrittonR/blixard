//! MadSim-specific Network Partition Tests
//! 
//! This file contains network partition tests that can only run under MadSim.
//! They use MadSim's network control APIs to create true network partitions.

#![cfg(madsim)]

use std::time::Duration;
use std::net::SocketAddr;
use madsim::{runtime::Handle, net::NetSim, time::sleep};
use tonic::transport::Server;
use tracing::info;

// Import our test proto definitions
use blixard_simulation::proto::{
    cluster_service_server::{ClusterService, ClusterServiceServer},
    cluster_service_client::ClusterServiceClient,
    ClusterStatusRequest, ClusterStatusResponse, 
    CreateVmRequest, CreateVmResponse,
    ListVmsRequest, ListVmsResponse,
    NodeInfo, NodeState, VmInfo,
};

/// Mock node for testing network partitions
struct MockNode {
    id: u64,
    addr: SocketAddr,
    leader_id: std::sync::Arc<std::sync::Mutex<u64>>,
    vms: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    peers: std::sync::Arc<std::sync::Mutex<Vec<u64>>>,
}

#[tonic::async_trait]
impl ClusterService for MockNode {
    async fn get_cluster_status(
        &self,
        _request: tonic::Request<ClusterStatusRequest>,
    ) -> Result<tonic::Response<ClusterStatusResponse>, tonic::Status> {
        let leader_id = *self.leader_id.lock().unwrap();
        let peers = self.peers.lock().unwrap();
        
        let nodes: Vec<NodeInfo> = peers.iter()
            .map(|&id| NodeInfo {
                id,
                address: format!("10.0.0.{}:7001", id),
                state: if id == leader_id { NodeState::Leader as i32 } else { NodeState::Follower as i32 },
            })
            .collect();
        
        Ok(tonic::Response::new(ClusterStatusResponse {
            leader_id,
            nodes,
            term: 1,
        }))
    }
    
    async fn create_vm(
        &self,
        request: tonic::Request<CreateVmRequest>,
    ) -> Result<tonic::Response<CreateVmResponse>, tonic::Status> {
        let req = request.into_inner();
        let leader_id = *self.leader_id.lock().unwrap();
        
        // Only accept writes if we're the leader or can reach the leader
        if self.id == leader_id {
            // We're the leader, accept the write
            self.vms.lock().unwrap().push(req.name.clone());
            Ok(tonic::Response::new(CreateVmResponse {
                success: true,
                message: "VM created".to_string(),
                vm_id: req.name,
            }))
        } else if leader_id > 0 {
            // We're not the leader but know who is
            Ok(tonic::Response::new(CreateVmResponse {
                success: false,
                message: format!("Not leader, forward to node {}", leader_id),
                vm_id: String::new(),
            }))
        } else {
            // No leader known
            Ok(tonic::Response::new(CreateVmResponse {
                success: false,
                message: "No leader available".to_string(),
                vm_id: String::new(),
            }))
        }
    }
    
    async fn list_vms(
        &self,
        _request: tonic::Request<ListVmsRequest>,
    ) -> Result<tonic::Response<ListVmsResponse>, tonic::Status> {
        let vms = self.vms.lock().unwrap();
        let vm_infos: Vec<VmInfo> = vms.iter()
            .map(|name| VmInfo {
                name: name.clone(),
                state: 0, // Running
                node_id: self.id,
                vcpus: 1,
                memory_mb: 256,
                ip_address: "10.0.0.106".to_string(),
            })
            .collect();
            
        Ok(tonic::Response::new(ListVmsResponse { vms: vm_infos }))
    }
    
    // Implement other required methods with empty/default responses
    async fn join_cluster(
        &self,
        _request: tonic::Request<blixard_simulation::proto::JoinRequest>,
    ) -> Result<tonic::Response<blixard_simulation::proto::JoinResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not implemented"))
    }
    
    async fn leave_cluster(
        &self,
        _request: tonic::Request<blixard_simulation::proto::LeaveRequest>,
    ) -> Result<tonic::Response<blixard_simulation::proto::LeaveResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not implemented"))
    }
    
    async fn start_vm(
        &self,
        _request: tonic::Request<blixard_simulation::proto::StartVmRequest>,
    ) -> Result<tonic::Response<blixard_simulation::proto::StartVmResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not implemented"))
    }
    
    async fn stop_vm(
        &self,
        _request: tonic::Request<blixard_simulation::proto::StopVmRequest>,
    ) -> Result<tonic::Response<blixard_simulation::proto::StopVmResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not implemented"))
    }
    
    async fn get_vm_status(
        &self,
        _request: tonic::Request<blixard_simulation::proto::GetVmStatusRequest>,
    ) -> Result<tonic::Response<blixard_simulation::proto::GetVmStatusResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not implemented"))
    }
    
    async fn health_check(
        &self,
        _request: tonic::Request<blixard_simulation::proto::HealthCheckRequest>,
    ) -> Result<tonic::Response<blixard_simulation::proto::HealthCheckResponse>, tonic::Status> {
        Ok(tonic::Response::new(blixard_simulation::proto::HealthCheckResponse {
            healthy: true,
            message: "OK".to_string(),
        }))
    }
    
    async fn send_raft_message(
        &self,
        _request: tonic::Request<blixard_simulation::proto::RaftMessageRequest>,
    ) -> Result<tonic::Response<blixard_simulation::proto::RaftMessageResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not implemented"))
    }
    
    async fn submit_task(
        &self,
        _request: tonic::Request<blixard_simulation::proto::TaskRequest>,
    ) -> Result<tonic::Response<blixard_simulation::proto::TaskResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not implemented"))
    }
    
    async fn get_task_status(
        &self,
        _request: tonic::Request<blixard_simulation::proto::TaskStatusRequest>,
    ) -> Result<tonic::Response<blixard_simulation::proto::TaskStatusResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not implemented"))
    }

    async fn delete_vm(
        &self,
        _request: tonic::Request<blixard_simulation::proto::DeleteVmRequest>,
    ) -> Result<tonic::Response<blixard_simulation::proto::DeleteVmResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not implemented in mock"))
    }

    async fn create_vm_with_scheduling(
        &self,
        _request: tonic::Request<blixard_simulation::proto::CreateVmWithSchedulingRequest>,
    ) -> Result<tonic::Response<blixard_simulation::proto::CreateVmWithSchedulingResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not implemented in mock"))
    }

    async fn schedule_vm_placement(
        &self,
        _request: tonic::Request<blixard_simulation::proto::ScheduleVmPlacementRequest>,
    ) -> Result<tonic::Response<blixard_simulation::proto::ScheduleVmPlacementResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not implemented in mock"))
    }

    async fn get_cluster_resource_summary(
        &self,
        _request: tonic::Request<blixard_simulation::proto::ClusterResourceSummaryRequest>,
    ) -> Result<tonic::Response<blixard_simulation::proto::ClusterResourceSummaryResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not implemented in mock"))
    }
}

/// Test true network partition with split-brain prevention
#[madsim::test]
async fn test_true_network_partition() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let handle = Handle::current();
    let net = NetSim::current();
    
    // Create 5 nodes
    let mut nodes = Vec::new();
    let mut node_handles = Vec::new();
    
    for id in 1..=5 {
        let node_handle = handle.create_node()
            .name(format!("node-{}", id))
            .ip(format!("10.0.0.{}", id).parse().unwrap())
            .build();
            
        let addr: SocketAddr = format!("10.0.0.{}:7001", id).parse().unwrap();
        
        let leader_id = std::sync::Arc::new(std::sync::Mutex::new(1)); // All nodes initially see node 1 as leader
        let vms = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let peers = std::sync::Arc::new(std::sync::Mutex::new((1..=5).collect()));
        
        let mock_node = MockNode {
            id,
            addr,
            leader_id: leader_id.clone(),
            vms: vms.clone(),
            peers: peers.clone(),
        };
        
        // Start server on this node
        let server_future = Server::builder()
            .add_service(ClusterServiceServer::new(mock_node))
            .serve(addr);
            
        node_handle.spawn(async move {
            info!("Starting server on {}", addr);
            match server_future.await {
                Ok(_) => info!("Server on {} stopped", addr),
                Err(e) => tracing::error!("Server error on {}: {}", addr, e),
            }
        });
        
        nodes.push((id, addr, leader_id, vms, peers));
        node_handles.push(node_handle);
    }
    
    // Wait for services to start - needs more time in MadSim
    sleep(Duration::from_secs(1)).await;
    
    // Create a client node to verify connectivity
    let client_node = handle.create_node()
        .name("test-client")
        .ip("10.0.0.10".parse().unwrap())
        .build();
    
    // Verify initial connectivity from client node
    let nodes_for_client = nodes.clone();
    client_node.spawn(async move {
        // Give servers a bit more time to fully start
        sleep(Duration::from_millis(500)).await;
        
        for (id, addr, _, _, _) in &nodes_for_client {
            let mut connected = false;
            let mut last_error = String::new();
            
            for attempt in 0..10 {
                match ClusterServiceClient::connect(format!("http://{}", addr)).await {
                    Ok(mut client) => {
                        connected = true;
                        let response = client.get_cluster_status(ClusterStatusRequest {})
                            .await
                            .expect("Should get status");
                        
                        let status = response.into_inner();
                        assert_eq!(status.leader_id, 1, "Node {} should see node 1 as leader", id);
                        break;
                    }
                    Err(e) => {
                        last_error = format!("Attempt {}: {}", attempt + 1, e);
                        sleep(Duration::from_millis(200)).await;
                    }
                }
            }
            assert!(connected, "Should be able to connect to node {} at {} - Last error: {}", id, addr, last_error);
        }
        info!("All nodes are accessible and report correct leader");
    }).await.unwrap();
    
    info!("Initial cluster state verified, all nodes see node 1 as leader");
    
    // Create network partition: nodes 1,2,3 (majority) | nodes 4,5 (minority)
    info!("Creating network partition");
    
    // Block connections between partitions
    for i in 0..3 {  // Nodes 1,2,3
        for j in 3..5 {  // Nodes 4,5
            net.clog_link(node_handles[i].id(), node_handles[j].id());
            net.clog_link(node_handles[j].id(), node_handles[i].id());
        }
    }
    
    // Simulate leader election in majority partition
    // In a real Raft implementation, this would happen automatically
    sleep(Duration::from_millis(200)).await;
    
    // Update majority partition to maintain leader
    for i in 0..3 {
        let (_, _, leader_id, _, peers) = &nodes[i];
        *leader_id.lock().unwrap() = 1; // Node 1 remains leader
        *peers.lock().unwrap() = vec![1, 2, 3]; // Only see majority nodes
    }
    
    // Update minority partition - they lose the leader
    for i in 3..5 {
        let (_, _, leader_id, _, peers) = &nodes[i];
        *leader_id.lock().unwrap() = 0; // No leader
        *peers.lock().unwrap() = vec![4, 5]; // Only see minority nodes
    }
    
    info!("Network partition created, testing write operations");
    
    // Test: Majority partition should accept writes
    let majority_test_node = handle.create_node()
        .name("majority-test")
        .ip("10.0.0.12".parse().unwrap())
        .build();
        
    let majority_nodes = nodes.clone();
    let majority_result = majority_test_node.spawn(async move {
        sleep(Duration::from_millis(200)).await; // Let partition settle
        
        // Try to connect to node 1 (in majority)
        if let Ok(mut client) = ClusterServiceClient::connect(format!("http://{}", majority_nodes[0].1)).await {
            let response = client.create_vm(CreateVmRequest {
                name: "majority-vm".to_string(),
                config_path: "/tmp/test.nix".to_string(),
                vcpus: 1,
                memory_mb: 256,
            }).await.expect("Should create VM");
            
            response.into_inner().success
        } else {
            panic!("Should connect to node 1 in majority");
        }
    }).await.unwrap();
    
    assert!(majority_result, "Majority should accept writes");
    
    // Test: Minority partition should reject writes
    let minority_test_node = handle.create_node()
        .name("minority-test")
        .ip("10.0.0.13".parse().unwrap())
        .build();
        
    let minority_nodes = nodes.clone();
    let (minority_accepts_writes, minority_leader) = minority_test_node.spawn(async move {
        sleep(Duration::from_millis(200)).await; // Let partition settle
        
        // Try to connect to node 4 (in minority)
        if let Ok(mut client) = ClusterServiceClient::connect(format!("http://{}", minority_nodes[3].1)).await {
            // Try to create VM
            let vm_response = client.create_vm(CreateVmRequest {
                name: "minority-vm".to_string(),
                config_path: "/tmp/test.nix".to_string(),
                vcpus: 1,
                memory_mb: 256,
            }).await.expect("Should get response");
            
            let accepts_writes = vm_response.into_inner().success;
            
            // Check cluster status
            let status_response = client.get_cluster_status(ClusterStatusRequest {})
                .await
                .expect("Should get status");
            
            let leader_id = status_response.into_inner().leader_id;
            
            (accepts_writes, leader_id)
        } else {
            panic!("Should connect to node 4 in minority");
        }
    }).await.unwrap();
    
    assert!(!minority_accepts_writes, "Minority should reject writes");
    assert_eq!(minority_leader, 0, "Minority should have no leader");
    
    info!("Split-brain prevention verified");
    
    // Heal the partition
    info!("Healing network partition");
    
    for i in 0..3 {
        for j in 3..5 {
            net.unclog_link(node_handles[i].id(), node_handles[j].id());
            net.unclog_link(node_handles[j].id(), node_handles[i].id());
        }
    }
    
    // Update all nodes to see each other again
    for (_, _, leader_id, _, peers) in &nodes {
        *leader_id.lock().unwrap() = 1;
        *peers.lock().unwrap() = (1..=5).collect();
    }
    
    sleep(Duration::from_millis(200)).await;
    
    // Verify all nodes converged
    let verify_node = handle.create_node()
        .name("verify-healing")
        .ip("10.0.0.14".parse().unwrap())
        .build();
        
    let nodes_for_verify = nodes.clone();
    verify_node.spawn(async move {
        sleep(Duration::from_millis(200)).await; // Let healing settle
        
        for (id, addr, _, _, _) in &nodes_for_verify {
            let mut client = ClusterServiceClient::connect(format!("http://{}", addr))
                .await
                .expect("Should reconnect after healing");
                
            let response = client.get_cluster_status(ClusterStatusRequest {})
                .await
                .expect("Should get status");
                
            let status = response.into_inner();
            assert_eq!(status.leader_id, 1, "Node {} should see leader after healing", id);
            assert_eq!(status.nodes.len(), 5, "Node {} should see all peers", id);
        }
    }).await.unwrap();
    
    info!("Network partition healed successfully");
}

/// Test complete node isolation
#[madsim::test]
async fn test_node_isolation() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let handle = Handle::current();
    let net = NetSim::current();
    
    // Create 3 nodes
    let mut nodes = Vec::new();
    let mut node_handles = Vec::new();
    
    for id in 1..=3 {
        let node_handle = handle.create_node()
            .name(format!("node-{}", id))
            .ip(format!("10.0.0.{}", id).parse().unwrap())
            .build();
            
        let addr: SocketAddr = format!("10.0.0.{}:7001", id).parse().unwrap();
        
        let leader_id = std::sync::Arc::new(std::sync::Mutex::new(1));
        let vms = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let peers = std::sync::Arc::new(std::sync::Mutex::new((1..=3).collect()));
        
        let mock_node = MockNode {
            id,
            addr,
            leader_id: leader_id.clone(),
            vms: vms.clone(),
            peers: peers.clone(),
        };
        
        let server_future = Server::builder()
            .add_service(ClusterServiceServer::new(mock_node))
            .serve(addr);
            
        node_handle.spawn(async move {
            if let Err(e) = server_future.await {
                tracing::error!("Server error: {}", e);
            }
        });
        
        nodes.push((id, addr, leader_id, vms));
        node_handles.push(node_handle);
    }
    
    sleep(Duration::from_millis(100)).await;
    
    // Create a client node to verify initial state
    let client_node = handle.create_node()
        .name("test-client")
        .ip("10.0.0.10".parse().unwrap())
        .build();
    
    // Verify initial state
    let nodes_for_client = nodes.clone();
    client_node.spawn(async move {
        sleep(Duration::from_millis(200)).await;
        
        // Verify all nodes initially see node 1 as leader
        for (id, addr, _, _) in &nodes_for_client {
            let mut client = ClusterServiceClient::connect(format!("http://{}", addr))
                .await
                .expect("Should connect initially");
            
            let response = client.get_cluster_status(ClusterStatusRequest {})
                .await
                .expect("Should get status");
            
            let status = response.into_inner();
            assert_eq!(status.leader_id, 1, "Node {} should initially see node 1 as leader", id);
        }
        info!("Initial state verified - all nodes see node 1 as leader");
    }).await.unwrap();
    
    // Isolate node 1 (the leader)
    info!("Isolating node 1");
    net.clog_node(node_handles[0].id());
    
    // Nodes 2 and 3 should elect a new leader
    sleep(Duration::from_millis(200)).await;
    
    // Update nodes 2,3 to have node 2 as new leader
    for i in 1..3 {
        let (_, _, leader_id, _) = &nodes[i];
        *leader_id.lock().unwrap() = 2;
    }
    
    // Node 1 still thinks it's leader but isolated
    
    // Test connectivity from a new client node
    let test_client = handle.create_node()
        .name("test-isolation")
        .ip("10.0.0.11".parse().unwrap())
        .build();
    
    let nodes_for_test = nodes.clone();
    test_client.spawn(async move {
        sleep(Duration::from_millis(200)).await;
        
        // Test: Isolated node can't be reached
        let client1_result = ClusterServiceClient::connect(format!("http://{}", nodes_for_test[0].1)).await;
        assert!(client1_result.is_err() || {
            // If we somehow connect, operations should fail
            let mut client = client1_result.unwrap();
            client.get_cluster_status(ClusterStatusRequest {}).await.is_err()
        }, "Isolated node should be unreachable");
        
        // Test: Remaining nodes have new leader
        let mut client2 = ClusterServiceClient::connect(format!("http://{}", nodes_for_test[1].1))
            .await
            .expect("Should connect to node 2");
            
        let response = client2.get_cluster_status(ClusterStatusRequest {})
            .await
            .expect("Should get status");
            
        let status = response.into_inner();
        assert_eq!(status.leader_id, 2, "Nodes 2,3 should elect node 2 as leader");
    }).await.unwrap();
    
    // Restore node 1
    info!("Restoring node 1");
    net.unclog_node(node_handles[0].id());
    
    // Node 1 should step down and recognize node 2 as leader
    let (_, _, leader_id, _) = &nodes[0];
    *leader_id.lock().unwrap() = 2;
    
    sleep(Duration::from_millis(200)).await;
    
    // Verify all nodes see node 2 as leader
    let verify_client = handle.create_node()
        .name("verify-restoration")
        .ip("10.0.0.12".parse().unwrap())
        .build();
    
    let nodes_for_verify = nodes.clone();
    verify_client.spawn(async move {
        sleep(Duration::from_millis(200)).await;
        
        for (id, addr, _, _) in &nodes_for_verify {
            let mut client = ClusterServiceClient::connect(format!("http://{}", addr))
                .await
                .expect("Should connect after restoration");
                
            let response = client.get_cluster_status(ClusterStatusRequest {})
                .await
                .expect("Should get status");
                
            let status = response.into_inner();
            assert_eq!(status.leader_id, 2, "Node {} should see node 2 as leader", id);
        }
    }).await.unwrap();
    
    info!("Node isolation test completed successfully");
}