#![cfg(madsim)]

use madsim::{
    net::NetSim,
    runtime::{Handle, NodeHandle, Runtime},
    time::*,
};
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tonic::{transport::Server, Request, Response, Status};

// Use the simulation crate's proto definitions
use blixard_simulation::{
    cluster_service_client::ClusterServiceClient,
    cluster_service_server::{ClusterService, ClusterServiceServer},
    CreateVmRequest, CreateVmResponse,
    GetVmStatusRequest, GetVmStatusResponse,
    HealthCheckRequest, HealthCheckResponse,
    ClusterStatusRequest, ClusterStatusResponse,
    JoinRequest, JoinResponse,
    LeaveRequest, LeaveResponse,
    ListVmsRequest, ListVmsResponse,
    NodeInfo, NodeState,
    RaftMessageRequest, RaftMessageResponse,
    StartVmRequest, StartVmResponse,
    StopVmRequest, StopVmResponse,
    TaskRequest, TaskResponse,
    TaskStatusRequest, TaskStatusResponse, TaskStatus,
    VmInfo, VmState,
};

/// Simulated Raft state for testing consensus behavior
#[derive(Debug, Clone)]
struct RaftState {
    node_id: u64,
    term: u64,
    leader_id: u64,
    members: HashSet<u64>,
    voted_for: Option<u64>,
    is_leader: bool,
}

/// Simulated VM info
#[derive(Debug, Clone)]
struct SimulatedVm {
    id: String,
    name: String,
    state: VmState,
    node_id: u64,
    config_path: String,
    vcpus: u32,
    memory_mb: u32,
}

/// Test implementation of a Raft consensus node
#[derive(Clone)]
struct TestRaftNode {
    node_id: u64,
    bind_address: String,
    state: Arc<Mutex<RaftState>>,
    vms: Arc<Mutex<HashMap<String, SimulatedVm>>>,
    tasks: Arc<Mutex<HashMap<String, (String, u64)>>>, // task_id -> (status, assigned_node)
}

