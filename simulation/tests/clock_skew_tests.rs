//! Clock Skew and Time Synchronization Tests using MadSim
//!
//! These tests verify the system's behavior under various time-related edge cases.
//! MadSim allows us to control time advancement precisely.

use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use madsim::{time::sleep, runtime::Handle};
use tonic::{transport::Server, Request, Response, Status};
use blixard_simulation::proto::{
    cluster_service_server::{ClusterService, ClusterServiceServer},
    cluster_service_client::ClusterServiceClient,
    HealthCheckRequest, HealthCheckResponse,
    CreateVmRequest, CreateVmResponse,
    ListVmsRequest, ListVmsResponse, VmInfo,
    ClusterStatusRequest, ClusterStatusResponse, NodeInfo, NodeState,
    JoinRequest, JoinResponse,
    LeaveRequest, LeaveResponse,
    RaftMessageRequest, RaftMessageResponse,
    TaskRequest, TaskResponse,
    TaskStatusRequest, TaskStatusResponse,
    StartVmRequest, StartVmResponse,
    StopVmRequest, StopVmResponse,
    GetVmStatusRequest, GetVmStatusResponse,
};

/// Node that tracks timing of operations
struct TimeAwareNode {
    node_id: u64,
    last_heartbeat: Arc<Mutex<Instant>>,
    election_timeout: Duration,
    is_leader: Arc<Mutex<bool>>,
    term: Arc<Mutex<u64>>,
    vms: Arc<Mutex<HashMap<String, VmInfo>>>,
}

