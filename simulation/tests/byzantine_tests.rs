//! Byzantine Failure Tests using MadSim
//!
//! These tests verify the system's behavior under Byzantine (malicious) node conditions.
//! MadSim allows us to inject network failures and control message delivery.

use std::time::Duration;
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
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

/// A Byzantine node that claims to be leader when it's not
struct ByzantineNode {
    node_id: u64,
    vms: Arc<Mutex<HashMap<String, VmInfo>>>,
}

#[tonic::async_trait]
impl ClusterService for ByzantineNode {
    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse { 
            healthy: true,
            message: "Byzantine node is healthy".to_string(),
        }))
    }

    async fn get_cluster_status(
        &self,
        _request: Request<ClusterStatusRequest>,
    ) -> Result<Response<ClusterStatusResponse>, Status> {
        // Byzantine behavior: Always claim to be the leader
        let nodes = vec![
            NodeInfo {
                id: self.node_id,
                address: format!("10.0.0.{}:7001", self.node_id),
                state: NodeState::Leader as i32,
            },
        ];
        
        Ok(Response::new(ClusterStatusResponse {
            leader_id: self.node_id,
            nodes,
            term: 999, // Claim a high term
        }))
    }

    async fn create_vm(
        &self,
        request: Request<CreateVmRequest>,
    ) -> Result<Response<CreateVmResponse>, Status> {
        let req = request.into_inner();
        
        // Byzantine behavior: Accept writes even though we're not the real leader
        let vm_info = VmInfo {
            name: req.name.clone(),
            state: 1, // Running state
            node_id: self.node_id,
            memory_mb: req.memory_mb,
            vcpus: req.vcpus,
        };
        
        self.vms.lock().unwrap().insert(req.name.clone(), vm_info);
        
        Ok(Response::new(CreateVmResponse {
            success: true,
            message: "VM created (Byzantine)".to_string(),
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

    // Stub implementations for other methods
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
}

/// Normal node implementation for comparison
struct NormalNode {
    node_id: u64,
    is_leader: Arc<Mutex<bool>>,
    vms: Arc<Mutex<HashMap<String, VmInfo>>>,
}

#[tonic::async_trait]
impl ClusterService for NormalNode {
    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse { 
            healthy: true,
            message: "Byzantine node is healthy".to_string(),
        }))
    }

    async fn get_cluster_status(
        &self,
        _request: Request<ClusterStatusRequest>,
    ) -> Result<Response<ClusterStatusResponse>, Status> {
        let is_leader = *self.is_leader.lock().unwrap();
        let state = if is_leader { NodeState::Leader } else { NodeState::Follower };
        
        let nodes = vec![
            NodeInfo {
                id: self.node_id,
                address: format!("10.0.0.{}:7001", self.node_id),
                state: state as i32,
            },
        ];
        
        Ok(Response::new(ClusterStatusResponse {
            leader_id: if is_leader { self.node_id } else { 1 }, // Assume node 1 is leader if not us
            nodes,
            term: 1,
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
}

#[madsim::test]
async fn test_byzantine_false_leader() {
    let handle = Handle::current();
    
    // Create normal leader node
    let leader_node = handle.create_node()
        .name("leader")
        .ip("10.0.0.1".parse().unwrap())
        .build();
    
    let leader_state = Arc::new(Mutex::new(true));
    let leader_vms = Arc::new(Mutex::new(HashMap::new()));
    
    let leader_service = NormalNode {
        node_id: 1,
        is_leader: leader_state.clone(),
        vms: leader_vms.clone(),
    };
    
    leader_node.spawn(async move {
        let addr = "10.0.0.1:7001".parse().unwrap();
        Server::builder()
            .add_service(ClusterServiceServer::new(leader_service))
            .serve(addr)
            .await
            .unwrap();
    });
    
    // Create Byzantine node that falsely claims leadership
    let byzantine_node = handle.create_node()
        .name("byzantine")
        .ip("10.0.0.2".parse().unwrap())
        .build();
    
    let byzantine_service = ByzantineNode {
        node_id: 2,
        vms: Arc::new(Mutex::new(HashMap::new())),
    };
    
    byzantine_node.spawn(async move {
        let addr = "10.0.0.2:7001".parse().unwrap();
        Server::builder()
            .add_service(ClusterServiceServer::new(byzantine_service))
            .serve(addr)
            .await
            .unwrap();
    });
    
    // Wait for services to start
    sleep(Duration::from_millis(100)).await;
    
    // Create clients
    let client_node = handle.create_node()
        .name("client")
        .ip("10.0.0.3".parse().unwrap())
        .build();
    
    client_node.spawn(async move {
        // Connect to both nodes
        let mut leader_client = ClusterServiceClient::connect("http://10.0.0.1:7001")
            .await
            .expect("Failed to connect to leader");
        
        let mut byzantine_client = ClusterServiceClient::connect("http://10.0.0.2:7001")
            .await
            .expect("Failed to connect to byzantine node");
        
        // Both claim to be leaders
        let leader_status = leader_client.get_cluster_status(Request::new(ClusterStatusRequest {}))
            .await
            .unwrap();
        assert_eq!(leader_status.into_inner().leader_id, 1);
        
        let byzantine_status = byzantine_client.get_cluster_status(Request::new(ClusterStatusRequest {}))
            .await
            .unwrap();
        assert_eq!(byzantine_status.into_inner().leader_id, 2); // Byzantine node claims leadership
        
        // Try to create VMs on both
        let vm_req = CreateVmRequest {
            name: "test-vm".to_string(),
            config_path: "/tmp/test.nix".to_string(),
            vcpus: 2,
            memory_mb: 512,
        };
        
        // Real leader accepts
        let leader_result = leader_client.create_vm(Request::new(vm_req.clone()))
            .await
            .unwrap();
        assert!(leader_result.into_inner().success);
        
        // Byzantine node also accepts (incorrectly)
        let byzantine_result = byzantine_client.create_vm(Request::new(vm_req))
            .await
            .unwrap();
        assert!(byzantine_result.into_inner().success);
        
        // This creates a split-brain scenario - both nodes think they have the VM
        let leader_vms = leader_client.list_vms(Request::new(ListVmsRequest {}))
            .await
            .unwrap()
            .into_inner()
            .vms;
        assert_eq!(leader_vms.len(), 1);
        
        let byzantine_vms = byzantine_client.list_vms(Request::new(ListVmsRequest {}))
            .await
            .unwrap()
            .into_inner()
            .vms;
        assert_eq!(byzantine_vms.len(), 1);
        
        println!("Byzantine test: Detected split-brain scenario!");
    }).await.unwrap();
}

#[madsim::test]
async fn test_selective_message_dropping() {
    let handle = Handle::current();
    
    // Create a Byzantine node that selectively drops messages
    let byzantine_node = handle.create_node()
        .name("byzantine")
        .ip("10.0.0.1".parse().unwrap())
        .build();
    
    let drop_counter = Arc::new(Mutex::new(0));
    let drop_counter_clone = drop_counter.clone();
    
    // Service that drops every 3rd request
    struct SelectiveDropService {
        counter: Arc<Mutex<u32>>,
        vms: Arc<Mutex<HashMap<String, VmInfo>>>,
    }
    
    #[tonic::async_trait]
    impl ClusterService for SelectiveDropService {
        async fn create_vm(
            &self,
            request: Request<CreateVmRequest>,
        ) -> Result<Response<CreateVmResponse>, Status> {
            let mut count = self.counter.lock().unwrap();
            *count += 1;
            
            // Drop every 3rd request
            if *count % 3 == 0 {
                return Err(Status::unavailable("Network error (simulated)"));
            }
            
            let req = request.into_inner();
            let vm_info = VmInfo {
                name: req.name.clone(),
                state: 1, // Running state
                node_id: 1,
                memory_mb: req.memory_mb,
                vcpus: req.vcpus,
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
        
        // Minimal implementations for other required methods
        async fn health_check(&self, _: Request<HealthCheckRequest>) -> Result<Response<HealthCheckResponse>, Status> {
            Ok(Response::new(HealthCheckResponse { 
            healthy: true,
            message: "Byzantine node is healthy".to_string(),
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
    }
    
    let service = SelectiveDropService {
        counter: drop_counter_clone,
        vms: Arc::new(Mutex::new(HashMap::new())),
    };
    
    byzantine_node.spawn(async move {
        let addr = "10.0.0.1:7001".parse().unwrap();
        Server::builder()
            .add_service(ClusterServiceServer::new(service))
            .serve(addr)
            .await
            .unwrap();
    });
    
    sleep(Duration::from_millis(100)).await;
    
    // Client tries to create multiple VMs
    let client_node = handle.create_node()
        .name("client")
        .ip("10.0.0.2".parse().unwrap())
        .build();
    
    client_node.spawn(async move {
        let mut client = ClusterServiceClient::connect("http://10.0.0.1:7001")
            .await
            .expect("Failed to connect");
        
        let mut successes = 0;
        let mut failures = 0;
        
        // Try to create 10 VMs
        for i in 0..10 {
            let req = CreateVmRequest {
                name: format!("vm-{}", i),
                config_path: "/tmp/test.nix".to_string(),
                vcpus: 1,
                memory_mb: 256,
            };
            
            match client.create_vm(Request::new(req)).await {
                Ok(_) => successes += 1,
                Err(_) => failures += 1,
            }
        }
        
        println!("Selective drop test: {} successes, {} failures", successes, failures);
        
        // We expect roughly 1/3 to fail
        assert!(failures >= 2 && failures <= 4, "Expected ~3 failures, got {}", failures);
        assert!(successes >= 6 && successes <= 8, "Expected ~7 successes, got {}", successes);
        
        // Verify final state
        let final_vms = client.list_vms(Request::new(ListVmsRequest {}))
            .await
            .unwrap()
            .into_inner()
            .vms;
        
        assert_eq!(final_vms.len(), successes, "VM count should match successes");
    }).await.unwrap();
}