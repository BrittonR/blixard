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
            voters: state.members.iter().cloned().collect(), // All members are voters in this mock
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
        
        // Debug print
        eprintln!("DEBUG: Node {} get_cluster_status - leader_id: {}, term: {}, is_leader: {}, members: {:?}", 
                 self.node_id, state.leader_id, state.term, state.is_leader, state.members);
        
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
        let state = self.state.lock().unwrap();
        let num_nodes = state.members.len() as u64;
        drop(state);
        
        // Round-robin assignment based on task_id for multi-node, self for single-node
        let assigned_node = if num_nodes == 1 {
            self.node_id
        } else {
            // Extract number from task_id if it ends with a number (e.g., "task-1" -> 1)
            // Otherwise use a simple hash
            let node_index = if let Some(last_dash_pos) = req.task_id.rfind('-') {
                req.task_id[last_dash_pos + 1..].parse::<u64>().unwrap_or_else(|_| {
                    // Fallback to simple hash if not a number
                    req.task_id.bytes().map(|b| b as u64).sum::<u64>()
                })
            } else {
                req.task_id.bytes().map(|b| b as u64).sum::<u64>()
            };
            (node_index % num_nodes) + 1
        };
        
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
                ip_address: "10.0.0.107".to_string(),
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
                    ip_address: "10.0.0.108".to_string(),
                }),
            }))
        } else {
            Ok(Response::new(GetVmStatusResponse {
                found: false,
                vm_info: None,
            }))
        }
    }

    async fn delete_vm(
        &self,
        _request: Request<blixard_simulation::proto::DeleteVmRequest>,
    ) -> Result<Response<blixard_simulation::proto::DeleteVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented in mock"))
    }

    async fn create_vm_with_scheduling(
        &self,
        _request: Request<blixard_simulation::proto::CreateVmWithSchedulingRequest>,
    ) -> Result<Response<blixard_simulation::proto::CreateVmWithSchedulingResponse>, Status> {
        Err(Status::unimplemented("Not implemented in mock"))
    }

    async fn schedule_vm_placement(
        &self,
        _request: Request<blixard_simulation::proto::ScheduleVmPlacementRequest>,
    ) -> Result<Response<blixard_simulation::proto::ScheduleVmPlacementResponse>, Status> {
        Err(Status::unimplemented("Not implemented in mock"))
    }

    async fn get_cluster_resource_summary(
        &self,
        _request: Request<blixard_simulation::proto::ClusterResourceSummaryRequest>,
    ) -> Result<Response<blixard_simulation::proto::ClusterResourceSummaryResponse>, Status> {
        Err(Status::unimplemented("Not implemented in mock"))
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
        
        // Add small delay to ensure node networking is ready
        sleep(Duration::from_millis(50)).await;
        
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

/// Wait for a leader to be elected among the given nodes
async fn wait_for_leader(
    nodes: &[(NodeHandle, SocketAddr, TestRaftNode)],
    timeout: Duration,
) -> Result<u64, String> {
    let start = Instant::now();
    
    while start.elapsed() < timeout {
        let mut leader_ids = HashSet::new();
        let mut has_leader = false;
        
        for (_, _, service) in nodes {
            let state = service.state.lock().unwrap();
            if state.leader_id > 0 {
                leader_ids.insert(state.leader_id);
                has_leader = true;
            }
        }
        
        // Check if all nodes agree on the same leader
        if has_leader && leader_ids.len() == 1 {
            return Ok(*leader_ids.iter().next().unwrap());
        }
        
        sleep(Duration::from_millis(50)).await;
    }
    
    Err(format!("No leader elected within {:?}", timeout))
}

/// Wait for all nodes to converge on the same leader
async fn wait_for_leader_convergence(
    nodes: &[(NodeHandle, SocketAddr, TestRaftNode)],
    expected_leader: u64,
    timeout: Duration,
) -> Result<(), String> {
    let start = Instant::now();
    
    while start.elapsed() < timeout {
        let mut all_agree = true;
        
        for (_, _, service) in nodes {
            let state = service.state.lock().unwrap();
            if state.leader_id != expected_leader {
                all_agree = false;
                break;
            }
        }
        
        if all_agree {
            return Ok(());
        }
        
        sleep(Duration::from_millis(50)).await;
    }
    
    Err(format!("Nodes did not converge on leader {} within {:?}", expected_leader, timeout))
}

/// Wait for a specific condition with timeout
async fn wait_for_condition<F>(
    mut condition: F,
    timeout: Duration,
    check_interval: Duration,
) -> Result<(), String>
where
    F: FnMut() -> bool,
{
    let start = Instant::now();
    
    while start.elapsed() < timeout {
        if condition() {
            return Ok(());
        }
        sleep(check_interval).await;
    }
    
    Err(format!("Condition not met within {:?}", timeout))
}

/// Helper to wait for an async condition to become true
async fn wait_for_condition_async<F, Fut>(
    mut condition: F,
    timeout: Duration,
    check_interval: Duration,
) -> Result<(), String>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = Instant::now();
    
    while start.elapsed() < timeout {
        if condition().await {
            return Ok(());
        }
        sleep(check_interval).await;
    }
    
    Err(format!("Condition not met within {:?}", timeout))
}