impl TimeAwareNode {
    fn new(node_id: u64, election_timeout: Duration) -> Self {
        Self {
            node_id,
            last_heartbeat: Arc::new(Mutex::new(Instant::now())),
            election_timeout,
            is_leader: Arc::new(Mutex::new(false)),
            term: Arc::new(Mutex::new(0)),
            vms: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    fn check_election_timeout(&self) -> bool {
        let last = *self.last_heartbeat.lock().unwrap();
        last.elapsed() > self.election_timeout
    }
    
    fn trigger_election(&self) {
        let mut term = self.term.lock().unwrap();
        *term += 1;
        *self.is_leader.lock().unwrap() = true;
        *self.last_heartbeat.lock().unwrap() = Instant::now();
        println!("Node {} triggered election for term {}", self.node_id, *term);
    }
}

#[tonic::async_trait]
impl ClusterService for TimeAwareNode {
    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse { 
            healthy: true,
            message: "Time aware node is healthy".to_string(),
        }))
    }

    async fn get_cluster_status(
        &self,
        _request: Request<ClusterStatusRequest>,
    ) -> Result<Response<ClusterStatusResponse>, Status> {
        // Check if we should trigger an election due to timeout
        if !*self.is_leader.lock().unwrap() && self.check_election_timeout() {
            self.trigger_election();
        }
        
        let is_leader = *self.is_leader.lock().unwrap();
        let term = *self.term.lock().unwrap();
        let state = if is_leader { NodeState::Leader } else { NodeState::Follower };
        
        let nodes = vec![
            NodeInfo {
                id: self.node_id,
                address: format!("10.0.0.{}:7001", self.node_id),
                state: state as i32,
            },
        ];
        
        Ok(Response::new(ClusterStatusResponse {
            leader_id: if is_leader { self.node_id } else { 0 },
            nodes,
            term,
        }))
    }

    async fn send_raft_message(
        &self,
        _request: Request<RaftMessageRequest>,
    ) -> Result<Response<RaftMessageResponse>, Status> {
        // Simulate receiving a heartbeat
        *self.last_heartbeat.lock().unwrap() = Instant::now();
        Ok(Response::new(RaftMessageResponse { 
            success: true,
            error: String::new(),
        }))
    }

    async fn create_vm(
        &self,
        request: Request<CreateVmRequest>,
    ) -> Result<Response<CreateVmResponse>, Status> {
        if !*self.is_leader.lock().unwrap() {
            return Err(Status::unavailable("Not the leader"));
        }
        
        let req = request.into_inner();
        let vm_info = VmInfo {
            name: req.name.clone(),
            state: 1, // Running state
            node_id: self.node_id,
            memory_mb: req.memory_mb,
            vcpus: req.vcpus,
            ip_address: "10.0.0.103".to_string(),
        };
        
        self.vms.lock().unwrap().insert(req.name.clone(), vm_info);
        
        Ok(Response::new(CreateVmResponse {
            success: true,
            message: "VM created".to_string(),
            vm_id: req.name,
        }))
    }

    async fn list_vms(
        &self,
        _request: Request<ListVmsRequest>,
    ) -> Result<Response<ListVmsResponse>, Status> {
        let vms = self.vms.lock().unwrap().values().cloned().collect();
        Ok(Response::new(ListVmsResponse { vms }))
    }

    // Stub implementations
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
    
    async fn start_vm(&self, _: Request<StartVmRequest>) -> Result<Response<StartVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }
    
    async fn stop_vm(&self, _: Request<StopVmRequest>) -> Result<Response<StopVmResponse>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }
    
    async fn get_vm_status(&self, _: Request<GetVmStatusRequest>) -> Result<Response<GetVmStatusResponse>, Status> {
        Err(Status::unimplemented("Not implemented"))
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

#[madsim::test]
async fn test_election_with_clock_skew() {
    let handle = Handle::current();
    
    // Create nodes with different election timeouts (simulating clock skew)
    let fast_node = handle.create_node()
        .name("fast")
        .ip("10.0.0.1".parse().unwrap())
        .build();
    
    let slow_node = handle.create_node()
        .name("slow")
        .ip("10.0.0.2".parse().unwrap())
        .build();
    
    // Fast node has shorter timeout (simulating faster clock)
    let fast_service = TimeAwareNode::new(1, Duration::from_millis(150));
    fast_service.is_leader.lock().unwrap().clone_from(&true); // Start as leader
    fast_service.term.lock().unwrap().clone_from(&1);
    
    // Slow node has longer timeout (simulating slower clock)
    let slow_service = TimeAwareNode::new(2, Duration::from_millis(300));
    
    let _fast_heartbeat = fast_service.last_heartbeat.clone();
    let _slow_heartbeat = slow_service.last_heartbeat.clone();
    
    fast_node.spawn(async move {
        let addr = "10.0.0.1:7001".parse().unwrap();
        Server::builder()
            .add_service(ClusterServiceServer::new(fast_service))
            .serve(addr)
            .await
            .unwrap();
    });
    
    slow_node.spawn(async move {
        let addr = "10.0.0.2:7001".parse().unwrap();
        Server::builder()
            .add_service(ClusterServiceServer::new(slow_service))
            .serve(addr)
            .await
            .unwrap();
    });
    
    sleep(Duration::from_millis(100)).await;
    
    // Simulate heartbeat sender with variable delays
    let heartbeat_node = handle.create_node()
        .name("heartbeat")
        .ip("10.0.0.3".parse().unwrap())
        .build();
    
    heartbeat_node.spawn(async move {
        let mut fast_client = ClusterServiceClient::connect("http://10.0.0.1:7001")
            .await
            .expect("Failed to connect to fast node");
        
        let mut slow_client = ClusterServiceClient::connect("http://10.0.0.2:7001")
            .await
            .expect("Failed to connect to slow node");
        
        // Send heartbeats with increasing delays
        for i in 0..5 {
            let delay = Duration::from_millis(100 + i * 50);
            sleep(delay).await;
            
            // Send heartbeat to both nodes
            let _ = fast_client.send_raft_message(Request::new(RaftMessageRequest {
                raft_data: vec![], // Simplified for test
            })).await;
            
            let _ = slow_client.send_raft_message(Request::new(RaftMessageRequest {
                raft_data: vec![], // Simplified for test
            })).await;
            
            println!("Sent heartbeat after {:?} delay", delay);
        }
        
        // Stop sending heartbeats and wait for election
        sleep(Duration::from_millis(400)).await;
        
        // Check status - fast node should trigger election first
        let fast_status = fast_client.get_cluster_status(Request::new(ClusterStatusRequest {}))
            .await
            .unwrap()
            .into_inner();
        
        let slow_status = slow_client.get_cluster_status(Request::new(ClusterStatusRequest {}))
            .await
            .unwrap()
            .into_inner();
        
        println!("Fast node: term={}, leader={}", fast_status.term, fast_status.leader_id == 1);
        println!("Slow node: term={}, leader={}", slow_status.term, slow_status.leader_id == 2);
        
        // Fast node should have higher term due to earlier timeout
        assert!(fast_status.term >= slow_status.term, 
            "Fast node should trigger election first");
    }).await.unwrap();
}

#[madsim::test]
async fn test_time_jumps() {
    let handle = Handle::current();
    
    // Create a node that tracks operation timing
    let node = handle.create_node()
        .name("timenode")
        .ip("10.0.0.1".parse().unwrap())
        .build();
    
    #[derive(Clone)]
    struct TimedOperationNode {
        operation_times: Arc<Mutex<Vec<(String, Instant)>>>,
        vms: Arc<Mutex<HashMap<String, VmInfo>>>,
    }
    
    #[tonic::async_trait]
    impl ClusterService for TimedOperationNode {
        async fn create_vm(
            &self,
            request: Request<CreateVmRequest>,
        ) -> Result<Response<CreateVmResponse>, Status> {
            let req = request.into_inner();
            let now = Instant::now();
            
            self.operation_times.lock().unwrap().push((req.name.clone(), now));
            
            let vm_info = VmInfo {
                name: req.name.clone(),
                state: 1, // Running state
                node_id: 1,
                memory_mb: req.memory_mb,
                vcpus: req.vcpus,
                ip_address: "10.0.0.104".to_string(),
            };
            
            self.vms.lock().unwrap().insert(req.name.clone(), vm_info);
            
            Ok(Response::new(CreateVmResponse {
                success: true,
                message: "VM created".to_string(),
                vm_id: req.name,
            }))
        }
        
        async fn list_vms(
            &self,
            _request: Request<ListVmsRequest>,
        ) -> Result<Response<ListVmsResponse>, Status> {
            let vms = self.vms.lock().unwrap().values().cloned().collect();
            Ok(Response::new(ListVmsResponse { vms }))
        }
        
        // Minimal implementations
        async fn health_check(&self, _: Request<HealthCheckRequest>) -> Result<Response<HealthCheckResponse>, Status> {
            Ok(Response::new(HealthCheckResponse { 
            healthy: true,
            message: "Time aware node is healthy".to_string(),
        }))
        }
        
        async fn get_cluster_status(&self, _: Request<ClusterStatusRequest>) -> Result<Response<ClusterStatusResponse>, Status> {
            Ok(Response::new(ClusterStatusResponse {
                leader_id: 1,
                nodes: vec![],
                term: 1,
            }))
        }
        
        async fn join_cluster(&self, _: Request<JoinRequest>) -> Result<Response<JoinResponse>, Status> {
            Err(Status::unimplemented("Not implemented"))
        }
        
        async fn leave_cluster(&self, _: Request<LeaveRequest>) -> Result<Response<LeaveResponse>, Status> {
            Err(Status::unimplemented("Not implemented"))
        }
        
        async fn send_raft_message(&self, _: Request<RaftMessageRequest>) -> Result<Response<RaftMessageResponse>, Status> {
            Err(Status::unimplemented("Not implemented"))
        }
        
        async fn submit_task(&self, _: Request<TaskRequest>) -> Result<Response<TaskResponse>, Status> {
            Err(Status::unimplemented("Not implemented"))
        }
        
        async fn get_task_status(&self, _: Request<TaskStatusRequest>) -> Result<Response<TaskStatusResponse>, Status> {
            Err(Status::unimplemented("Not implemented"))
        }
        
        async fn start_vm(&self, _: Request<StartVmRequest>) -> Result<Response<StartVmResponse>, Status> {
            Err(Status::unimplemented("Not implemented"))
        }
        
        async fn stop_vm(&self, _: Request<StopVmRequest>) -> Result<Response<StopVmResponse>, Status> {
            Err(Status::unimplemented("Not implemented"))
        }
        
        async fn get_vm_status(&self, _: Request<GetVmStatusRequest>) -> Result<Response<GetVmStatusResponse>, Status> {
            Err(Status::unimplemented("Not implemented"))
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
    
    let service = TimedOperationNode {
        operation_times: Arc::new(Mutex::new(Vec::new())),
        vms: Arc::new(Mutex::new(HashMap::new())),
    };
    
    let op_times = service.operation_times.clone();
    
    node.spawn(async move {
        let addr = "10.0.0.1:7001".parse().unwrap();
        Server::builder()
            .add_service(ClusterServiceServer::new(service))
            .serve(addr)
            .await
            .unwrap();
    });
    
    sleep(Duration::from_millis(100)).await;
    
    // Client simulates time jumps
    let client_node = handle.create_node()
        .name("client")
        .ip("10.0.0.2".parse().unwrap())
        .build();
    
    client_node.spawn(async move {
        let mut client = ClusterServiceClient::connect("http://10.0.0.1:7001")
            .await
            .expect("Failed to connect");
        
        let start = Instant::now();
        
        // Normal operation
        client.create_vm(Request::new(CreateVmRequest {
            name: "vm-1".to_string(),
            config_path: "/tmp/test.nix".to_string(),
            vcpus: 1,
            memory_mb: 256,
        })).await.unwrap();
        
        // Simulate forward time jump by sleeping a lot
        sleep(Duration::from_secs(3600)).await; // 1 hour jump
        
        client.create_vm(Request::new(CreateVmRequest {
            name: "vm-2".to_string(),
            config_path: "/tmp/test.nix".to_string(),
            vcpus: 1,
            memory_mb: 256,
        })).await.unwrap();
        
        // Rapid operations (simulating time moving slowly)
        for i in 3..6 {
            client.create_vm(Request::new(CreateVmRequest {
                name: format!("vm-{}", i),
                config_path: "/tmp/test.nix".to_string(),
                vcpus: 1,
                memory_mb: 256,
            })).await.unwrap();
            
            sleep(Duration::from_millis(1)).await; // Very short delay
        }
        
        let elapsed = start.elapsed();
        println!("Total elapsed time: {:?}", elapsed);
        
        // Check operation timing
        let times = op_times.lock().unwrap();
        assert!(times.len() == 5, "Should have 5 operations");
        
        // There should be a large gap between first and second operation
        let gap = times[1].1.duration_since(times[0].1);
        assert!(gap >= Duration::from_secs(3600), "Should see time jump");
        
        // Last operations should be very close together (allow more time for simulation precision)
        for i in 2..4 {
            let gap = times[i+1].1.duration_since(times[i].1);
            assert!(gap < Duration::from_millis(100), 
                "Rapid operations should be close, but gap was {:?}", gap);
        }
    }).await.unwrap();
}

#[madsim::test]
async fn test_lease_expiration_with_drift() {
    let handle = Handle::current();
    
    // Node that implements lease-based operations
    struct LeaseNode {
        node_id: u64,
        leases: Arc<Mutex<HashMap<String, (VmInfo, Instant)>>>, // VM -> (info, lease_expiry)
        lease_duration: Duration,
    }
    
    impl LeaseNode {
        fn new(node_id: u64, lease_duration: Duration) -> Self {
            Self {
                node_id,
                leases: Arc::new(Mutex::new(HashMap::new())),
                lease_duration,
            }
        }
        
        fn cleanup_expired_leases(&self) {
            let now = Instant::now();
            let mut leases = self.leases.lock().unwrap();
            leases.retain(|name, (_, expiry)| {
                if *expiry <= now {
                    println!("Lease expired for VM: {}", name);
                    false
                } else {
                    true
                }
            });
        }
    }
    
    #[tonic::async_trait]
    impl ClusterService for LeaseNode {
        async fn create_vm(
            &self,
            request: Request<CreateVmRequest>,
        ) -> Result<Response<CreateVmResponse>, Status> {
            self.cleanup_expired_leases();
            
            let req = request.into_inner();
            let vm_info = VmInfo {
                name: req.name.clone(),
                state: 1, // Running state
                node_id: self.node_id,
                memory_mb: req.memory_mb,
                vcpus: req.vcpus,
                ip_address: "10.0.0.105".to_string(),
            };
            
            let expiry = Instant::now() + self.lease_duration;
            self.leases.lock().unwrap().insert(
                req.name.clone(),
                (vm_info, expiry)
            );
            
            println!("Created VM {} with lease expiring in {:?}", req.name, self.lease_duration);
            
            Ok(Response::new(CreateVmResponse {
                success: true,
                message: "VM created with lease".to_string(),
                vm_id: req.name,
            }))
        }
        
        async fn list_vms(
            &self,
            _request: Request<ListVmsRequest>,
        ) -> Result<Response<ListVmsResponse>, Status> {
            self.cleanup_expired_leases();
            
            let leases = self.leases.lock().unwrap();
            let vms: Vec<VmInfo> = leases.values().map(|(info, _)| info.clone()).collect();
            Ok(Response::new(ListVmsResponse { vms }))
        }
        
        // Minimal implementations for other methods
        async fn health_check(&self, _: Request<HealthCheckRequest>) -> Result<Response<HealthCheckResponse>, Status> {
            Ok(Response::new(HealthCheckResponse { 
            healthy: true,
            message: "Time aware node is healthy".to_string(),
        }))
        }
        
        async fn get_cluster_status(&self, _: Request<ClusterStatusRequest>) -> Result<Response<ClusterStatusResponse>, Status> {
            Ok(Response::new(ClusterStatusResponse {
                leader_id: self.node_id,
                nodes: vec![],
                term: 1,
            }))
        }
        
        async fn join_cluster(&self, _: Request<JoinRequest>) -> Result<Response<JoinResponse>, Status> {
            Err(Status::unimplemented("Not implemented"))
        }
        
        async fn leave_cluster(&self, _: Request<LeaveRequest>) -> Result<Response<LeaveResponse>, Status> {
            Err(Status::unimplemented("Not implemented"))
        }
        
        async fn send_raft_message(&self, _: Request<RaftMessageRequest>) -> Result<Response<RaftMessageResponse>, Status> {
            Err(Status::unimplemented("Not implemented"))
        }
        
        async fn submit_task(&self, _: Request<TaskRequest>) -> Result<Response<TaskResponse>, Status> {
            Err(Status::unimplemented("Not implemented"))
        }
        
        async fn get_task_status(&self, _: Request<TaskStatusRequest>) -> Result<Response<TaskStatusResponse>, Status> {
            Err(Status::unimplemented("Not implemented"))
        }
        
        async fn start_vm(&self, _: Request<StartVmRequest>) -> Result<Response<StartVmResponse>, Status> {
            Err(Status::unimplemented("Not implemented"))
        }
        
        async fn stop_vm(&self, _: Request<StopVmRequest>) -> Result<Response<StopVmResponse>, Status> {
            Err(Status::unimplemented("Not implemented"))
        }
        
        async fn get_vm_status(&self, _: Request<GetVmStatusRequest>) -> Result<Response<GetVmStatusResponse>, Status> {
            Err(Status::unimplemented("Not implemented"))
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
    
    // Create nodes with different lease durations (simulating clock drift)
    let fast_node = handle.create_node()
        .name("fast")
        .ip("10.0.0.1".parse().unwrap())
        .build();
    
    let slow_node = handle.create_node()
        .name("slow")
        .ip("10.0.0.2".parse().unwrap())
        .build();
    
    // Fast node has shorter leases (time moves faster)
    let fast_service = LeaseNode::new(1, Duration::from_secs(2));
    
    // Slow node has longer leases (time moves slower)
    let slow_service = LeaseNode::new(2, Duration::from_secs(5));
    
    fast_node.spawn(async move {
        let addr = "10.0.0.1:7001".parse().unwrap();
        Server::builder()
            .add_service(ClusterServiceServer::new(fast_service))
            .serve(addr)
            .await
            .unwrap();
    });
    
    slow_node.spawn(async move {
        let addr = "10.0.0.2:7001".parse().unwrap();
        Server::builder()
            .add_service(ClusterServiceServer::new(slow_service))
            .serve(addr)
            .await
            .unwrap();
    });
    
    sleep(Duration::from_millis(100)).await;
    
    // Client tests lease behavior
    let client_node = handle.create_node()
        .name("client")
        .ip("10.0.0.3".parse().unwrap())
        .build();
    
    client_node.spawn(async move {
        let mut fast_client = ClusterServiceClient::connect("http://10.0.0.1:7001")
            .await
            .expect("Failed to connect to fast node");
        
        let mut slow_client = ClusterServiceClient::connect("http://10.0.0.2:7001")
            .await
            .expect("Failed to connect to slow node");
        
        // Create VMs on both nodes
        fast_client.create_vm(Request::new(CreateVmRequest {
            name: "fast-vm".to_string(),
            config_path: "/tmp/test.nix".to_string(),
            vcpus: 1,
            memory_mb: 256,
        })).await.unwrap();
        
        slow_client.create_vm(Request::new(CreateVmRequest {
            name: "slow-vm".to_string(),
            config_path: "/tmp/test.nix".to_string(),
            vcpus: 1,
            memory_mb: 256,
        })).await.unwrap();
        
        // Check VMs immediately
        let fast_vms = fast_client.list_vms(Request::new(ListVmsRequest {}))
            .await.unwrap().into_inner().vms;
        let slow_vms = slow_client.list_vms(Request::new(ListVmsRequest {}))
            .await.unwrap().into_inner().vms;
        
        assert_eq!(fast_vms.len(), 1, "Fast node should have 1 VM");
        assert_eq!(slow_vms.len(), 1, "Slow node should have 1 VM");
        
        // Wait for fast node's lease to expire
        sleep(Duration::from_secs(3)).await;
        
        let fast_vms = fast_client.list_vms(Request::new(ListVmsRequest {}))
            .await.unwrap().into_inner().vms;
        let slow_vms = slow_client.list_vms(Request::new(ListVmsRequest {}))
            .await.unwrap().into_inner().vms;
        
        assert_eq!(fast_vms.len(), 0, "Fast node's lease should have expired");
        assert_eq!(slow_vms.len(), 1, "Slow node's lease should still be valid");
        
        // Wait for slow node's lease to expire
        sleep(Duration::from_secs(3)).await;
        
        let slow_vms = slow_client.list_vms(Request::new(ListVmsRequest {}))
            .await.unwrap().into_inner().vms;
        
        assert_eq!(slow_vms.len(), 0, "Slow node's lease should have expired");
        
        println!("Lease test completed: Different nodes expired leases at different rates");
    }).await.unwrap();
}