impl TestRaftNode {
    fn new(node_id: u64, bind_address: String) -> Self {
        let state = RaftState {
            node_id,
            term: 0,
            leader_id: 0,
            members: HashSet::from([node_id]),
            voted_for: None,
            is_leader: false,
        };

        Self {
            node_id,
            bind_address,
            state: Arc::new(Mutex::new(state)),
            vms: Arc::new(Mutex::new(HashMap::new())),
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn elect_as_leader(&self) {
        let mut state = self.state.lock().unwrap();
        state.term += 1;
        state.leader_id = self.node_id;
        state.voted_for = Some(self.node_id);
        state.is_leader = true;
    }

    fn add_peer(&self, node_id: u64) {
        let mut state = self.state.lock().unwrap();
        state.members.insert(node_id);
    }

    fn update_leader(&self, leader_id: u64, term: u64) {
        let mut state = self.state.lock().unwrap();
        if term > state.term {
            state.term = term;
            state.leader_id = leader_id;
            state.is_leader = false;
            state.voted_for = None;
        }
    }
}

#[tonic::async_trait]
impl ClusterService for TestRaftNode {
    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse {
            healthy: true,
            message: format!("Node {} is healthy", self.node_id),
        }))
    }

    async fn join_cluster(
        &self,
        request: Request<JoinRequest>,
    ) -> Result<Response<JoinResponse>, Status> {
        let req = request.into_inner();
        self.add_peer(req.node_id);
        
        let state = self.state.lock().unwrap();
        let peers: Vec<NodeInfo> = state.members.iter().map(|&id| {
            NodeInfo {
                id,
                address: format!("10.0.0.{}:700{}", id, id),
                state: if id == state.leader_id {
                    NodeState::Leader as i32
                } else {
                    NodeState::Follower as i32
                },
            }
        }).collect();

        Ok(Response::new(JoinResponse {
            success: true,
            message: format!("Node {} joined successfully", req.node_id),
            peers,
        }))
    }

    async fn leave_cluster(
        &self,
        _request: Request<LeaveRequest>,
    ) -> Result<Response<LeaveResponse>, Status> {
        Ok(Response::new(LeaveResponse {
            success: true,
            message: "Left cluster".to_string(),
        }))
    }

    async fn get_cluster_status(
        &self,
        _request: Request<ClusterStatusRequest>,
    ) -> Result<Response<ClusterStatusResponse>, Status> {
        let state = self.state.lock().unwrap();
        
        let nodes: Vec<NodeInfo> = state.members.iter().map(|&id| {
            NodeInfo {
                id,
                address: format!("10.0.0.{}:700{}", id, id),
                state: if id == state.leader_id {
                    NodeState::Leader as i32
                } else {
                    NodeState::Follower as i32
                },
            }
        }).collect();

        Ok(Response::new(ClusterStatusResponse {
            leader_id: state.leader_id,
            nodes,
            term: state.term,
        }))
    }

    async fn send_raft_message(
        &self,
        request: Request<RaftMessageRequest>,
    ) -> Result<Response<RaftMessageResponse>, Status> {
        let _req = request.into_inner();
        
        // In real implementation, would deserialize and process raft_data
        
        Ok(Response::new(RaftMessageResponse {
            success: true,
            error: String::new(),
        }))
    }

    async fn submit_task(
        &self,
        request: Request<TaskRequest>,
    ) -> Result<Response<TaskResponse>, Status> {
        let req = request.into_inner();
        let state = self.state.lock().unwrap();
        
        // Only leader can accept tasks
        if !state.is_leader {
            return Err(Status::unavailable(format!(
                "Not the leader. Current leader is node {}", 
                state.leader_id
            )));
        }
        
        drop(state); // Release lock before acquiring tasks lock
        
        // Simulate task assignment
        let assigned_node = (req.task_id.len() as u64 % 3) + 1; // Simple assignment
        self.tasks.lock().unwrap().insert(
            req.task_id.clone(),
            ("running".to_string(), assigned_node)
        );
        
        Ok(Response::new(TaskResponse {
            accepted: true,
            message: "Task accepted".to_string(),
            assigned_node,
        }))
    }

    async fn get_task_status(
        &self,
        request: Request<TaskStatusRequest>,
    ) -> Result<Response<TaskStatusResponse>, Status> {
        let req = request.into_inner();
        let tasks = self.tasks.lock().unwrap();
        
        if let Some((_status, _node_id)) = tasks.get(&req.task_id) {
            Ok(Response::new(TaskStatusResponse {
                found: true,
                status: TaskStatus::Running as i32,
                output: String::new(),
                error: String::new(),
                execution_time_ms: 0,
            }))
        } else {
            Ok(Response::new(TaskStatusResponse {
                found: false,
                status: TaskStatus::Unknown as i32,
                output: String::new(),
                error: String::new(),
                execution_time_ms: 0,
            }))
        }
    }

    async fn create_vm(
        &self,
        request: Request<CreateVmRequest>,
    ) -> Result<Response<CreateVmResponse>, Status> {
        let req = request.into_inner();
        let state = self.state.lock().unwrap();
        
        if !state.is_leader {
            return Err(Status::unavailable("Not the leader"));
        }
        
        drop(state);
        
        let vm_id = format!("vm-{}-{}", self.node_id, req.name);
        let vm = SimulatedVm {
            id: vm_id.clone(),
            name: req.name,
            state: VmState::Created,
            node_id: self.node_id,
            config_path: req.config_path,
            vcpus: req.vcpus,
            memory_mb: req.memory_mb,
        };
        
        self.vms.lock().unwrap().insert(vm_id.clone(), vm);
        
        Ok(Response::new(CreateVmResponse {
            success: true,
            message: "VM created successfully".to_string(),
            vm_id,
        }))
    }

    async fn start_vm(
        &self,
        request: Request<StartVmRequest>,
    ) -> Result<Response<StartVmResponse>, Status> {
        let req = request.into_inner();
        let mut vms = self.vms.lock().unwrap();
        
        // Find VM by name
        let vm_id = vms.iter()
            .find(|(_, vm)| vm.name == req.name)
            .map(|(id, _)| id.clone());
        
        if let Some(id) = vm_id {
            if let Some(vm) = vms.get_mut(&id) {
                vm.state = VmState::Running;
                Ok(Response::new(StartVmResponse {
                    success: true,
                    message: "VM started".to_string(),
                }))
            } else {
                Err(Status::not_found("VM not found"))
            }
        } else {
            Err(Status::not_found("VM not found"))
        }
    }

    async fn stop_vm(
        &self,
        request: Request<StopVmRequest>,
    ) -> Result<Response<StopVmResponse>, Status> {
        let req = request.into_inner();
        let mut vms = self.vms.lock().unwrap();
        
        let vm_id = vms.iter()
            .find(|(_, vm)| vm.name == req.name)
            .map(|(id, _)| id.clone());
        
        if let Some(id) = vm_id {
            if let Some(vm) = vms.get_mut(&id) {
                vm.state = VmState::Stopped;
                Ok(Response::new(StopVmResponse {
                    success: true,
                    message: "VM stopped".to_string(),
                }))
            } else {
                Err(Status::not_found("VM not found"))
            }
        } else {
            Err(Status::not_found("VM not found"))
        }
    }

    async fn list_vms(
        &self,
        _request: Request<ListVmsRequest>,
    ) -> Result<Response<ListVmsResponse>, Status> {
        let vms = self.vms.lock().unwrap();
        
        let vm_infos: Vec<VmInfo> = vms.values().map(|vm| {
            VmInfo {
                name: vm.name.clone(),
                state: vm.state as i32,
                node_id: vm.node_id,
                vcpus: vm.vcpus,
                memory_mb: vm.memory_mb,
            }
        }).collect();

        Ok(Response::new(ListVmsResponse {
            vms: vm_infos,
        }))
    }

    async fn get_vm_status(
        &self,
        request: Request<GetVmStatusRequest>,
    ) -> Result<Response<GetVmStatusResponse>, Status> {
        let req = request.into_inner();
        let vms = self.vms.lock().unwrap();
        
        // Find VM by name instead of ID
        if let Some(vm) = vms.values().find(|v| v.name == req.name) {
            Ok(Response::new(GetVmStatusResponse {
                found: true,
                vm_info: Some(VmInfo {
                    name: vm.name.clone(),
                    state: vm.state as i32,
                    node_id: vm.node_id,
                    vcpus: vm.vcpus,
                    memory_mb: vm.memory_mb,
                }),
            }))
        } else {
            Ok(Response::new(GetVmStatusResponse {
                found: false,
                vm_info: None,
            }))
        }
    }
}

