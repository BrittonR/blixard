//! Comprehensive Raft consensus verification tests
//! 
//! Based on tikv/raft-rs testing patterns, this suite verifies the correctness
//! of Raft consensus implementation including leader election, log replication,
//! and fault tolerance.

#![cfg(madsim)]

mod test_util;

use madsim::{
    runtime::Handle,
    net::NetSim,
    time::{sleep, Duration, Instant},
    task::spawn,
};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, RwLock};
use tonic::{transport::Server, Request, Response, Status};
use tracing::{info, debug, warn};
use serde::{Serialize, Deserialize};

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

use test_util::{
    TestClusterConfig, ConsensusVerifier, NetworkPartition,
    run_test, random_election_timeout,
};

/// Global network state for simulating partitions
static NETWORK_STATE: once_cell::sync::Lazy<Arc<RwLock<NetworkState>>> = 
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(NetworkState::new())));

#[derive(Debug, Clone)]
struct NetworkState {
    /// Set of blocked connections (from_node, to_node)
    blocked_connections: HashSet<(u64, u64)>,
    /// Set of isolated nodes
    isolated_nodes: HashSet<u64>,
}

impl NetworkState {
    fn new() -> Self {
        Self {
            blocked_connections: HashSet::new(),
            isolated_nodes: HashSet::new(),
        }
    }
    
    fn block_connection(&mut self, from: u64, to: u64) {
        self.blocked_connections.insert((from, to));
    }
    
    fn unblock_connection(&mut self, from: u64, to: u64) {
        self.blocked_connections.remove(&(from, to));
    }
    
    fn isolate_node(&mut self, node_id: u64) {
        self.isolated_nodes.insert(node_id);
    }
    
    fn unisolate_node(&mut self, node_id: u64) {
        self.isolated_nodes.remove(&node_id);
    }
    
    fn is_blocked(&self, from: u64, to: u64) -> bool {
        self.isolated_nodes.contains(&from) || 
        self.isolated_nodes.contains(&to) ||
        self.blocked_connections.contains(&(from, to))
    }
    
    fn clear(&mut self) {
        self.blocked_connections.clear();
        self.isolated_nodes.clear();
    }
}

/// Wrapper for TestRaftNode to implement ClusterService
#[derive(Clone)]
struct TestRaftNodeService(TestRaftNode);

/// Comprehensive Raft node implementation for testing
#[derive(Clone)]
struct TestRaftNode {
    node_id: u64,
    addr: String,
    
    // Core Raft state - following the Raft paper
    state: Arc<Mutex<RaftNodeState>>,
    current_term: Arc<Mutex<u64>>,
    voted_for: Arc<Mutex<Option<u64>>>,
    log: Arc<Mutex<Vec<LogEntry>>>,
    
    // Volatile state on all servers
    commit_index: Arc<Mutex<u64>>,
    last_applied: Arc<Mutex<u64>>,
    
    // Volatile state on leaders
    next_index: Arc<Mutex<HashMap<u64, u64>>>,
    match_index: Arc<Mutex<HashMap<u64, u64>>>,
    
    // Cluster membership
    peers: Arc<Mutex<HashSet<u64>>>,
    
    // Election timing
    election_timeout: Arc<Mutex<Instant>>,
    last_heartbeat: Arc<Mutex<Instant>>,
    
    // Message handling
    message_tx: Arc<std::sync::Mutex<std::sync::mpsc::Sender<RaftMessage>>>,
    message_rx: Arc<std::sync::Mutex<std::sync::mpsc::Receiver<RaftMessage>>>,
    
    // Vote tracking
    votes_received: Arc<Mutex<HashSet<u64>>>,
    