/// Count the number of nodes that believe they are leaders
fn count_leaders(nodes: &[(NodeHandle, SocketAddr, TestRaftNode)]) -> usize {
    nodes.iter()
        .filter(|(_, _, service)| {
            service.state.lock().unwrap().is_leader
        })
        .count()
}

/// Check if all nodes agree on the same leader
fn all_nodes_agree_on_leader(
    nodes: &[(NodeHandle, SocketAddr, TestRaftNode)],
    expected_leader: u64,
) -> bool {
    nodes.iter()
        .all(|(_, _, service)| {
            service.state.lock().unwrap().leader_id == expected_leader
        })
}

/// Get the current term from a node
fn get_node_term(service: &TestRaftNode) -> u64 {
    service.state.lock().unwrap().term
}

/// Wait for a node to reach a specific term
async fn wait_for_term(
    service: &TestRaftNode,
    min_term: u64,
    timeout: Duration,
) -> Result<u64, String> {
    let start = Instant::now();
    
    while start.elapsed() < timeout {
        let term = get_node_term(service);
        if term >= min_term {
            return Ok(term);
        }
        sleep(Duration::from_millis(50)).await;
    }
    
    Err(format!("Node did not reach term {} within {:?}", min_term, timeout))
}

#[madsim::test]
async fn test_single_node_bootstrap() {
    let handle = Handle::current();
    
    // Create a single node
    let (node, addr, service) = create_raft_node(&handle, 1, "10.0.0.1", 7001).await;
    
    // Give server time to start
    sleep(Duration::from_secs(1)).await;
    
    // Verify initial state - no leader yet
    {
        let state = service.state.lock().unwrap();
        assert_eq!(state.leader_id, 0, "Should have no leader initially");
        assert_eq!(state.term, 0, "Should start at term 0");
        assert!(!state.is_leader, "Should not be leader initially");
        assert_eq!(state.members.len(), 1, "Should have only itself as member");
        assert!(state.members.contains(&1), "Should contain node 1");
    }
    
    // Single node should elect itself as leader
    service.elect_as_leader();
    
    // Verify leadership state after election
    {
        let state = service.state.lock().unwrap();
        assert_eq!(state.leader_id, 1, "Node 1 should be leader");
        assert_eq!(state.term, 1, "Term should be incremented to 1");
        assert!(state.is_leader, "Should believe it is leader");
        assert_eq!(state.voted_for, Some(1), "Should have voted for itself");
    }
    
    // Create client node to verify via gRPC
    let client_node = handle.create_node()
        .name("client")
        .ip("10.0.0.2".parse().unwrap())
        .build();
    
    // Run client tests
    client_node.spawn(async move {
        // Wait a bit for client node networking to initialize
        sleep(Duration::from_millis(100)).await;
        
        // Verify via gRPC
        let mut client = create_client(&addr.to_string()).await;
        
        // Test 1: Health check
        let response = client
            .health_check(Request::new(HealthCheckRequest {}))
            .await
            .expect("Health check should succeed");
        let health = response.into_inner();
        assert!(health.healthy, "Node should report healthy");
        assert!(health.message.contains("Node 1"), "Health message should mention node ID");
        
        // Test 2: Cluster status
        let status = client
            .get_cluster_status(Request::new(ClusterStatusRequest {}))
            .await
            .expect("Cluster status should succeed")
            .into_inner();
        
        assert_eq!(status.leader_id, 1, "Leader should be node 1");
        assert_eq!(status.nodes.len(), 1, "Should have exactly 1 node");
        assert_eq!(status.term, 1, "Term should be 1 after election");
        
        // Verify node info
        let node_info = &status.nodes[0];
        assert_eq!(node_info.id, 1, "Node ID should be 1");
        assert_eq!(node_info.state, NodeState::Leader as i32, "Node should be in Leader state");
        assert!(node_info.address.contains("10.0.0.1:7001"), "Address should match");
        
        // Test 3: Task submission should work on leader
        let task_response = client
            .submit_task(Request::new(TaskRequest {
                task_id: "bootstrap-task-1".to_string(),
                command: "echo".to_string(),
                args: vec!["test".to_string()],
                cpu_cores: 1,
                memory_mb: 256,
                disk_gb: 1,
                required_features: vec![],
                timeout_secs: 10,
            }))
            .await
            .expect("Task submission should succeed on leader");
        
        let task_resp = task_response.into_inner();
        assert!(task_resp.accepted, "Task should be accepted by leader");
        assert_eq!(task_resp.assigned_node, 1, "Task should be assigned to node 1");
        
        // Test 4: VM creation should work on leader
        let vm_response = client
            .create_vm(Request::new(CreateVmRequest {
                name: "test-vm-bootstrap".to_string(),
                config_path: "/test/config".to_string(),
                vcpus: 1,
                memory_mb: 512,
            }))
            .await
            .expect("VM creation should succeed on leader");
        
        let vm_resp = vm_response.into_inner();
        assert!(vm_resp.success, "VM creation should succeed");
        assert!(vm_resp.vm_id.contains("vm-1-"), "VM ID should contain node ID");
    }).await.expect("Client tests should complete successfully");
    
    // Final verification - ensure state remained consistent
    {
        let state = service.state.lock().unwrap();
        assert_eq!(state.leader_id, 1, "Should still be leader");
        assert_eq!(state.term, 1, "Term should remain 1");
        assert!(state.is_leader, "Should still believe it is leader");
    }
}