/// Helper to create and start a Raft node with gRPC service
async fn create_raft_node(
    handle: &Handle,
    node_id: u64,
    ip: &str,
    port: u16,
) -> (NodeHandle, SocketAddr, TestRaftNode) {
    let addr: SocketAddr = format!("{}:{}", ip, port).parse().unwrap();
    let bind_address = addr.to_string();
    
    let node_handle = handle
        .create_node()
        .name(format!("raft-node-{}", node_id))
        .ip(ip.parse().unwrap())
        .build();
    
    let service = TestRaftNode::new(node_id, bind_address);
    let service_clone = service.clone();
    
    // Start gRPC server on the node
    node_handle.spawn(async move {
        let server = ClusterServiceServer::new(service_clone);
        
        Server::builder()
            .add_service(server)
            .serve(addr)
            .await
            .unwrap();
    });
    
    (node_handle, addr, service)
}

/// Helper to create a gRPC client
async fn create_client(addr: &str) -> ClusterServiceClient<tonic::transport::Channel> {
    ClusterServiceClient::connect(format!("http://{}", addr))
        .await
        .unwrap()
}

#[madsim::test]
async fn test_single_node_bootstrap() {
    let handle = Handle::current();
    
    // Create a single node
    let (_node, addr, service) = create_raft_node(&handle, 1, "10.0.0.1", 7001).await;
    
    // Give the server time to start
    sleep(Duration::from_secs(1)).await;
    
    // Single node should elect itself as leader
    service.elect_as_leader();
    
    // Create client node to connect
    let client_node = handle.create_node()
        .name("client")
        .ip("10.0.0.2".parse().unwrap())
        .build();
    
    // Run client tests
    client_node.spawn(async move {
        // Verify via gRPC
        let mut client = create_client(&addr.to_string()).await;
        
        let response = client
            .health_check(Request::new(HealthCheckRequest {}))
            .await
            .unwrap();
        assert!(response.into_inner().healthy);
        
        let status = client
            .get_cluster_status(Request::new(ClusterStatusRequest {}))
            .await
            .unwrap()
            .into_inner();
        
        assert_eq!(status.leader_id, 1);
        assert_eq!(status.nodes.len(), 1);
        assert_eq!(status.term, 1);
    }).await.unwrap();
}

