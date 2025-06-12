//! Raft leader election verification tests
//! 
//! Tests the correctness of leader election in our Raft implementation
//! using the existing proto definitions and MadSim for deterministic testing.

#![cfg(madsim)]

use madsim::{
    net::NetSim,
    runtime::{Handle, NodeHandle},
    time::{sleep, Duration, Instant},
};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tonic::{transport::Server, Request, Response, Status};
use tracing::{info, debug, warn};

use blixard_simulation::{
    cluster_service_server::{ClusterService, ClusterServiceServer},
    cluster_service_client::ClusterServiceClient,
    HealthCheckRequest, HealthCheckResponse,
    RaftMessageRequest, RaftMessageResponse,
    ClusterStatusRequest, ClusterStatusResponse,
    NodeInfo, NodeState,
    TaskRequest, TaskResponse,
    TaskStatusRequest, TaskStatusResponse,
    JoinRequest, JoinResponse,
    LeaveRequest, LeaveResponse,
    CreateVmRequest, CreateVmResponse,
    StartVmRequest, StartVmResponse,
    StopVmRequest, StopVmResponse,
    ListVmsRequest, ListVmsResponse,
    GetVmStatusRequest, GetVmStatusResponse,
};

/// Represents a test Raft node with consensus implementation
#[derive(Clone)]
struct TestRaftNode {
    node_id: u64,
    addr: String,
    
    // Raft state
    state: Arc<RwLock<RaftNodeState>>,
    
    // Peer connections
    peers: Arc<RwLock<HashMap<u64, String>>>,
    
    // Log entries
    log: Arc<RwLock<Vec<LogEntry>>>,
    committed_index: Arc<RwLock<u64>>,
    
    // Leader election state
    current_term: Arc<RwLock<u64>>,
    voted_for: Arc<RwLock<Option<u64>>>,
    leader_id: Arc<RwLock<Option<u64>>>,
    
    // Timers
    election_timeout: Arc<RwLock<Instant>>,
    heartbeat_interval: Duration,
}

#[derive(Debug, Clone, PartialEq)]
enum RaftNodeState {
    Follower,
    Candidate,
    Leader,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct LogEntry {
    index: u64,
    term: u64,
    data: Vec<u8>,
}

// Custom Raft message types encoded in the RaftMessageRequest
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
enum RaftMessageType {
    RequestVote { candidate_id: u64, term: u64 },
    VoteResponse { term: u64, vote_granted: bool },
    Heartbeat { leader_id: u64, term: u64 },
    AppendEntries { leader_id: u64, term: u64, entries: Vec<LogEntry> },
    AppendEntriesResponse { term: u64, success: bool },
}

impl RaftMessageType {
    fn encode(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }
    
    fn decode(data: &[u8]) -> Result<Self, String> {
        bincode::deserialize(data).map_err(|e| e.to_string())
    }
}

impl TestRaftNode {
    fn new(node_id: u64, addr: String, peers: HashMap<u64, String>) -> Self {
        Self {
            node_id,
            addr: addr.clone(),
            state: Arc::new(RwLock::new(RaftNodeState::Follower)),
            peers: Arc::new(RwLock::new(peers)),
            log: Arc::new(RwLock::new(vec![])),
            committed_index: Arc::new(RwLock::new(0)),
            current_term: Arc::new(RwLock::new(0)),
            voted_for: Arc::new(RwLock::new(None)),
            leader_id: Arc::new(RwLock::new(None)),
            election_timeout: Arc::new(RwLock::new(Instant::now() + Duration::from_millis(150))),
            heartbeat_interval: Duration::from_millis(50),
        }
    }
    
    async fn reset_election_timeout(&self) {
        let timeout_ms = 150 + (self.node_id * 50) % 150; // Randomized timeout
        *self.election_timeout.write().unwrap() = Instant::now() + Duration::from_millis(timeout_ms);
    }
    