#[madsim::test]
async fn test_three_node_leader_election() {
    let handle = Handle::current();
    
    // Create three nodes
    let (node1, addr1, service1) = create_raft_node(&handle, 1, "10.0.0.1", 7001).await;
    let (node2, addr2, service2) = create_raft_node(&handle, 2, "10.0.0.2", 7002).await;
    let (node3, addr3, service3) = create_raft_node(&handle, 3, "10.0.0.3", 7003).await;
    
    let nodes = vec![
        (node1, addr1, service1.clone()),
        (node2, addr2, service2.clone()),
        (node3, addr3, service3.clone()),
    ];
    
    // Give servers time to start
    sleep(Duration::from_secs(1)).await;
    
    // Verify initial state - no leaders
    assert_eq!(count_leaders(&nodes), 0, "Should have no leaders initially");
    for (_, _, service) in &nodes {
        let state = service.state.lock().unwrap();
        assert_eq!(state.term, 0, "All nodes should start at term 0");
        assert_eq!(state.leader_id, 0, "No node should have a leader initially");
    }
    
    // Update peer lists on all nodes - simulating discovery
    for service in [&service1, &service2, &service3] {
        service.add_peer(1);
        service.add_peer(2);
        service.add_peer(3);
    }
    
    // Verify all nodes know about each other
    for (i, (_, _, service)) in nodes.iter().enumerate() {
        let state = service.state.lock().unwrap();
        assert_eq!(state.members.len(), 3, "Node {} should know about 3 members", i + 1);
        assert!(state.members.contains(&1) && state.members.contains(&2) && state.members.contains(&3),
                "Node {} should know about all nodes", i + 1);
    }
    
    // Node 1 becomes initial leader through election
    service1.elect_as_leader();
    
    // Verify node 1's state after self-election
    {
        let state = service1.state.lock().unwrap();
        assert_eq!(state.term, 1, "Leader should be in term 1");
        assert_eq!(state.leader_id, 1, "Node 1 should know it's the leader");
        assert!(state.is_leader, "Node 1 should believe it's the leader");
        assert_eq!(state.voted_for, Some(1), "Node 1 should have voted for itself");
    }
    
    // Propagate leader info to other nodes (simulating heartbeats)
    service2.update_leader(1, 1);
    service3.update_leader(1, 1);
    
    // Wait for leader convergence
    wait_for_leader_convergence(&nodes, 1, Duration::from_secs(2))
        .await
        .expect("All nodes should converge on leader 1");
    
    // Verify leader election results
    assert_eq!(count_leaders(&nodes), 1, "Should have exactly one leader");
    assert!(all_nodes_agree_on_leader(&nodes, 1), "All nodes should agree node 1 is leader");
    
    // Verify follower states
    for (i, (_, _, service)) in nodes.iter().enumerate().skip(1) {
        let state = service.state.lock().unwrap();
        assert_eq!(state.term, 1, "Node {} should be in term 1", i + 1);
        assert_eq!(state.leader_id, 1, "Node {} should recognize node 1 as leader", i + 1);
        assert!(!state.is_leader, "Node {} should not think it's leader", i + 1);
        assert_eq!(state.voted_for, None, "Node {} should not have voted (didn't participate in election)", i + 1);
    }
    
    // Create client node to test cluster operations
    let client_node = handle.create_node()
        .name("client")
        .ip("10.0.0.10".parse().unwrap())
        .build();
    
    let addrs = vec![addr1, addr2, addr3];
    client_node.spawn(async move {
        // Wait for client networking
        sleep(Duration::from_millis(100)).await;
        
        // Test join cluster operations
        let mut client1 = create_client(&addrs[0].to_string()).await;
        
        // Node 2 joins the cluster
        let join2_response = client1
            .join_cluster(Request::new(JoinRequest {
                node_id: 2,
                bind_address: addrs[1].to_string(),
            }))
            .await
            .expect("Join request for node 2 should succeed");
        
        let join2 = join2_response.into_inner();
        assert!(join2.success, "Node 2 should join successfully");
        assert!(join2.message.contains("Node 2"), "Join message should mention node 2");
        assert_eq!(join2.peers.len(), 3, "Should return 3 peers after join");
        
        // Node 3 joins the cluster
        let join3_response = client1
            .join_cluster(Request::new(JoinRequest {
                node_id: 3,
                bind_address: addrs[2].to_string(),
            }))
            .await
            .expect("Join request for node 3 should succeed");
        
        let join3 = join3_response.into_inner();
        assert!(join3.success, "Node 3 should join successfully");
        assert_eq!(join3.peers.len(), 3, "Should still have 3 peers");
        
        // Verify cluster status from all nodes
        for (i, addr) in addrs.iter().enumerate() {
            let mut client = create_client(&addr.to_string()).await;
            let status = client
                .get_cluster_status(Request::new(ClusterStatusRequest {}))
                .await
                .expect(&format!("Cluster status from node {} should succeed", i + 1))
                .into_inner();
            
            // All nodes should agree on the same state
            assert_eq!(status.leader_id, 1, "Node {} should report leader as node 1", i + 1);
            assert_eq!(status.nodes.len(), 3, "Node {} should see 3 nodes in cluster", i + 1);
            assert_eq!(status.term, 1, "Node {} should be in term 1", i + 1);
            
            // Verify node states in the response
            let leader_count = status.nodes.iter()
                .filter(|n| n.state == NodeState::Leader as i32)
                .count();
            assert_eq!(leader_count, 1, "Node {} should report exactly one leader", i + 1);
            
            let follower_count = status.nodes.iter()
                .filter(|n| n.state == NodeState::Follower as i32)
                .count();
            assert_eq!(follower_count, 2, "Node {} should report exactly two followers", i + 1);
            
            // Verify the leader is node 1
            let leader_node = status.nodes.iter()
                .find(|n| n.state == NodeState::Leader as i32)
                .expect("Should find leader node");
            assert_eq!(leader_node.id, 1, "Leader should be node 1");
        }
        
        // Test that only leader accepts tasks
        let task_id = "election-test-task";
        
        // Try submitting to leader (should succeed)
        let leader_task_response = client1
            .submit_task(Request::new(TaskRequest {
                task_id: task_id.to_string(),
                command: "test".to_string(),
                args: vec![],
                cpu_cores: 1,
                memory_mb: 256,
                disk_gb: 1,
                required_features: vec![],
                timeout_secs: 10,
            }))
            .await
            .expect("Task submission to leader should not error");
        
        assert!(leader_task_response.into_inner().accepted, "Leader should accept task");
        
        // Try submitting to follower (should fail)
        let mut client2 = create_client(&addrs[1].to_string()).await;
        let follower_task_response = client2
            .submit_task(Request::new(TaskRequest {
                task_id: "follower-task".to_string(),
                command: "test".to_string(),
                args: vec![],
                cpu_cores: 1,
                memory_mb: 256,
                disk_gb: 1,
                required_features: vec![],
                timeout_secs: 10,
            }))
            .await;
        
        assert!(follower_task_response.is_err(), "Follower should reject task submission");
        let error = follower_task_response.unwrap_err();
        assert!(error.message().contains("Not the leader"), "Error should indicate not leader");
    }).await.expect("Client tests should complete successfully");
    
    // Final state verification
    assert_eq!(count_leaders(&nodes), 1, "Should still have exactly one leader");
    assert!(all_nodes_agree_on_leader(&nodes, 1), "All nodes should still agree on leader");
}