#[madsim::test]
async fn test_three_node_leader_election() {
    let handle = Handle::current();
    
    // Create three nodes
    let (_node1, addr1, service1) = create_raft_node(&handle, 1, "10.0.0.1", 7001).await;
    let (_node2, addr2, service2) = create_raft_node(&handle, 2, "10.0.0.2", 7002).await;
    let (_node3, addr3, service3) = create_raft_node(&handle, 3, "10.0.0.3", 7003).await;
    
    sleep(Duration::from_millis(500)).await;
    
    // Node 1 becomes initial leader
    service1.elect_as_leader();
    
    // Update peer lists on all nodes
    service1.add_peer(2);
    service1.add_peer(3);
    service2.add_peer(1);
    service2.add_peer(3);
    service3.add_peer(1);
    service3.add_peer(2);
    
    // Propagate leader info
    service2.update_leader(1, 1);
    service3.update_leader(1, 1);
    
    // Create client node to test cluster operations
    let client_node = handle.create_node()
        .name("client")
        .ip("10.0.0.10".parse().unwrap())
        .build();
    
    let addrs = vec![addr1, addr2, addr3];
    client_node.spawn(async move {
        // Nodes 2 and 3 join the cluster
        let mut client1 = create_client(&addrs[0].to_string()).await;
        
        let join2 = client1
            .join_cluster(Request::new(JoinRequest {
                node_id: 2,
                bind_address: addrs[1].to_string(),
            }))
            .await
            .unwrap();
        assert!(join2.into_inner().success);
        
        let join3 = client1
            .join_cluster(Request::new(JoinRequest {
                node_id: 3,
                bind_address: addrs[2].to_string(),
            }))
            .await
            .unwrap();
        assert!(join3.into_inner().success);
        
        // Verify cluster status from all nodes
        for (_i, addr) in addrs.iter().enumerate() {
            let mut client = create_client(&addr.to_string()).await;
            let status = client
                .get_cluster_status(Request::new(ClusterStatusRequest {}))
                .await
                .unwrap()
                .into_inner();
            
            assert_eq!(status.leader_id, 1);
            assert_eq!(status.nodes.len(), 3);
        }
    }).await.unwrap();
}

#[madsim::test]
async fn test_task_assignment_and_execution() {
    let handle = Handle::current();
    
    // Create a 3-node cluster
    let (_node1, addr1, service1) = create_raft_node(&handle, 1, "10.0.0.1", 7001).await;
    let (_node2, addr2, service2) = create_raft_node(&handle, 2, "10.0.0.2", 7002).await;
    let (_node3, _addr3, service3) = create_raft_node(&handle, 3, "10.0.0.3", 7003).await;
    
    sleep(Duration::from_millis(100)).await;
    
    // Form cluster with node 1 as leader
    service1.elect_as_leader();
    for service in [&service1, &service2, &service3] {
        service.add_peer(1);
        service.add_peer(2);
        service.add_peer(3);
    }
    service2.update_leader(1, 1);
    service3.update_leader(1, 1);
    
    // Create client node to test task operations
    let client_node = handle.create_node()
        .name("client")
        .ip("10.0.0.10".parse().unwrap())
        .build();
    
    let a1 = addr1.clone();
    let a2 = addr2.clone();
    client_node.spawn(async move {
        // Submit task to leader - should succeed
        let mut client1 = create_client(&a1.to_string()).await;
        let task_response = client1
            .submit_task(Request::new(TaskRequest {
                task_id: "test-task-1".to_string(),
                command: "echo".to_string(),
                args: vec!["Hello from task".to_string()],
                cpu_cores: 1,
                memory_mb: 512,
                disk_gb: 1,
                required_features: vec![],
                timeout_secs: 10,
            }))
            .await
            .unwrap();
        
        let resp = task_response.into_inner();
        assert!(resp.accepted);
        assert!(resp.assigned_node > 0);
        
        // Try to submit task to follower - should fail
        let mut client2 = create_client(&a2.to_string()).await;
        let follower_result = client2
            .submit_task(Request::new(TaskRequest {
                task_id: "test-task-2".to_string(),
                command: "echo".to_string(),
                args: vec!["Should fail".to_string()],
                cpu_cores: 1,
                memory_mb: 256,
                disk_gb: 1,
                required_features: vec![],
                timeout_secs: 10,
            }))
            .await;
        
        assert!(follower_result.is_err());
        
        // Check task status
        let status = client1
            .get_task_status(Request::new(TaskStatusRequest {
                task_id: "test-task-1".to_string(),
            }))
            .await
            .unwrap()
            .into_inner();
        
        assert!(status.found);
        assert_eq!(status.status, TaskStatus::Running as i32);
    }).await.unwrap();
}