    async fn start_election(&self) {
        info!("Node {} starting election", self.node_id);
        
        // Increment term and vote for self
        *self.current_term.write().unwrap() += 1;
        *self.voted_for.write().unwrap() = Some(self.node_id);
        *self.state.write().unwrap() = RaftNodeState::Candidate;
        
        let term = *self.current_term.read().unwrap();
        let mut votes = 1u64; // Vote for self
        
        // Request votes from peers
        let peers = self.peers.read().unwrap().clone();
        let total_nodes = peers.len() + 1;
        let majority = (total_nodes / 2) + 1;
        
        for (peer_id, peer_addr) in peers {
            if peer_id == self.node_id {
                continue;
            }
            
            // Send RequestVote RPC
            match ClusterServiceClient::connect(format!("http://{}", peer_addr)).await {
                Ok(mut client) => {
                    let vote_request = RaftMessageType::RequestVote {
                        candidate_id: self.node_id,
                        term,
                    };
                    
                    let request = Request::new(RaftMessageRequest {
                        raft_data: vote_request.encode(),
                    });
                    
                    if let Ok(response) = client.send_raft_message(request).await {
                        let raft_response = response.into_inner();
                        if raft_response.success {
                            // For now, assume vote granted if response succeeded
                            votes += 1;
                            debug!("Node {} received vote from {}", self.node_id, peer_id);
                        }
                    }
                }
                Err(e) => {
                    warn!("Node {} failed to connect to peer {}: {}", self.node_id, peer_id, e);
                }
            }
        }
        
        // Check if we won the election
        if votes >= majority as u64 {
            info!("Node {} elected as leader with {} votes", self.node_id, votes);
            *self.state.write().unwrap() = RaftNodeState::Leader;
            *self.leader_id.write().unwrap() = Some(self.node_id);
            
            // Send initial heartbeat
            self.send_heartbeat().await;
        } else {
            info!("Node {} failed election with {} votes (needed {})", self.node_id, votes, majority);
            *self.state.write().unwrap() = RaftNodeState::Follower;
        }
    }
    
    async fn send_heartbeat(&self) {
        let peers = self.peers.read().unwrap().clone();
        let term = *self.current_term.read().unwrap();
        
        for (peer_id, peer_addr) in peers {
            if peer_id == self.node_id {
                continue;
            }
            
            match ClusterServiceClient::connect(format!("http://{}", peer_addr)).await {
                Ok(mut client) => {
                    let heartbeat = RaftMessageType::Heartbeat {
                        leader_id: self.node_id,
                        term,
                    };
                    
                    let request = Request::new(RaftMessageRequest {
                        raft_data: heartbeat.encode(),
                    });
                    
                    let _ = client.send_raft_message(request).await;
                }
                Err(_) => {}
            }
        }
    }
    