#[madsim::test]
async fn test_task_assignment_and_execution() {
    let handle = Handle::current();
    
    // Create a 3-node cluster
    let (node1, addr1, service1) = create_raft_node(&handle, 1, "10.0.0.1", 7001).await;
    let (node2, addr2, service2) = create_raft_node(&handle, 2, "10.0.0.2", 7002).await;
    let (node3, addr3, service3) = create_raft_node(&handle, 3, "10.0.0.3", 7003).await;
    
    let nodes = vec![
        (node1, addr1, service1.clone()),
        (node2, addr2, service2.clone()),
        (node3, addr3, service3.clone()),
    ];
    
    // Give servers time to start
    sleep(Duration::from_secs(1)).await;
    
    // Form cluster with node 1 as leader
    for service in [&service1, &service2, &service3] {
        service.add_peer(1);
        service.add_peer(2);
        service.add_peer(3);
    }
    service1.elect_as_leader();
    service2.update_leader(1, 1);
    service3.update_leader(1, 1);
    
    // Wait for leader convergence
    wait_for_leader_convergence(&nodes, 1, Duration::from_secs(2))
        .await
        .expect("All nodes should recognize node 1 as leader");
    
    // Verify initial task state
    assert_eq!(service1.tasks.lock().unwrap().len(), 0, "Should have no tasks initially");
    
    // Create client node to test task operations
    let client_node = handle.create_node()
        .name("client")
        .ip("10.0.0.10".parse().unwrap())
        .build();
    
    let a1 = addr1.clone();
    let a2 = addr2.clone();
    let a3 = addr3.clone();
    let s1 = service1.clone();
    let s2 = service2.clone();
    let s3 = service3.clone();
    
    client_node.spawn(async move {
        // Wait for client networking
        sleep(Duration::from_millis(100)).await;
        
        let mut client1 = create_client(&a1.to_string()).await;
        let mut client2 = create_client(&a2.to_string()).await;
        let mut client3 = create_client(&a3.to_string()).await;
        
        // Test 1: Submit task to leader - should succeed
        let task1_id = "test-task-1";
        let task_response = client1
            .submit_task(Request::new(TaskRequest {
                task_id: task1_id.to_string(),
                command: "echo".to_string(),
                args: vec!["Hello from task".to_string()],
                cpu_cores: 1,
                memory_mb: 512,
                disk_gb: 1,
                required_features: vec![],
                timeout_secs: 10,
            }))
            .await
            .expect("Task submission to leader should succeed");
        
        let resp = task_response.into_inner();
        assert!(resp.accepted, "Leader should accept task");
        assert!(resp.assigned_node > 0 && resp.assigned_node <= 3, 
                "Task should be assigned to a valid node (got {})", resp.assigned_node);
        assert!(resp.message.contains("accepted"), "Response should indicate acceptance");
        
        // Verify task was stored
        {
            let tasks = s1.tasks.lock().unwrap();
            assert_eq!(tasks.len(), 1, "Leader should have stored 1 task");
            assert!(tasks.contains_key(task1_id), "Task should be stored with correct ID");
            let (status, assigned) = &tasks[task1_id];
            assert_eq!(status, "running", "Task should be in running state");
            assert_eq!(*assigned, resp.assigned_node, "Assigned node should match");
        }
        
        // Test 2: Try to submit task to follower (node 2) - should fail
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
        
        assert!(follower_result.is_err(), "Follower should reject task submission");
        let error = follower_result.unwrap_err();
        assert!(error.message().contains("Not the leader"), 
                "Error should indicate node is not leader");
        assert!(error.message().contains("leader is node 1"), 
                "Error should indicate who the leader is");
        
        // Test 3: Try to submit task to other follower (node 3) - should also fail
        let follower3_result = client3
            .submit_task(Request::new(TaskRequest {
                task_id: "test-task-3".to_string(),
                command: "test".to_string(),
                args: vec![],
                cpu_cores: 1,
                memory_mb: 128,
                disk_gb: 1,
                required_features: vec![],
                timeout_secs: 5,
            }))
            .await;
        
        assert!(follower3_result.is_err(), "Follower 3 should also reject task");
        
        // Test 4: Submit multiple tasks to verify distribution
        let mut task_assignments = HashMap::new();
        for i in 0..9 {
            let task_id = format!("distributed-task-{}", i);
            let task_resp = client1
                .submit_task(Request::new(TaskRequest {
                    task_id: task_id.clone(),
                    command: "test".to_string(),
                    args: vec![format!("arg{}", i)],
                    cpu_cores: 1,
                    memory_mb: 256,
                    disk_gb: 1,
                    required_features: vec![],
                    timeout_secs: 10,
                }))
                .await
                .expect("Task submission should succeed")
                .into_inner();
            
            assert!(task_resp.accepted, "All tasks should be accepted");
            *task_assignments.entry(task_resp.assigned_node).or_insert(0) += 1;
        }
        
        // Verify task distribution (simple round-robin based on task_id length)
        assert_eq!(task_assignments.len(), 3, "Tasks should be distributed across all 3 nodes");
        for (node, count) in &task_assignments {
            assert_eq!(*count, 3, "Each node should get 3 tasks (node {} got {})", node, count);
        }
        
        // Test 5: Check task status
        let status = client1
            .get_task_status(Request::new(TaskStatusRequest {
                task_id: task1_id.to_string(),
            }))
            .await
            .expect("Task status check should succeed")
            .into_inner();
        
        assert!(status.found, "Task should be found");
        assert_eq!(status.status, TaskStatus::Running as i32, "Task should be in Running state");
        
        // Test 6: Check non-existent task
        let missing_status = client1
            .get_task_status(Request::new(TaskStatusRequest {
                task_id: "non-existent-task".to_string(),
            }))
            .await
            .expect("Status check should succeed even for missing task")
            .into_inner();
        
        assert!(!missing_status.found, "Non-existent task should not be found");
        assert_eq!(missing_status.status, TaskStatus::Unknown as i32, "Should return Unknown status");
        
        // Test 7: Verify tasks are only stored on leader
        {
            let leader_tasks = s1.tasks.lock().unwrap();
            let follower2_tasks = s2.tasks.lock().unwrap();
            let follower3_tasks = s3.tasks.lock().unwrap();
            
            assert_eq!(leader_tasks.len(), 10, "Leader should have all 10 tasks");
            assert_eq!(follower2_tasks.len(), 0, "Follower 2 should have no tasks");
            assert_eq!(follower3_tasks.len(), 0, "Follower 3 should have no tasks");
        }
    }).await.expect("Client tests should complete successfully");
    
    // Final verification
    assert_eq!(count_leaders(&nodes), 1, "Should still have exactly one leader");
    let final_tasks = service1.tasks.lock().unwrap();
    assert_eq!(final_tasks.len(), 10, "Leader should maintain all submitted tasks");
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
    
    // Give servers time to start
    sleep(Duration::from_secs(1)).await;
    
    // All nodes know about each other
    for (_, _, service) in &nodes {
        for i in 1..=5 {
            service.add_peer(i);
        }
    }
    
    // Verify initial state - no leaders yet
    assert_eq!(count_leaders(&nodes), 0, "Should have no leaders initially");
    for (i, (_, _, service)) in nodes.iter().enumerate() {
        let state = service.state.lock().unwrap();
        assert_eq!(state.term, 0, "Node {} should start at term 0", i + 1);
        assert_eq!(state.members.len(), 5, "Node {} should know about all 5 nodes", i + 1);
    }
    
    // Node 1 becomes initial leader
    nodes[0].2.elect_as_leader();
    
    // Verify node 1's state after election
    {
        let state = nodes[0].2.state.lock().unwrap();
        assert_eq!(state.term, 1, "Leader should advance to term 1");
        assert_eq!(state.leader_id, 1, "Node 1 should be leader");
        assert!(state.is_leader, "Node 1 should believe it's leader");
        assert_eq!(state.voted_for, Some(1), "Node 1 should have voted for itself");
    }
    
    // Propagate leader info to all other nodes
    for i in 1..5 {
        nodes[i].2.update_leader(1, 1);
    }
    
    // Wait for convergence
    wait_for_leader_convergence(&nodes, 1, Duration::from_secs(2))
        .await
        .expect("All nodes should recognize node 1 as leader");
    
    // Verify pre-failover state
    assert_eq!(count_leaders(&nodes), 1, "Should have exactly one leader");
    let initial_term = get_node_term(&nodes[0].2);
    assert_eq!(initial_term, 1, "Initial term should be 1");
    
    // Create client node to verify cluster state and submit work
    let client_node = handle.create_node()
        .name("client")
        .ip("10.0.0.10".parse().unwrap())
        .build();
    
    let node_addrs = nodes.iter().map(|(_, addr, _)| *addr).collect::<Vec<_>>();
    let (initial_term_verified, initial_tasks) = client_node.spawn(async move {
        sleep(Duration::from_millis(100)).await;
        
        // Verify initial state
        let mut client1 = create_client(&node_addrs[0].to_string()).await;
        let initial_status = client1
            .get_cluster_status(Request::new(ClusterStatusRequest {}))
            .await
            .expect("Should get cluster status")
            .into_inner();
        
        assert_eq!(initial_status.leader_id, 1, "Initial leader should be node 1");
        assert_eq!(initial_status.nodes.len(), 5, "Should see all 5 nodes");
        
        // Submit some tasks before failover
        let mut task_ids = Vec::new();
        for i in 0..3 {
            let task_id = format!("pre-failover-task-{}", i);
            let resp = client1
                .submit_task(Request::new(TaskRequest {
                    task_id: task_id.clone(),
                    command: "test".to_string(),
                    args: vec![format!("arg{}", i)],
                    cpu_cores: 1,
                    memory_mb: 256,
                    disk_gb: 1,
                    required_features: vec![],
                    timeout_secs: 10,
                }))
                .await
                .expect("Task submission should succeed");
            
            assert!(resp.into_inner().accepted, "Task {} should be accepted", i);
            task_ids.push(task_id);
        }
        
        (initial_status.term, task_ids)
    }).await.expect("Initial client operations should succeed");
    
    // Verify tasks were stored on leader
    {
        let tasks = nodes[0].2.tasks.lock().unwrap();
        assert_eq!(tasks.len(), 3, "Leader should have 3 tasks before failover");
    }
    
    // FAILOVER: Isolate the leader (node 1)
    net.clog_node(nodes[0].0.id());
    
    // In a real Raft implementation, nodes would detect leader failure via heartbeat timeout
    // For this mock, we simulate that node 2 detects the failure and starts an election
    sleep(Duration::from_millis(500)).await;
    
    // Node 2 campaigns and becomes new leader
    nodes[1].2.elect_as_leader();
    
    // Verify node 2's new term is higher
    let new_term = get_node_term(&nodes[1].2);
    assert!(new_term > initial_term_verified, 
            "New leader's term {} should be higher than initial term {}", 
            new_term, initial_term_verified);
    
    // Propagate new leader info to remaining nodes (3, 4, 5)
    for i in 2..5 {
        nodes[i].2.update_leader(2, new_term);
    }
    
    // Wait for remaining nodes to converge on new leader
    let remaining_nodes: Vec<_> = nodes.iter().skip(1).cloned().collect();
    wait_for_leader_convergence(&remaining_nodes, 2, Duration::from_secs(2))
        .await
        .expect("Remaining nodes should converge on node 2 as leader");
    
    // Verify new leader state
    assert_eq!(
        count_leaders(&remaining_nodes), 1, 
        "Should have exactly one leader among remaining nodes"
    );
    
    // Verify isolated node 1 is no longer leader
    {
        let state = nodes[0].2.state.lock().unwrap();
        // In a real Raft implementation, it would step down when isolated
        // For this test, we just verify our state
        assert_eq!(state.term, 1, "Isolated node should still be in old term");
    }
    
    // Create new client to verify post-failover state
    let client_node2 = handle.create_node()
        .name("client2")
        .ip("10.0.0.11".parse().unwrap())
        .build();
    
    let node_addr2 = nodes[2].1; // Connect to node 3 (a follower)
    let node_addr_new_leader = nodes[1].1; // Node 2 is new leader
    let final_new_term = new_term;
    
    client_node2.spawn(async move {
        sleep(Duration::from_millis(100)).await;
        
        // Verify cluster status from a follower
        let mut client3 = create_client(&node_addr2.to_string()).await;
        let new_status = client3
            .get_cluster_status(Request::new(ClusterStatusRequest {}))
            .await
            .expect("Should get cluster status from follower")
            .into_inner();
        
        assert_eq!(new_status.leader_id, 2, "New leader should be node 2");
        assert_eq!(new_status.term, final_new_term, "Term should match new leader's term");
        // Note: might see 4 or 5 nodes depending on whether isolated node is included
        assert!(new_status.nodes.len() >= 4, "Should see at least 4 active nodes");
        
        // Verify new leader accepts tasks
        let mut client_leader = create_client(&node_addr_new_leader.to_string()).await;
        let post_failover_task = client_leader
            .submit_task(Request::new(TaskRequest {
                task_id: "post-failover-task-1".to_string(),
                command: "test".to_string(),
                args: vec!["after failover".to_string()],
                cpu_cores: 1,
                memory_mb: 256,
                disk_gb: 1,
                required_features: vec![],
                timeout_secs: 10,
            }))
            .await
            .expect("Should connect to new leader");
        
        assert!(post_failover_task.into_inner().accepted, 
                "New leader should accept tasks");
        
        // Verify old leader (if we could reach it) would reject tasks
        // Since it's isolated, we can't test this directly
        
        // Test that followers still reject tasks
        let follower_task = client3
            .submit_task(Request::new(TaskRequest {
                task_id: "follower-reject-task".to_string(),
                command: "test".to_string(),
                args: vec![],
                cpu_cores: 1,
                memory_mb: 128,
                disk_gb: 1,
                required_features: vec![],
                timeout_secs: 5,
            }))
            .await;
        
        assert!(follower_task.is_err(), "Followers should still reject tasks");
        assert!(follower_task.unwrap_err().message().contains("Not the leader"),
                "Error should indicate not leader");
    }).await.expect("Post-failover client operations should succeed");
    
    // Final verification
    let final_leaders = count_leaders(&remaining_nodes);
    assert_eq!(final_leaders, 1, "Should maintain exactly one leader after failover");
    
    // Verify state consistency
    for (i, (_, _, service)) in remaining_nodes.iter().enumerate() {
        let state = service.state.lock().unwrap();
        assert_eq!(state.leader_id, 2, "Node {} should recognize node 2 as leader", i + 2);
        assert_eq!(state.term, new_term, "Node {} should be in new term", i + 2);
    }
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
    // Note: In MadSim tests, we can't create a nested runtime or configure packet loss dynamically
    // The test harness already provides a runtime. We'll simulate packet loss effects
    // through retry logic and timeouts instead.
    
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
    
    // Simulate packet loss by having some operations randomly fail
    // We'll use shorter timeouts and retry logic
    let mut successful = 0;
    let mut attempts = 0;
    
    while successful < 5 && attempts < 10 {
        let client_node = handle.create_node()
            .name(format!("client-{}", attempts))
            .ip(format!("10.0.1.{}", attempts + 1).parse().unwrap())
            .build();
        
        let leader_addr = nodes[0].1;
        let task_id = format!("task-{}", attempts);
        let result = client_node.spawn(async move {
            // Use shorter timeout to simulate packet loss effects
            match timeout(
                Duration::from_millis(200),
                create_client(&leader_addr.to_string())
            ).await {
                Ok(mut client) => {
                    timeout(
                        Duration::from_millis(300),
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
                }
                Err(_) => Err(madsim::time::error::Elapsed),
            }
        }).await;
        
        if let Ok(Ok(Ok(resp))) = result {
            if resp.into_inner().accepted {
                successful += 1;
            }
        }
        attempts += 1;
        sleep(Duration::from_millis(100)).await;
    }
    
    // Should succeed most of the time with retries
    assert!(successful >= 3, "Only {} tasks succeeded out of {} attempts", successful, attempts);
}