#[madsim::test]
async fn test_leader_failover() {
    let handle = Handle::current();
    let net = NetSim::current();
    
    // Create a 5-node cluster
    let mut nodes = Vec::new();
    for i in 1..=5 {
        let ip = format!("10.0.0.{}", i);
        let port = 7000 + i as u16;
        let node = create_raft_node(&handle, i as u64, &ip, port).await;
        nodes.push(node);
    }
    
    sleep(Duration::from_millis(500)).await;
    
    // Node 1 is initial leader
    nodes[0].2.elect_as_leader();
    
    // All nodes know about each other
    for (_, _, service) in &nodes {
        for i in 1..=5 {
            service.add_peer(i);
        }
    }
    
    // Propagate leader info
    for i in 1..5 {
        nodes[i].2.update_leader(1, 1);
    }
    
    // Create client node to verify cluster state
    let client_node = handle.create_node()
        .name("client")
        .ip("10.0.0.10".parse().unwrap())
        .build();
    
    let node_addrs = nodes.iter().map(|(_, addr, _)| *addr).collect::<Vec<_>>();
    let initial_term = client_node.spawn(async move {
        // Verify initial state
        let mut client1 = create_client(&node_addrs[0].to_string()).await;
        let initial_status = client1
            .get_cluster_status(Request::new(ClusterStatusRequest {}))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(initial_status.leader_id, 1);
        initial_status.term
    }).await.unwrap();
    
    // Isolate the leader
    net.clog_node(nodes[0].0.id());
    
    // Wait for detection
    sleep(Duration::from_secs(2)).await;
    
    // Node 2 becomes new leader
    nodes[1].2.elect_as_leader();
    for i in 2..5 {
        nodes[i].2.update_leader(2, 2);
    }
    
    // Verify new leader from a different node
    let client_node2 = handle.create_node()
        .name("client2")
        .ip("10.0.0.11".parse().unwrap())
        .build();
    
    let node_addr2 = nodes[2].1;
    client_node2.spawn(async move {
        let mut client3 = create_client(&node_addr2.to_string()).await;
        let new_status = client3
            .get_cluster_status(Request::new(ClusterStatusRequest {}))
            .await
            .unwrap()
            .into_inner();
        
        assert_eq!(new_status.leader_id, 2);
        assert!(new_status.term > initial_term);
    }).await.unwrap();
}

#[madsim::test]
async fn test_network_partition_recovery() {
    let handle = Handle::current();
    let net = NetSim::current();
    
    // Create 5-node cluster
    let mut nodes = Vec::new();
    for i in 1..=5 {
        let ip = format!("10.0.0.{}", i);
        let port = 7000 + i as u16;
        let node = create_raft_node(&handle, i as u64, &ip, port).await;
        nodes.push(node);
    }
    
    sleep(Duration::from_millis(500)).await;
    
    // Node 3 is leader (for interesting partition)
    nodes[2].2.elect_as_leader();
    for (_, _, service) in &nodes {
        for i in 1..=5 {
            service.add_peer(i);
        }
    }
    for i in [0, 1, 3, 4] {
        nodes[i].2.update_leader(3, 1);
    }
    
    // Create partition: nodes 1,2 (minority) vs nodes 3,4,5 (majority)
    for i in 0..2 {
        for j in 2..5 {
            net.clog_link(nodes[i].0.id(), nodes[j].0.id());
        }
    }
    
    sleep(Duration::from_millis(500)).await;
    
    // Minority partition should not accept tasks
    // (In real Raft, they would step down without quorum)
    // For this test, we simulate by having them recognize they lost quorum
    nodes[0].2.update_leader(0, 2); // No leader
    nodes[1].2.update_leader(0, 2);
    
    // Test from client nodes
    let client_node_majority = handle.create_node()
        .name("client-majority")
        .ip("10.0.0.10".parse().unwrap())
        .build();
    
    let majority_addr = nodes[2].1;
    let majority_result = client_node_majority.spawn(async move {
        // Majority partition should still accept tasks
        let mut client3 = create_client(&majority_addr.to_string()).await;
        client3
            .submit_task(Request::new(TaskRequest {
                task_id: "majority-task".to_string(),
                command: "test".to_string(),
                args: vec![],
                cpu_cores: 1,
                memory_mb: 256,
                disk_gb: 1,
                required_features: vec![],
                timeout_secs: 10,
            }))
            .await
    }).await.unwrap();
    
    assert!(majority_result.is_ok());
    
    let client_node_minority = handle.create_node()
        .name("client-minority")
        .ip("10.0.0.11".parse().unwrap())
        .build();
    
    let minority_addr = nodes[0].1;
    let minority_result = client_node_minority.spawn(async move {
        let mut client1 = create_client(&minority_addr.to_string()).await;
        client1
            .submit_task(Request::new(TaskRequest {
                task_id: "minority-task".to_string(),
                command: "test".to_string(),
                args: vec![],
                cpu_cores: 1,
                memory_mb: 256,
                disk_gb: 1,
                required_features: vec![],
                timeout_secs: 10,
            }))
            .await
    }).await.unwrap();
    
    assert!(minority_result.is_err());
    
    // Heal the partition
    for i in 0..2 {
        for j in 2..5 {
            net.unclog_link(nodes[i].0.id(), nodes[j].0.id());
        }
    }
    
    sleep(Duration::from_secs(1)).await;
    
    // Nodes should reconcile
    nodes[0].2.update_leader(3, 2);
    nodes[1].2.update_leader(3, 2);
    
    // Verify cluster is healed - check from a node that was in majority partition
    let client_node_healed = handle.create_node()
        .name("client-healed")
        .ip("10.0.0.12".parse().unwrap())
        .build();
    
    let healed_addr = nodes[3].1;  // Node 3 was in majority partition
    client_node_healed.spawn(async move {
        let mut client1 = create_client(&healed_addr.to_string()).await;
        let status = client1
            .get_cluster_status(Request::new(ClusterStatusRequest {}))
            .await
            .unwrap()
            .into_inner();
        
        assert_eq!(status.nodes.len(), 5);
        assert_eq!(status.leader_id, 3);  // Node 3 should be leader
    }).await.unwrap();
}