    async fn run_tick(&self) {
        loop {
            sleep(Duration::from_millis(10)).await;
            
            let state = self.state.read().unwrap().clone();
            match state {
                RaftNodeState::Follower | RaftNodeState::Candidate => {
                    // Check election timeout
                    if Instant::now() > *self.election_timeout.read().unwrap() {
                        self.start_election().await;
                        self.reset_election_timeout().await;
                    }
                }
                RaftNodeState::Leader => {
                    // Send periodic heartbeats
                    self.send_heartbeat().await;
                    sleep(self.heartbeat_interval).await;
                }
            }
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
    
    async fn send_raft_message(
        &self,
        request: Request<RaftMessageRequest>,
    ) -> Result<Response<RaftMessageResponse>, Status> {
        let msg_request = request.into_inner();
        
        match RaftMessageType::decode(&msg_request.raft_data) {
            Ok(msg) => {
                match msg {
                    RaftMessageType::RequestVote { candidate_id, term } => {
                        let mut vote_granted = false;
                        
                        // Check term and vote
                        let current_term = *self.current_term.read().unwrap();
                        if term > current_term {
                            *self.current_term.write().unwrap() = term;
                            *self.voted_for.write().unwrap() = None;
                            *self.state.write().unwrap() = RaftNodeState::Follower;
                        }
                        
                        if term == *self.current_term.read().unwrap() {
                            let voted_for = self.voted_for.read().unwrap().clone();
                            if voted_for.is_none() || voted_for == Some(candidate_id) {
                                *self.voted_for.write().unwrap() = Some(candidate_id);
                                vote_granted = true;
                                self.reset_election_timeout().await;
                            }
                        }
                        
                        Ok(Response::new(RaftMessageResponse {
                            success: vote_granted,
                            error: String::new(),
                        }))
                    }
                    RaftMessageType::Heartbeat { leader_id, term } => {
                        // Handle heartbeat from leader
                        if term >= *self.current_term.read().unwrap() {
                            *self.current_term.write().unwrap() = term;
                            *self.state.write().unwrap() = RaftNodeState::Follower;
                            *self.leader_id.write().unwrap() = Some(leader_id);
                            self.reset_election_timeout().await;
                        }
                        
                        Ok(Response::new(RaftMessageResponse {
                            success: true,
                            error: String::new(),
                        }))
                    }
                    RaftMessageType::AppendEntries { leader_id, term, entries: _ } => {
                        // Handle log replication
                        if term >= *self.current_term.read().unwrap() {
                            *self.current_term.write().unwrap() = term;
                            *self.state.write().unwrap() = RaftNodeState::Follower;
                            *self.leader_id.write().unwrap() = Some(leader_id);
                            self.reset_election_timeout().await;
                            
                            // TODO: Implement actual log replication logic
                        }
                        
                        let _response = RaftMessageType::AppendEntriesResponse {
                            term: *self.current_term.read().unwrap(),
                            success: true,
                        };
                        
                        Ok(Response::new(RaftMessageResponse {
                            success: true,
                            error: String::new(),
                        }))
                    }
                    _ => Ok(Response::new(RaftMessageResponse {
                        success: false,
                        error: "Unexpected message type".to_string(),
                    })),
                }
            }
            Err(e) => Ok(Response::new(RaftMessageResponse {
                success: false,
                error: format!("Failed to decode message: {}", e),
            })),
        }
    }
    
    async fn get_cluster_status(
        &self,
        _request: Request<ClusterStatusRequest>,
    ) -> Result<Response<ClusterStatusResponse>, Status> {
        let leader_id = *self.leader_id.read().unwrap();
        let current_term = *self.current_term.read().unwrap();
        let state = self.state.read().unwrap().clone();
        
        let node_state = match state {
            RaftNodeState::Follower => NodeState::Follower,
            RaftNodeState::Candidate => NodeState::Candidate,
            RaftNodeState::Leader => NodeState::Leader,
        };
        
        Ok(Response::new(ClusterStatusResponse {
            leader_id: leader_id.unwrap_or(0),
            nodes: vec![NodeInfo {
                id: self.node_id,
                address: self.addr.clone(),
                state: node_state as i32,
            }],
            term: current_term,
        }))
    }
    
    async fn submit_task(
        &self,
        request: Request<TaskRequest>,
    ) -> Result<Response<TaskResponse>, Status> {
        // Only leader can accept tasks
        if *self.state.read().unwrap() != RaftNodeState::Leader {
            return Ok(Response::new(TaskResponse {
                accepted: false,
                message: "Not the leader".to_string(),
                assigned_node: 0,
            }));
        }
        
        let task = request.into_inner();
        
        // Add to log (simplified - real implementation would replicate)
        let mut log = self.log.write().unwrap();
        let index = log.len() as u64 + 1;
        log.push(LogEntry {
            index,
            term: *self.current_term.read().unwrap(),
            data: task.task_id.as_bytes().to_vec(), // Just use task ID for testing
        });
        
        Ok(Response::new(TaskResponse {
            accepted: true,
            message: "Task accepted".to_string(),
            assigned_node: self.node_id,
        }))
    }
    
    // Implement remaining required methods with simple responses
    async fn join_cluster(&self, _: Request<JoinRequest>) -> Result<Response<JoinResponse>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }
    
    async fn leave_cluster(&self, _: Request<LeaveRequest>) -> Result<Response<LeaveResponse>, Status> {
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

/// Helper to create and start a Raft node
async fn create_raft_node(
    handle: &Handle,
    node_id: u64,
    ip: &str,
    port: u16,
    peers: HashMap<u64, String>,
) -> NodeHandle {
    let addr = format!("{}:{}", ip, port);
    let node = handle.create_node()
        .name(format!("raft-node-{}", node_id))
        .ip(ip.parse().unwrap())
        .build();
    
    let service = TestRaftNode::new(node_id, addr.clone(), peers);
    
    // Clone for the tick loop
    let tick_service = service.clone();
    
    node.spawn(async move {
        // Start the tick loop in background
        tokio::task::spawn(async move {
            tick_service.run_tick().await;
        });
        
        // Start the gRPC server
        Server::builder()
            .add_service(ClusterServiceServer::new(service))
            .serve(addr.parse().unwrap())
            .await
            .unwrap();
    });
    
    node
}

/// Test helper to run scenarios with nice output
async fn run_test<F, Fut>(name: &str, test_fn: F) 
where 
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>
{
    println!("\n🧪 Running: {}", name);
    let start = Instant::now();
    
    match test_fn().await {
        Ok(()) => {
            println!("✅ {} passed in {:?}", name, start.elapsed());
        }
        Err(e) => {
            println!("❌ {} failed: {}", name, e);
            panic!("Test failed: {}", e);
        }
    }
}

#[madsim::test]
async fn test_leader_election_basic() {
    run_test("leader_election_basic", || async {
        let handle = Handle::current();
        
        // Create 3-node cluster
        let mut peers = HashMap::new();
        peers.insert(1, "10.0.0.1:7001".to_string());
        peers.insert(2, "10.0.0.2:7002".to_string());
        peers.insert(3, "10.0.0.3:7003".to_string());
        
        let _node1 = create_raft_node(&handle, 1, "10.0.0.1", 7001, peers.clone()).await;
        let _node2 = create_raft_node(&handle, 2, "10.0.0.2", 7002, peers.clone()).await;
        let _node3 = create_raft_node(&handle, 3, "10.0.0.3", 7003, peers.clone()).await;
        
        // Give services time to start
        sleep(Duration::from_millis(500)).await;
        
        // Wait for leader election
        sleep(Duration::from_secs(2)).await;
        
        // Check that exactly one leader was elected
        let mut leader_count = 0;
        let mut leader_id = None;
        
        for (node_id, addr) in &peers {
            let mut client = ClusterServiceClient::connect(format!("http://{}", addr))
                .await
                .map_err(|e| format!("Failed to connect to node {}: {}", node_id, e))?;
            
            let response = client.get_cluster_status(Request::new(ClusterStatusRequest {}))
                .await
                .map_err(|e| format!("Failed to get cluster status from node {}: {}", node_id, e))?;
            
            let status = response.into_inner();
            if status.leader_id > 0 {
                if leader_id.is_none() {
                    leader_id = Some(status.leader_id);
                } else if leader_id != Some(status.leader_id) {
                    return Err(format!("Multiple leaders detected: {:?} and {}", leader_id, status.leader_id));
                }
                
                // Check if this node thinks it's the leader
                for node_info in &status.nodes {
                    if node_info.id == *node_id && node_info.state == NodeState::Leader as i32 {
                        leader_count += 1;
                    }
                }
            }
        }
        
        if leader_count != 1 {
            return Err(format!("Expected exactly 1 leader, found {}", leader_count));
        }
        
        info!("Leader elected: {:?}", leader_id);
        Ok(())
    }).await;
}

#[madsim::test]
async fn test_leader_election_with_partition() {
    run_test("leader_election_with_partition", || async {
        let handle = Handle::current();
        let net = NetSim::current();
        
        // Create 5-node cluster
        let mut peers = HashMap::new();
        for i in 1..=5 {
            peers.insert(i, format!("10.0.0.{}:700{}", i, i));
        }
        
        let mut nodes = HashMap::new();
        for i in 1..=5 {
            let node = create_raft_node(&handle, i, &format!("10.0.0.{}", i), 7000 + i as u16, peers.clone()).await;
            nodes.insert(i, node);
        }
        
        // Wait for initial leader election
        sleep(Duration::from_secs(2)).await;
        
        // Find the current leader
        let mut current_leader = None;
        for (node_id, addr) in &peers {
            if let Ok(mut client) = ClusterServiceClient::connect(format!("http://{}", addr)).await {
                if let Ok(response) = client.get_cluster_status(Request::new(ClusterStatusRequest {})).await {
                    let status = response.into_inner();
                    
                    // Check if this node is the leader
                    for node_info in &status.nodes {
                        if node_info.id == *node_id && node_info.state == NodeState::Leader as i32 {
                            current_leader = Some(*node_id);
                            break;
                        }
                    }
                }
            }
        }
        
        let leader_id = current_leader.ok_or("No leader found initially")?;
        info!("Initial leader: {}", leader_id);
        
        // Partition the leader from the rest by clogging its network
        if let Some(leader_node) = nodes.get(&leader_id) {
            net.clog_node(leader_node.id());
        } else {
            return Err("Leader node not found in nodes map".to_string());
        }
        
        info!("Partitioned leader {} from the cluster", leader_id);
        
        // Wait for new leader election
        sleep(Duration::from_secs(3)).await;
        
        // Check that a new leader was elected among the remaining nodes
        let mut new_leader = None;
        for (node_id, addr) in &peers {
            if *node_id == leader_id {
                continue; // Skip the partitioned leader
            }
            
            if let Ok(mut client) = ClusterServiceClient::connect(format!("http://{}", addr)).await {
                if let Ok(response) = client.get_cluster_status(Request::new(ClusterStatusRequest {})).await {
                    let status = response.into_inner();
                    if status.leader_id > 0 && status.leader_id != leader_id {
                        new_leader = Some(status.leader_id);
                        break;
                    }
                }
            }
        }
        
        let new_leader_id = new_leader.ok_or("No new leader elected after partition")?;
        info!("New leader elected: {}", new_leader_id);
        
        Ok(())
    }).await;
}

#[madsim::test]
async fn test_task_submission() {
    run_test("task_submission", || async {
        let handle = Handle::current();
        
        // Create 3-node cluster
        let mut peers = HashMap::new();
        peers.insert(1, "10.0.0.1:7001".to_string());
        peers.insert(2, "10.0.0.2:7002".to_string());
        peers.insert(3, "10.0.0.3:7003".to_string());
        
        let _node1 = create_raft_node(&handle, 1, "10.0.0.1", 7001, peers.clone()).await;
        let _node2 = create_raft_node(&handle, 2, "10.0.0.2", 7002, peers.clone()).await;
        let _node3 = create_raft_node(&handle, 3, "10.0.0.3", 7003, peers.clone()).await;
        
        // Wait for leader election
        sleep(Duration::from_secs(2)).await;
        
        // Find the leader
        let mut leader_addr = None;
        for (_node_id, addr) in &peers {
            if let Ok(mut client) = ClusterServiceClient::connect(format!("http://{}", addr)).await {
                // Try to submit a task
                let task_request = Request::new(TaskRequest {
                    task_id: "test-task-1".to_string(),
                    command: "echo".to_string(),
                    args: vec!["hello".to_string()],
                    cpu_cores: 1,
                    memory_mb: 128,
                    disk_gb: 1,
                    required_features: vec![],
                    timeout_secs: 60,
                });
                
                if let Ok(response) = client.submit_task(task_request).await {
                    if response.into_inner().accepted {
                        leader_addr = Some(addr.clone());
                        break;
                    }
                }
            }
        }
        
        let leader = leader_addr.ok_or("No leader found to accept tasks")?;
        info!("Found leader at: {}", leader);
        
        // Submit multiple tasks
        let mut client = ClusterServiceClient::connect(format!("http://{}", leader))
            .await
            .map_err(|e| format!("Failed to connect to leader: {}", e))?;
        
        for i in 1..=5 {
            let request = Request::new(TaskRequest {
                task_id: format!("task-{}", i),
                command: "test".to_string(),
                args: vec![format!("arg-{}", i)],
                cpu_cores: 1,
                memory_mb: 128,
                disk_gb: 1,
                required_features: vec![],
                timeout_secs: 60,
            });
            
            let response = client.submit_task(request)
                .await
                .map_err(|e| format!("Failed to submit task: {}", e))?;
            
            if !response.into_inner().accepted {
                return Err(format!("Task {} not accepted", i));
            }
            
            info!("Task {} accepted", i);
        }
        
        Ok(())
    }).await;
}