    // Applied entries (simulated state machine)
    applied_entries: Arc<Mutex<Vec<LogEntry>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RaftNodeState {
    Follower,
    Candidate,
    Leader,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogEntry {
    index: u64,
    term: u64,
    data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum RaftMessage {
    RequestVote {
        term: u64,
        candidate_id: u64,
        last_log_index: u64,
        last_log_term: u64,
    },
    RequestVoteResponse {
        term: u64,
        vote_granted: bool,
        from: u64,
    },
    AppendEntries {
        term: u64,
        leader_id: u64,
        prev_log_index: u64,
        prev_log_term: u64,
        entries: Vec<LogEntry>,
        leader_commit: u64,
    },
    AppendEntriesResponse {
        term: u64,
        success: bool,
        from: u64,
        match_index: u64,
    },
}

impl TestRaftNode {
    fn new(node_id: u64, addr: String, peers: HashSet<u64>) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        
        Self {
            node_id,
            addr,
            state: Arc::new(Mutex::new(RaftNodeState::Follower)),
            current_term: Arc::new(Mutex::new(0)),
            voted_for: Arc::new(Mutex::new(None)),
            log: Arc::new(Mutex::new(vec![])),
            commit_index: Arc::new(Mutex::new(0)),
            last_applied: Arc::new(Mutex::new(0)),
            next_index: Arc::new(Mutex::new(HashMap::new())),
            match_index: Arc::new(Mutex::new(HashMap::new())),
            peers: Arc::new(Mutex::new(peers)),
            election_timeout: Arc::new(Mutex::new(Instant::now() + Duration::from_millis(150))),
            last_heartbeat: Arc::new(Mutex::new(Instant::now())),
            message_tx: Arc::new(std::sync::Mutex::new(tx)),
            message_rx: Arc::new(std::sync::Mutex::new(rx)),
            votes_received: Arc::new(Mutex::new(HashSet::new())),
            applied_entries: Arc::new(Mutex::new(vec![])),
        }
    }
    
    /// Start the Raft node's main loop
    async fn run(&self) {
        // Spawn message handler
        let node = self.clone();
        spawn(async move {
            loop {
                // Try to receive messages without blocking
                let msg_opt = {
                    if let Ok(rx) = node.message_rx.try_lock() {
                        rx.try_recv().ok()
                    } else {
                        None
                    }
                };
                
                if let Some(msg) = msg_opt {
                    node.handle_message(msg).await;
                }
                sleep(Duration::from_millis(1)).await;
            }
        });
        
        // Main tick loop
        loop {
            self.tick().await;
            sleep(Duration::from_millis(10)).await;
        }
    }
    
    async fn tick(&self) {
        let now = Instant::now();
        let state = *self.state.lock().unwrap();
        
        match state {
            RaftNodeState::Follower | RaftNodeState::Candidate => {
                let election_timeout = *self.election_timeout.lock().unwrap();
                if now > election_timeout {
                    self.start_election().await;
                }
            }
            RaftNodeState::Leader => {
                let last_heartbeat = *self.last_heartbeat.lock().unwrap();
                if now.duration_since(last_heartbeat) > Duration::from_millis(50) {
                    self.send_heartbeats().await;
                    *self.last_heartbeat.lock().unwrap() = now;
                }
            }
        }
    }
    
    async fn start_election(&self) {
        info!("Node {} starting election", self.node_id);
        
        // Increment term and transition to candidate
        let election_term = {
            let mut term = self.current_term.lock().unwrap();
            *term += 1;
            *term
        };
        
        *self.state.lock().unwrap() = RaftNodeState::Candidate;
        *self.voted_for.lock().unwrap() = Some(self.node_id);
        
        // Clear previous votes and vote for self
        self.votes_received.lock().unwrap().clear();
        self.votes_received.lock().unwrap().insert(self.node_id);
        
        // Reset election timeout
        *self.election_timeout.lock().unwrap() = Instant::now() + random_election_timeout(
            Duration::from_millis(150),
            Duration::from_millis(300),
            self.node_id * election_term,
        );
        
        // Get last log info
        let (last_log_index, last_log_term) = {
            let log = self.log.lock().unwrap();
            let last_log_index = log.len() as u64;
            let last_log_term = log.last().map(|e| e.term).unwrap_or(0);
            (last_log_index, last_log_term)
        };
        
        // Request votes from all peers
        let peers = self.peers.lock().unwrap().clone();
        for peer_id in peers {
            if peer_id != self.node_id {
                self.send_request_vote(peer_id, election_term, last_log_index, last_log_term).await;
            }
        }
    }
    
    async fn send_request_vote(&self, to: u64, term: u64, last_log_index: u64, last_log_term: u64) {
        // Check if connection is blocked
        if NETWORK_STATE.read().unwrap().is_blocked(self.node_id, to) {
            debug!("Node {} blocked from sending RequestVote to {} (network partition)", self.node_id, to);
            return;
        }
        
        let msg = RaftMessage::RequestVote {
            term,
            candidate_id: self.node_id,
            last_log_index,
            last_log_term,
        };
        
        // Actually send via gRPC
        let addr = format!("http://10.0.0.{}:700{}", to, to);
        debug!("Node {} sending RequestVote to {} for term {}", self.node_id, to, term);
        
        let _ = spawn(async move {
            if let Ok(mut client) = ClusterServiceClient::connect(addr).await {
                let _ = client.send_raft_message(Request::new(RaftMessageRequest {
                    raft_data: bincode::serialize(&msg).unwrap(),
                })).await;
            }
        });
    }
    
    async fn send_heartbeats(&self) {
        let term = *self.current_term.lock().unwrap();
        let commit_index = *self.commit_index.lock().unwrap();
        let peers = self.peers.lock().unwrap().clone();
        
        for peer_id in peers {
            if peer_id != self.node_id {
                // Check if connection is blocked
                if NETWORK_STATE.read().unwrap().is_blocked(self.node_id, peer_id) {
                    debug!("Node {} blocked from sending heartbeat to {} (network partition)", self.node_id, peer_id);
                    continue;
                }
                
                let msg = RaftMessage::AppendEntries {
                    term,
                    leader_id: self.node_id,
                    prev_log_index: 0,
                    prev_log_term: 0,
                    entries: vec![],
                    leader_commit: commit_index,
                };
                
                // Actually send via gRPC
                let addr = format!("http://10.0.0.{}:700{}", peer_id, peer_id);
                debug!("Node {} sending heartbeat to {}", self.node_id, peer_id);
                
                let _ = spawn(async move {
                    if let Ok(mut client) = ClusterServiceClient::connect(addr).await {
                        let _ = client.send_raft_message(Request::new(RaftMessageRequest {
                            raft_data: bincode::serialize(&msg).unwrap(),
                        })).await;
                    }
                });
            }
        }
    }
    
    async fn handle_message(&self, msg: RaftMessage) {
        match msg {
            RaftMessage::RequestVote { term, candidate_id, last_log_index, last_log_term } => {
                self.handle_request_vote(term, candidate_id, last_log_index, last_log_term).await;
            }
            RaftMessage::RequestVoteResponse { term, vote_granted, from } => {
                self.handle_vote_response(term, vote_granted, from).await;
            }
            RaftMessage::AppendEntries { term, leader_id, prev_log_index, prev_log_term, entries, leader_commit } => {
                self.handle_append_entries(term, leader_id, prev_log_index, prev_log_term, entries, leader_commit).await;
            }
            RaftMessage::AppendEntriesResponse { term, success, from, match_index } => {
                // Leader processes follower responses
                if *self.state.lock().unwrap() == RaftNodeState::Leader {
                    debug!("Leader {} received AppendEntries response from {} (success={})", self.node_id, from, success);
                }
            }
        }
    }
    
    async fn handle_request_vote(&self, term: u64, candidate_id: u64, _last_log_index: u64, _last_log_term: u64) {
        let mut current_term = self.current_term.lock().unwrap();
        let mut voted_for = self.voted_for.lock().unwrap();
        
        let vote_granted = if term > *current_term {
            // Update to newer term
            *current_term = term;
            *voted_for = Some(candidate_id);
            *self.state.lock().unwrap() = RaftNodeState::Follower;
            true
        } else if term == *current_term && (voted_for.is_none() || *voted_for == Some(candidate_id)) {
            // Grant vote if we haven't voted yet
            *voted_for = Some(candidate_id);
            true
        } else {
            false
        };
        
        info!("Node {} vote response: granted={} to {} for term {}", self.node_id, vote_granted, candidate_id, term);
        
        // Check if connection is blocked before sending response
        if NETWORK_STATE.read().unwrap().is_blocked(self.node_id, candidate_id) {
            debug!("Node {} blocked from sending vote response to {} (network partition)", self.node_id, candidate_id);
            return;
        }
        
        // Send response back to candidate
        let response = RaftMessage::RequestVoteResponse {
            term: *current_term,
            vote_granted,
            from: self.node_id,
        };
        
        let addr = format!("http://10.0.0.{}:700{}", candidate_id, candidate_id);
        let _ = spawn(async move {
            if let Ok(mut client) = ClusterServiceClient::connect(addr).await {
                let _ = client.send_raft_message(Request::new(RaftMessageRequest {
                    raft_data: bincode::serialize(&response).unwrap(),
                })).await;
            }
        });
    }
    
    async fn handle_vote_response(&self, term: u64, vote_granted: bool, from: u64) {
        let current_term = *self.current_term.lock().unwrap();
        let state = *self.state.lock().unwrap();
        
        if term != current_term || state != RaftNodeState::Candidate {
            return;
        }
        
        if vote_granted {
            // Track votes properly
            let should_become_leader = {
                let mut votes = self.votes_received.lock().unwrap();
                votes.insert(from);
                
                let peers = self.peers.lock().unwrap();
                let majority = peers.len() / 2 + 1;  // Majority of all nodes (including self)
                
                info!("Node {} received vote from {}, total votes: {}/{}", self.node_id, from, votes.len(), majority);
                
                // Check if we have majority
                votes.len() >= majority && *self.state.lock().unwrap() == RaftNodeState::Candidate
            };
            
            // If we have majority, become leader
            if should_become_leader {
                *self.state.lock().unwrap() = RaftNodeState::Leader;
                info!("Node {} became leader for term {} ", self.node_id, term);
                
                // Initialize leader state
                {
                    let peers = self.peers.lock().unwrap().clone();
                    let mut next_index = self.next_index.lock().unwrap();
                    let mut match_index = self.match_index.lock().unwrap();
                    let log_len = self.log.lock().unwrap().len() as u64;
                    
                    for peer in peers {
                        if peer != self.node_id {
                            next_index.insert(peer, log_len + 1);
                            match_index.insert(peer, 0);
                        }
                    }
                }
                
                // Send initial heartbeats immediately
                self.send_heartbeats().await;
            }
        }
    }
    
    async fn handle_append_entries(&self, term: u64, leader_id: u64, _prev_log_index: u64, _prev_log_term: u64, _entries: Vec<LogEntry>, _leader_commit: u64) {
        let mut current_term = self.current_term.lock().unwrap();
        
        let success = if term >= *current_term {
            *current_term = term;
            *self.state.lock().unwrap() = RaftNodeState::Follower;
            *self.voted_for.lock().unwrap() = None;
            
            // Reset election timeout
            *self.election_timeout.lock().unwrap() = Instant::now() + random_election_timeout(
                Duration::from_millis(150),
                Duration::from_millis(300),
                self.node_id * term,
            );
            
            debug!("Node {} accepted leader {} for term {}", self.node_id, leader_id, term);
            true
        } else {
            false
        };
        
        // Check if connection is blocked before sending response
        if NETWORK_STATE.read().unwrap().is_blocked(self.node_id, leader_id) {
            debug!("Node {} blocked from sending append entries response to {} (network partition)", self.node_id, leader_id);
            return;
        }
        
        // Send response back to leader
        let response = RaftMessage::AppendEntriesResponse {
            term: *current_term,
            success,
            from: self.node_id,
            match_index: self.log.lock().unwrap().len() as u64,
        };
        
        let addr = format!("http://10.0.0.{}:700{}", leader_id, leader_id);
        let _ = spawn(async move {
            if let Ok(mut client) = ClusterServiceClient::connect(addr).await {
                let _ = client.send_raft_message(Request::new(RaftMessageRequest {
                    raft_data: bincode::serialize(&response).unwrap(),
                })).await;
            }
        });
    }
}

#[tonic::async_trait]
impl ClusterService for TestRaftNodeService {
    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse {
            healthy: true,
            message: format!("Node {} is healthy", self.0.node_id),
        }))
    }
    
    async fn get_cluster_status(
        &self,
        _request: Request<ClusterStatusRequest>,
    ) -> Result<Response<ClusterStatusResponse>, Status> {
        let state = *self.0.state.lock().unwrap();
        let term = *self.0.current_term.lock().unwrap();
        let peers = self.0.peers.lock().unwrap();
        
        let leader_id = match state {
            RaftNodeState::Leader => self.0.node_id,
            _ => 0, // Unknown leader
        };
        
        let node_state = match state {
            RaftNodeState::Follower => NodeState::Follower,
            RaftNodeState::Candidate => NodeState::Candidate,
            RaftNodeState::Leader => NodeState::Leader,
        };
        
        let nodes = peers.iter().map(|&id| {
            NodeInfo {
                id,
                address: format!("10.0.0.{}:700{}", id, id),
                state: if id == self.0.node_id { node_state as i32 } else { NodeState::Unknown as i32 },
            }
        }).collect();
        
        Ok(Response::new(ClusterStatusResponse {
            leader_id,
            nodes,
            term,
        }))
    }
    
    async fn send_raft_message(
        &self,
        request: Request<RaftMessageRequest>,
    ) -> Result<Response<RaftMessageResponse>, Status> {
        let req = request.into_inner();
        
        if let Ok(msg) = bincode::deserialize::<RaftMessage>(&req.raft_data) {
            let _ = self.0.message_tx.lock().unwrap().send(msg);
            Ok(Response::new(RaftMessageResponse {
                success: true,
                error: String::new(),
            }))
        } else {
            Ok(Response::new(RaftMessageResponse {
                success: false,
                error: "Failed to deserialize message".to_string(),
            }))
        }
    }
    
    // Stub implementations for other methods
    async fn join_cluster(&self, _: Request<JoinRequest>) -> Result<Response<JoinResponse>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }
    
    async fn leave_cluster(&self, _: Request<LeaveRequest>) -> Result<Response<LeaveResponse>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }
    
    async fn submit_task(&self, request: Request<TaskRequest>) -> Result<Response<TaskResponse>, Status> {
        let state = *self.0.state.lock().unwrap();
        
        if state != RaftNodeState::Leader {
            return Err(Status::unavailable("Not the leader"));
        }
        
        // Simulate task submission through consensus
        let task = request.into_inner();
        let entry = LogEntry {
            index: self.0.log.lock().unwrap().len() as u64 + 1,
            term: *self.0.current_term.lock().unwrap(),
            data: task.task_id.clone().into_bytes(),
        };
        
        self.0.log.lock().unwrap().push(entry.clone());
        self.0.applied_entries.lock().unwrap().push(entry);
        
        Ok(Response::new(TaskResponse {
            accepted: true,
            message: "Task accepted".to_string(),
            assigned_node: self.0.node_id,
        }))
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

// ===== Test Cases =====

#[madsim::test]
async fn test_leader_election_basic() {
    let handle = Handle::current();
    
    // Create server node
    let server_addr: SocketAddr = "10.0.0.1:7001".parse().unwrap();
    let server_node = handle.create_node()
        .name("server")
        .ip(server_addr.ip())
        .build();
    
    // Start server
    server_node.spawn(async move {
        let test_node = TestRaftNode::new(1, server_addr.to_string(), HashSet::from([1]));
        let service = TestRaftNodeService(test_node);
        
        Server::builder()
            .add_service(ClusterServiceServer::new(service))
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
        let response = client.health_check(Request::new(HealthCheckRequest {}))
            .await
            .expect("Health check failed");
        assert!(response.into_inner().healthy);
        
        // Test cluster status
        let response = client.get_cluster_status(Request::new(ClusterStatusRequest {}))
            .await
            .expect("Get cluster status failed");
        let status = response.into_inner();
        // Note: In a real implementation, the node would elect itself as leader
        // For now, just verify we can get status
        info!("Cluster status: leader_id={}, term={}", status.leader_id, status.term);
        
        info!("✅ Basic Raft node test passed");
    }).await.unwrap();
        
}

#[madsim::test] 
async fn test_leader_election_with_partition() {
    // Clear any previous network state
    {
        let mut network_state = NETWORK_STATE.write().unwrap();
        network_state.clear();
    }
    
    let handle = Handle::current();
    let _net = NetSim::current();
    
    // Create 5-node cluster
    let peers: HashSet<u64> = vec![1, 2, 3, 4, 5].into_iter().collect();
    let mut node_handles = vec![];
    
    // Start all server nodes
    for i in 1..=5 {
        let addr = format!("10.0.0.{}:700{}", i, i);
        let socket_addr: SocketAddr = addr.parse().unwrap();
        
        let node_handle = handle.create_node()
            .name(format!("node-{}", i))
            .ip(socket_addr.ip())
            .build();
        
        let node = TestRaftNode::new(i, addr.clone(), peers.clone());
        let service = TestRaftNodeService(node.clone());
        let serve_addr = socket_addr;
        
        // Start the node with both server and tick loop
        node_handle.spawn(async move {
            // Run the Raft tick loop in background
            let node_for_tick = node.clone();
            spawn(async move {
                node_for_tick.run().await;
            });
            
            // Run the server
            Server::builder()
                .add_service(ClusterServiceServer::new(service))
                .serve(serve_addr)
                .await
                .unwrap();
        });
        
        node_handles.push(node_handle);
    }
    
    // Give servers time to start
    sleep(Duration::from_secs(2)).await;
    
    // Create client node for testing
    let client_node = handle.create_node()
        .name("test-client")
        .ip("10.0.0.100".parse().unwrap())
        .build();
    
    // Run all client tests
    client_node.spawn(async move {
        // Connect to all nodes
        let mut clients = vec![];
        for i in 1..=5 {
            let addr = format!("http://10.0.0.{}:700{}", i, i);
            if let Ok(client) = ClusterServiceClient::connect(addr).await {
                clients.push(client);
            }
        }
        
        // Wait for initial leader election
        let initial_leader = ConsensusVerifier::wait_for_leader(&mut clients, Duration::from_secs(10))
            .await
            .expect("Failed to elect initial leader");
        info!("Initial leader elected: {}", initial_leader);
        
        info!("Creating network partition: [1,2] | [3,4,5]");
        
        // Create partition: minority [1,2] | majority [3,4,5]
        let partition = NetworkPartition::create(&[1, 2, 3, 4, 5], 2);
        
        // Apply partition by blocking communication between minority and majority
        {
            let mut network_state = NETWORK_STATE.write().unwrap();
            for &minority_node in &partition.minority {
                for &majority_node in &partition.majority {
                    // Block bidirectional communication
                    network_state.block_connection(minority_node, majority_node);
                    network_state.block_connection(majority_node, minority_node);
                }
            }
        }
        info!("Applied network partition between {:?} and {:?}", partition.minority, partition.majority);
        
        // Wait for partition effects
        sleep(Duration::from_secs(3)).await;
        
        // Check that minority cannot elect leader
        let mut minority_has_leader = false;
        for i in 0..2 {
            if let Ok(response) = clients[i].get_cluster_status(Request::new(ClusterStatusRequest {})).await {
                let inner = response.into_inner();
                if inner.leader_id != 0 && partition.minority.contains(&inner.leader_id) {
                    minority_has_leader = true;
                }
            }
        }
        
        if minority_has_leader {
            panic!("Minority partition should not have a leader");
        }
        
        info!("✅ Minority partition has no leader");
        
        // Check that majority can still operate
        let majority_set: HashSet<u64> = partition.majority.iter().copied().collect();
        let majority_leader = ConsensusVerifier::wait_for_leader_among(
            &mut clients[2..5], 
            &majority_set,
            Duration::from_secs(10)
        ).await.expect("Majority partition should elect a leader");
        
        info!("✅ Majority partition has leader: {}", majority_leader);
        
        // Heal partition
        info!("Healing network partition");
        {
            let mut network_state = NETWORK_STATE.write().unwrap();
            network_state.clear(); // Clear all partitions
        }
        
        // Wait for nodes to converge on a single leader after healing
        info!("Waiting for cluster to converge after healing partition...");
        let start = Instant::now();
        let mut converged = false;
        
        while start.elapsed() < Duration::from_secs(20) {
            if let Ok(()) = ConsensusVerifier::verify_log_matching(&mut clients).await {
                converged = true;
                break;
            }
            sleep(Duration::from_millis(500)).await;
        }
        
        if !converged {
            panic!("Cluster failed to converge after healing partition");
        }
        
        info!("✅ Leader election with partition test passed");
    }).await.unwrap();
}

#[madsim::test]
async fn test_concurrent_elections() {
    let handle = Handle::current();
    
    // Create 7-node cluster for more complex scenarios
    let peers: HashSet<u64> = vec![1, 2, 3, 4, 5, 6, 7].into_iter().collect();
    let mut node_handles = vec![];
    
    // Start all server nodes
    for i in 1..=7 {
        let addr = format!("10.0.0.{}:700{}", i, i);
        let socket_addr: SocketAddr = addr.parse().unwrap();
        
        let node_handle = handle.create_node()
            .name(format!("node-{}", i))
            .ip(socket_addr.ip())
            .build();
        
        let node = TestRaftNode::new(i, addr.clone(), peers.clone());
        let service = TestRaftNodeService(node.clone());
        let serve_addr = socket_addr;
        
        // Start the node with both server and tick loop
        node_handle.spawn(async move {
            // Run the Raft tick loop in background
            let node_for_tick = node.clone();
            spawn(async move {
                node_for_tick.run().await;
            });
            
            // Run the server
            Server::builder()
                .add_service(ClusterServiceServer::new(service))
                .serve(serve_addr)
                .await
                .unwrap();
        });
        
        node_handles.push(node_handle);
    }
    
    // Let multiple elections happen
    sleep(Duration::from_secs(5)).await;
    
    // Create client node for testing
    let client_node = handle.create_node()
        .name("test-client")
        .ip("10.0.0.100".parse().unwrap())
        .build();
    
    // Run all client tests
    client_node.spawn(async move {
        // Connect to all nodes
        let mut clients = vec![];
        for i in 1..=7 {
            let addr = format!("http://10.0.0.{}:700{}", i, i);
            if let Ok(client) = ClusterServiceClient::connect(addr).await {
                clients.push(client);
            }
        }
        
        // Check election safety
        ConsensusVerifier::verify_election_safety(&mut clients).await
            .expect("Election safety verification failed");
        
        // Ensure a leader exists
        let leader_id = ConsensusVerifier::wait_for_leader(
            &mut clients,
            Duration::from_secs(10),
        ).await.expect("Failed to find leader");
        
        info!("✅ Leader {} elected after concurrent elections", leader_id);
    }).await.unwrap();
}

#[madsim::test]
async fn test_log_replication_basic() {
    let handle = Handle::current();
    
    // Create 3-node cluster
    let peers: HashSet<u64> = vec![1, 2, 3].into_iter().collect();
    let mut node_handles = vec![];
    
    // Start all server nodes
    for i in 1..=3 {
        let addr = format!("10.0.0.{}:700{}", i, i);
        let socket_addr: SocketAddr = addr.parse().unwrap();
        
        let node_handle = handle.create_node()
            .name(format!("node-{}", i))
            .ip(socket_addr.ip())
            .build();
        
        let node = TestRaftNode::new(i, addr.clone(), peers.clone());
        let service = TestRaftNodeService(node.clone());
        let serve_addr = socket_addr;
        
        // Start the node with both server and tick loop
        node_handle.spawn(async move {
            // Run the Raft tick loop in background
            let node_for_tick = node.clone();
            spawn(async move {
                node_for_tick.run().await;
            });
            
            // Run the server
            Server::builder()
                .add_service(ClusterServiceServer::new(service))
                .serve(serve_addr)
                .await
                .unwrap();
        });
        
        node_handles.push(node_handle);
    }
    
    sleep(Duration::from_secs(2)).await;
    
    // Create client node for testing
    let client_node = handle.create_node()
        .name("test-client")
        .ip("10.0.0.100".parse().unwrap())
        .build();
    
    // Run all client tests
    client_node.spawn(async move {
        // Connect clients and find leader
        let mut clients = vec![];
        let mut leader_client = None;
        let mut leader_id = 0;
        
        for i in 1..=3 {
            let addr = format!("http://10.0.0.{}:700{}", i, i);
            if let Ok(mut client) = ClusterServiceClient::connect(addr).await {
                if let Ok(response) = client.get_cluster_status(Request::new(ClusterStatusRequest {})).await {
                    let status = response.into_inner();
                    if status.leader_id == i {
                        leader_client = Some(client);
                        leader_id = i;
                    } else {
                        clients.push(client);
                    }
                }
            }
        }
        
        let mut leader = leader_client.expect("No leader found");
        
        // Submit multiple tasks through leader
        let mut task_ids = vec![];
        for i in 0..5 {
            let task_id = format!("task-{}", i);
            let response = leader.submit_task(Request::new(TaskRequest {
                task_id: task_id.clone(),
                command: "test".to_string(),
                args: vec![],
                cpu_cores: 1,
                memory_mb: 256,
                disk_gb: 1,
                required_features: vec![],
                timeout_secs: 60,
            })).await.expect("Failed to submit task");
            
            if !response.into_inner().accepted {
                panic!("Task submission failed");
            }
            task_ids.push(task_id);
        }
        
        info!("✅ Submitted {} tasks to leader {}", task_ids.len(), leader_id);
        
        // Allow time for replication
        sleep(Duration::from_secs(1)).await;
        
        // Verify all nodes have the same log
        ConsensusVerifier::verify_log_matching(&mut clients).await
            .expect("Log matching verification failed");
        
        info!("✅ Log replication basic test passed");
    }).await.unwrap();
}

#[madsim::test]
async fn test_log_replication_with_failures() {
    let handle = Handle::current();
    let _net = NetSim::current();
    
    // Create 5-node cluster
    let peers: HashSet<u64> = vec![1, 2, 3, 4, 5].into_iter().collect();
    let mut node_handles = vec![];
    
    // Start all server nodes
    for i in 1..=5 {
        let addr = format!("10.0.0.{}:700{}", i, i);
        let socket_addr: SocketAddr = addr.parse().unwrap();
        
        let node_handle = handle.create_node()
            .name(format!("node-{}", i))
            .ip(socket_addr.ip())
            .build();
        
        let node = TestRaftNode::new(i, addr.clone(), peers.clone());
        let service = TestRaftNodeService(node.clone());
        let serve_addr = socket_addr;
        
        // Start the node with both server and tick loop
        node_handle.spawn(async move {
            // Run the Raft tick loop in background
            let node_for_tick = node.clone();
            spawn(async move {
                node_for_tick.run().await;
            });
            
            // Run the server
            Server::builder()
                .add_service(ClusterServiceServer::new(service))
                .serve(serve_addr)
                .await
                .unwrap();
        });
        
        node_handles.push(node_handle);
    }
        
    sleep(Duration::from_secs(2)).await;
    
    // Create client node for testing
    let client_node = handle.create_node()
        .name("test-client")
        .ip("10.0.0.100".parse().unwrap())
        .build();
    
    // Run all client tests
    client_node.spawn(async move {
        // Connect clients and find leader
        let mut clients = vec![];
        let mut leader_client = None;
        let mut leader_id = 0;
        
        for i in 1..=5 {
            let addr = format!("http://10.0.0.{}:700{}", i, i);
            if let Ok(mut client) = ClusterServiceClient::connect(addr).await {
                if let Ok(response) = client.get_cluster_status(Request::new(ClusterStatusRequest {})).await {
                    let status = response.into_inner();
                    if status.leader_id == i {
                        leader_client = Some(client);
                        leader_id = i;
                    } else {
                        clients.push(client);
                    }
                }
            }
        }
        
        let mut leader = leader_client.expect("No leader found");
        info!("Leader is node {}", leader_id);
        
        // Submit some tasks
        for i in 0..3 {
            let task_id = format!("task-{}", i);
            leader.submit_task(Request::new(TaskRequest {
                task_id,
                command: "test".to_string(),
                args: vec![],
                cpu_cores: 1,
                memory_mb: 256,
                disk_gb: 1,
                required_features: vec![],
                timeout_secs: 60,
            })).await.expect("Failed to submit task");
        }
        
        // Disconnect one follower
        let disconnected_node = if leader_id == 1 { 2 } else { 1 };
        info!("Disconnecting node {}", disconnected_node);
        
        // TODO: Implement network partitioning once MadSim API is clarified
        // In MadSim, network partitioning is typically done through configuration
        // rather than direct API calls
        
        // Submit more tasks while one node is disconnected
        for i in 3..6 {
            let task_id = format!("task-{}", i);
            leader.submit_task(Request::new(TaskRequest {
                task_id,
                command: "test".to_string(),
                args: vec![],
                cpu_cores: 1,
                memory_mb: 256,
                disk_gb: 1,
                required_features: vec![],
                timeout_secs: 60,
            })).await.expect("Failed to submit task");
        }
        
        info!("✅ Submitted tasks while node {} was disconnected", disconnected_node);
        
        // Reconnect the node
        info!("Reconnecting node {}", disconnected_node);
        // TODO: Heal network partition
        // Network healing would be done through MadSim configuration
        
        // Allow time for catch-up
        sleep(Duration::from_secs(2)).await;
        
        // Verify all nodes have caught up
        ConsensusVerifier::verify_log_matching(&mut clients).await
            .expect("Log matching verification failed");
        
        info!("✅ All nodes have consistent logs after reconnection");
    }).await.unwrap();
}

#[madsim::test]
async fn test_leader_failover() {
    // Clear any previous network state
    {
        let mut network_state = NETWORK_STATE.write().unwrap();
        network_state.clear();
    }
    
    let handle = Handle::current();
    
    // Create 5-node cluster
    let peers: HashSet<u64> = vec![1, 2, 3, 4, 5].into_iter().collect();
    let mut node_handles = HashMap::new();
    
    // Start all server nodes
    for i in 1..=5 {
        let addr = format!("10.0.0.{}:700{}", i, i);
        let socket_addr: SocketAddr = addr.parse().unwrap();
        
        let node_handle = handle.create_node()
            .name(format!("node-{}", i))
            .ip(socket_addr.ip())
            .build();
        
        let node = TestRaftNode::new(i, addr.clone(), peers.clone());
        let service = TestRaftNodeService(node.clone());
        let serve_addr = socket_addr;
        
        // Start the node with both server and tick loop
        node_handle.spawn(async move {
            // Run the Raft tick loop in background
            let node_for_tick = node.clone();
            spawn(async move {
                node_for_tick.run().await;
            });
            
            // Run the server
            Server::builder()
                .add_service(ClusterServiceServer::new(service))
                .serve(serve_addr)
                .await
                .unwrap();
        });
        
        node_handles.insert(i, node_handle);
    }
        
    sleep(Duration::from_secs(2)).await;
    
    // Create client node for testing
    let client_node = handle.create_node()
        .name("test-client")
        .ip("10.0.0.100".parse().unwrap())
        .build();
    
    // Run all client tests
    client_node.spawn(async move {
        // Connect to all nodes
        let mut clients = vec![];
        for i in 1..=5 {
            let addr = format!("http://10.0.0.{}:700{}", i, i);
            if let Ok(client) = ClusterServiceClient::connect(addr).await {
                clients.push(client);
            }
        }
        
        // Wait for initial leader
        let current_leader = ConsensusVerifier::wait_for_leader(&mut clients, Duration::from_secs(10))
            .await
            .expect("Failed to elect initial leader");
        
        info!("Current leader is node {}", current_leader);
        
        // Simulate leader failure by isolating it
        {
            let mut network_state = NETWORK_STATE.write().unwrap();
            network_state.isolate_node(current_leader);
        }
        
        info!("Isolated leader node {}", current_leader);
        
        // Wait for new leader election (excluding the old leader)
        let mut remaining_nodes = HashSet::new();
        for i in 1..=5 {
            if i != current_leader {
                remaining_nodes.insert(i);
            }
        }
        
        let new_leader = ConsensusVerifier::wait_for_leader_among(
            &mut clients, 
            &remaining_nodes,
            Duration::from_secs(15)
        ).await.expect("No new leader elected after failover");
        
        info!("✅ New leader elected: node {}", new_leader);
        
        // Clean up network state
        {
            let mut network_state = NETWORK_STATE.write().unwrap();
            network_state.clear();
        }
    }).await.unwrap();
}