#[madsim::test]
async fn test_concurrent_task_submission() {
    use futures::future::join_all;
    
    let handle = Handle::current();
    
    // Create 3-node cluster
    let mut nodes = Vec::new();
    for i in 1..=3 {
        let ip = format!("10.0.0.{}", i);
        let port = 7000 + i as u16;
        let node = create_raft_node(&handle, i as u64, &ip, port).await;
        nodes.push(node);
    }
    
    sleep(Duration::from_millis(100)).await;
    
    // Setup cluster with node 1 as leader
    nodes[0].2.elect_as_leader();
    for (_, _, service) in &nodes {
        for i in 1..=3 {
            service.add_peer(i);
        }
    }
    nodes[1].2.update_leader(1, 1);
    nodes[2].2.update_leader(1, 1);
    
    // Submit many tasks concurrently from different client nodes
    let mut task_futures = Vec::new();
    
    for i in 0..30 {
        let client_node = handle.create_node()
            .name(format!("client-{}", i))
            .ip(format!("10.0.2.{}", i + 1).parse().unwrap())
            .build();
        
        let addr = nodes[i % 3].1;
        let task_id = format!("concurrent-task-{}", i);
        let task_num = i;
        
        let fut = client_node.spawn(async move {
            let mut client = create_client(&addr.to_string()).await;
            client.submit_task(Request::new(TaskRequest {
                task_id,
                command: "echo".to_string(),
                args: vec![format!("Task {}", task_num)],
                cpu_cores: 1,
                memory_mb: 256,
                disk_gb: 1,
                required_features: vec![],
                timeout_secs: 10,
            }))
            .await
        });
        
        task_futures.push(fut);
    }
    
    let results = join_all(task_futures).await;
    
    // Count successful submissions (only to leader should succeed)
    let success_count = results.iter()
        .filter(|r| matches!(r, Ok(Ok(_))))
        .count();
    
    // Approximately 1/3 should succeed (those that went to the leader)
    assert!(success_count >= 8 && success_count <= 12, 
            "Expected ~10 successes, got {}", success_count);
}

#[madsim::test]
async fn test_vm_state_replication() {
    let handle = Handle::current();
    
    // Create 3-node cluster
    let mut nodes = Vec::new();
    for i in 1..=3 {
        let ip = format!("10.0.0.{}", i);
        let port = 7000 + i as u16;
        let node = create_raft_node(&handle, i as u64, &ip, port).await;
        nodes.push(node);
    }
    
    sleep(Duration::from_millis(100)).await;
    
    // Setup cluster
    nodes[0].2.elect_as_leader();
    for (_, _, service) in &nodes {
        for i in 1..=3 {
            service.add_peer(i);
        }
    }
    nodes[1].2.update_leader(1, 1);
    nodes[2].2.update_leader(1, 1);
    
    // Create client node to test VM operations
    let client_node = handle.create_node()
        .name("client")
        .ip("10.0.0.10".parse().unwrap())
        .build();
    
    let leader_addr = nodes[0].1;
    let vm_created = client_node.spawn(async move {
        // Create VM on leader
        let mut client1 = create_client(&leader_addr.to_string()).await;
        let create_response = client1
            .create_vm(Request::new(CreateVmRequest {
                name: "test-vm".to_string(),
                config_path: "/path/to/config".to_string(),
                vcpus: 2,
                memory_mb: 1024,
            }))
            .await
            .unwrap();
        
        assert!(create_response.into_inner().success);
        true
    }).await.unwrap();
    
    assert!(vm_created);
    
    // In real Raft, state would be replicated via log entries
    // For this test, we simulate replication
    let vms = nodes[0].2.vms.lock().unwrap().clone();
    for vm in vms.values() {
        nodes[1].2.vms.lock().unwrap().insert(vm.id.clone(), vm.clone());
        nodes[2].2.vms.lock().unwrap().insert(vm.id.clone(), vm.clone());
    }
    
    // Verify VM is visible from all nodes
    let client_node2 = handle.create_node()
        .name("client2")
        .ip("10.0.0.11".parse().unwrap())
        .build();
    
    let node_addrs: Vec<_> = nodes.iter().map(|(_, addr, _)| *addr).collect();
    client_node2.spawn(async move {
        for addr in &node_addrs {
            let mut client = create_client(&addr.to_string()).await;
            let list_response = client
                .list_vms(Request::new(ListVmsRequest {}))
                .await
                .unwrap();
            
            let vms = list_response.into_inner().vms;
            assert_eq!(vms.len(), 1);
            assert_eq!(vms[0].name, "test-vm");
        }
    }).await.unwrap();
}

#[madsim::test]
async fn test_packet_loss_resilience() {
    // Create runtime with packet loss
    let mut config = madsim::Config::default();
    config.net.packet_loss_rate = 0.05; // 5% packet loss
    config.net.send_latency = Duration::from_millis(1)..Duration::from_millis(10);
    
    let runtime = Runtime::with_seed_and_config(12345, config);
    
    runtime.block_on(async {
        let handle = Handle::current();
        
        // Create 3-node cluster
        let mut nodes = Vec::new();
        for i in 1..=3 {
            let ip = format!("10.0.0.{}", i);
            let port = 7000 + i as u16;
            let node = create_raft_node(&handle, i as u64, &ip, port).await;
            nodes.push(node);
        }
        
        sleep(Duration::from_millis(500)).await;
        
        // Setup cluster with retries
        nodes[0].2.elect_as_leader();
        for (_, _, service) in &nodes {
            for i in 1..=3 {
                service.add_peer(i);
            }
        }
        
        // Try to submit tasks despite packet loss
        let mut successful = 0;
        let mut attempts = 0;
        
        while successful < 5 && attempts < 10 {
            let client_node = handle.create_node()
                .name(format!("client-{}", attempts))
                .ip(format!("10.0.1.{}", attempts).parse().unwrap())
                .build();
            
            let leader_addr = nodes[0].1;
            let task_id = format!("task-{}", attempts);
            let result = client_node.spawn(async move {
                let mut client = create_client(&leader_addr.to_string()).await;
                timeout(
                    Duration::from_millis(500),
                    client.submit_task(Request::new(TaskRequest {
                        task_id,
                        command: "test".to_string(),
                        args: vec![],
                        cpu_cores: 1,
                        memory_mb: 256,
                        disk_gb: 1,
                        required_features: vec![],
                        timeout_secs: 10,
                    }))
                ).await
            }).await;
            
            if let Ok(Ok(Ok(resp))) = result {
                if resp.into_inner().accepted {
                    successful += 1;
                }
            }
            attempts += 1;
            sleep(Duration::from_millis(100)).await;
        }
        
        // Should succeed most of the time despite packet loss
        assert!(successful >= 3, "Only {} tasks succeeded out of {} attempts", successful, attempts);
    });
}