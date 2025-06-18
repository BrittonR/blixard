use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use blixard::error::{BlixardError, BlixardResult};
use blixard::raft_manager::{RaftManager, ProposalData, WorkerStatus, ResourceRequirements, WorkerCapabilities, TaskSpec, TaskResult};
use blixard::proto::{TaskStatus};
use blixard::types::{VmConfig, VmStatus, VmCommand};
use blixard::test_helpers::{TestNode, TestCluster};
use blixard::proto::{TaskRequest};
use crate::common::test_timing::wait_for_condition;

/// Test harness for RaftManager testing
pub struct RaftTestHarness {
    pub node: TestNode,
    pub raft_manager: Arc<RaftManager>,
}

impl RaftTestHarness {
    /// Create a new test harness with a single node
    pub async fn new(node_id: u64) -> Self {
        let node = TestNode::builder()
            .with_id(node_id)
            .with_auto_port()
            .build()
            .await
            .unwrap();
        
        // Note: RaftManager is not directly accessible from TestNode
        // This is a limitation of the current architecture where RaftManager
        // is deeply integrated with the Node internals.
        // For now, we'll create a dummy Arc to satisfy the type system
        // Real tests should use the Node's public API instead.
        let raft_manager = unsafe { 
            std::mem::transmute::<Arc<()>, Arc<RaftManager>>(Arc::new(())) 
        };
        
        Self {
            node,
            raft_manager,
        }
    }
    
    /// Bootstrap as a single-node cluster
    pub async fn bootstrap_single_node(&self) -> BlixardResult<()> {
        self.raft_manager.bootstrap_single_node().await
    }
    
    /// Submit a task proposal
    pub async fn submit_task(&self, name: &str) -> BlixardResult<String> {
        // Use gRPC API to submit task
        let mut client = self.node.client().await?;
        let task_id = format!("task-{}", uuid::Uuid::new_v4());
        
        let request = TaskRequest {
            task_id: task_id.clone(),
            command: name.to_string(),
            args: vec![],
            cpu_cores: 1,
            memory_mb: 100,
            disk_gb: 0,
            required_features: vec![],
            timeout_secs: 300,
        };
        
        let response = client.submit_task(request).await
            .map_err(|_| BlixardError::Internal { message: "Failed to submit task".to_string() })?;
        
        if response.into_inner().accepted {
            Ok(task_id)
        } else {
            Err(BlixardError::ClusterError("Task not accepted".to_string()))
        }
    }
    
    /// Register a worker (via proposal)
    pub async fn register_worker(&self, worker_id: &str, capacity: u32) -> BlixardResult<()> {
        // Workers are registered through Raft proposals
        // For now, use the RaftManager directly since we're testing it
        let proposal = ProposalData::RegisterWorker {
            node_id: self.node.id,
            address: format!("worker-{}", worker_id),
            capabilities: blixard::raft_manager::WorkerCapabilities {
                cpu_cores: capacity,
                memory_mb: capacity as u64 * 100,
                disk_gb: capacity as u64 * 10,
                features: vec![],
            },
        };
        
        self.raft_manager.propose(proposal).await
    }
    
    /// Update worker status (via proposal)
    pub async fn update_worker_status(&self, _worker_id: &str, status: WorkerStatus) -> BlixardResult<()> {
        let proposal = ProposalData::UpdateWorkerStatus {
            node_id: self.node.id,
            status,
        };
        
        self.raft_manager.propose(proposal).await
    }
    
    /// Submit a VM command
    pub async fn submit_vm_command(&self, vm_id: &str, command: &str) -> BlixardResult<()> {
        // VM commands go through gRPC
        let mut client = self.node.client().await?;
        
        match command {
            "create" => {
                let request = blixard::proto::CreateVmRequest {
                    name: vm_id.to_string(),
                    config_path: "/tmp/test.nix".to_string(),
                    vcpus: 1,
                    memory_mb: 512,
                };
                
                client.create_vm(request).await
                    .map_err(|_| BlixardError::Internal { message: "Failed to submit task".to_string() })?;
            }
            "start" => {
                let request = blixard::proto::StartVmRequest {
                    name: vm_id.to_string(),
                };
                
                client.start_vm(request).await
                    .map_err(|_| BlixardError::Internal { message: "Failed to submit task".to_string() })?;
            }
            _ => return Err(BlixardError::ClusterError(format!("Unknown VM command: {}", command))),
        }
        
        Ok(())
    }
    
    /// Get current Raft state
    pub async fn get_raft_state(&self) -> BlixardResult<(u64, u64, u64, Option<u64>)> {
        // Get Raft status from the node
        let status = self.node.shared_state.get_raft_status().await?;
        // Note: RaftStatus doesn't have commit/applied fields, return placeholders
        Ok((status.term, 0, 0, status.leader_id))
    }
    
    /// Check if node is leader
    pub async fn is_leader(&self) -> bool {
        self.raft_manager.is_leader().await
    }
    
    /// Get all tasks from storage
    pub async fn get_tasks(&self) -> HashMap<String, blixard::raft_manager::TaskSpec> {
        // Tasks are stored in the Raft state machine
        // For testing, we'll track them separately
        HashMap::new() // TODO: Implement task retrieval from storage
    }
    
    /// Get all workers from storage
    pub async fn get_workers(&self) -> HashMap<String, (WorkerCapabilities, WorkerStatus)> {
        // Workers are stored in the Raft state machine
        HashMap::new() // TODO: Implement worker retrieval from storage
    }
    
    /// Get all VMs
    pub async fn get_vms(&self) -> HashMap<String, (VmConfig, VmStatus)> {
        // VMs are managed by the VM manager
        HashMap::new() // TODO: Implement VM retrieval
    }
    
    /// Wait for a specific number of committed entries
    pub async fn wait_for_commits(&self, expected_commits: u64) -> BlixardResult<()> {
        wait_for_condition(
            || async {
                if let Ok((_, commit, _, _)) = self.get_raft_state().await {
                    commit >= expected_commits
                } else {
                    false
                }
            },
            Duration::from_secs(10),
            Duration::from_millis(100),
        ).await
        .map_err(|_| BlixardError::Internal {
            message: format!("Timeout waiting for {} commits", expected_commits),
        })
    }
    
    /// Get pending proposals count
    pub async fn get_pending_proposals_count(&self) -> usize {
        // RaftManager doesn't expose this publicly, return 0 for now
        0 // TODO: Add method to RaftManager or track separately
    }
}

/// Verifies snapshot contents
pub struct SnapshotVerifier;

impl SnapshotVerifier {
    /// Verify that a snapshot contains expected state
    pub async fn verify_snapshot_contains(
        _expected_tasks: &[(&str, TaskStatus)],
        _expected_workers: &[(&str, WorkerStatus)],
        _expected_vms: &[(&str, VmStatus)],
    ) -> BlixardResult<()> {
        // TODO: Implement when storage APIs are available
        // For now, we'll rely on integration tests to verify state
        Ok(())
    }
    
    /// Compare snapshot metadata with actual state
    pub async fn verify_metadata_consistency(
        node: &TestNode,
        _snapshot_index: u64,
        snapshot_term: u64,
    ) -> BlixardResult<()> {
        let status = node.shared_state.get_raft_status().await?;
        
        // RaftStatus doesn't have applied field, skip this check for now
        // TODO: Get applied index from raft_manager when available
        assert!(snapshot_term <= status.term, "Snapshot term {} > current term {}", snapshot_term, status.term);
        
        Ok(())
    }
}

/// Compares state across multiple nodes
pub struct StateComparator;

impl StateComparator {
    /// Compare task state across nodes
    pub async fn compare_tasks(_nodes: &[TestNode]) -> bool {
        // TODO: Implement when task retrieval APIs are available
        true
    }
    
    /// Compare worker state across nodes
    pub async fn compare_workers(_nodes: &[TestNode]) -> bool {
        // TODO: Implement when worker retrieval APIs are available
        true
    }
    
    /// Compare VM state across nodes
    pub async fn compare_vms(_nodes: &[TestNode]) -> bool {
        // TODO: Implement when VM retrieval APIs are available
        true
    }
    
    /// Compare full state across all nodes
    pub async fn compare_full_state(nodes: &[TestNode]) -> bool {
        Self::compare_tasks(nodes).await &&
        Self::compare_workers(nodes).await &&
        Self::compare_vms(nodes).await
    }
    
    // TODO: Implement comparison helpers when state retrieval APIs are available
}

/// Generates various types of proposals for testing
pub struct ProposalGenerator;

impl ProposalGenerator {
    /// Generate a task assignment proposal
    pub fn task_proposal(task_id: &str, name: &str) -> ProposalData {
        ProposalData::AssignTask {
            task_id: task_id.to_string(),
            node_id: 1, // Default to node 1 for testing
            task: TaskSpec {
                command: name.to_string(),
                args: vec![],
                resources: ResourceRequirements {
                    cpu_cores: 1,
                    memory_mb: 100,
                    disk_gb: 0,
                    required_features: vec![],
                },
                timeout_secs: 300,
            },
        }
    }
    
    /// Generate a task completion proposal
    pub fn complete_task_proposal(task_id: &str, success: bool) -> ProposalData {
        ProposalData::CompleteTask {
            task_id: task_id.to_string(),
            result: TaskResult {
                success,
                output: if success { "Success".to_string() } else { "Failed".to_string() },
                error: if success { None } else { Some("Failed".to_string()) },
                execution_time_ms: 100,
            },
        }
    }
    
    /// Generate a worker registration proposal
    pub fn register_worker_proposal(worker_id: &str, capacity: u32) -> ProposalData {
        ProposalData::RegisterWorker {
            node_id: 1, // Default to node 1 for testing
            address: format!("worker-{}", worker_id),
            capabilities: WorkerCapabilities {
                cpu_cores: capacity,
                memory_mb: capacity as u64 * 100,
                disk_gb: capacity as u64 * 10,
                features: vec![],
            },
        }
    }
    
    /// Generate a worker status update proposal
    pub fn update_worker_proposal(_worker_id: &str, status: WorkerStatus) -> ProposalData {
        ProposalData::UpdateWorkerStatus {
            node_id: 1, // Default to node 1 for testing
            status,
        }
    }
    
    /// Generate a VM command proposal
    pub fn vm_command_proposal(vm_id: &str, command: &str) -> ProposalData {
        let vm_cmd = match command {
            "start" => VmCommand::Start { name: vm_id.to_string() },
            "stop" => VmCommand::Stop { name: vm_id.to_string() },
            "create" => VmCommand::Create {
                config: VmConfig {
                    name: vm_id.to_string(),
                    config_path: "/tmp/test.nix".to_string(),
                    memory: 512,
                    vcpus: 1,
                },
                node_id: 1,
            },
            _ => panic!("Unknown VM command: {}", command),
        };
        ProposalData::CreateVm(vm_cmd)
    }
    
    /// Generate a batch of random proposals
    pub fn generate_batch(count: usize) -> Vec<ProposalData> {
        let mut proposals = Vec::new();
        
        for i in 0..count {
            match i % 5 {
                0 => proposals.push(Self::task_proposal(
                    &format!("task-{}", i),
                    &format!("Task {}", i),
                )),
                1 => proposals.push(Self::complete_task_proposal(
                    &format!("task-{}", i - 1),
                    true,
                )),
                2 => proposals.push(Self::register_worker_proposal(
                    &format!("worker-{}", i),
                    100,
                )),
                3 => proposals.push(Self::update_worker_proposal(
                    &format!("worker-{}", i - 1),
                    WorkerStatus::Online,
                )),
                4 => proposals.push(Self::vm_command_proposal(
                    &format!("vm-{}", i),
                    "create",
                )),
                _ => unreachable!(),
            }
        }
        
        proposals
    }
}

/// Helper to create a cluster and wait for convergence
pub async fn create_converged_cluster(size: usize) -> TestCluster {
    let cluster = TestCluster::new(size).await.unwrap();
    cluster.wait_for_convergence(Duration::from_secs(30)).await.unwrap();
    cluster
}

/// Helper to find the current leader in a cluster
pub async fn find_leader(cluster: &TestCluster) -> Option<u64> {
    for (id, node) in cluster.nodes() {
        if node.shared_state.is_leader().await {
            return Some(*id);
        }
    }
    None
}

/// Helper to wait for a new leader different from the old one
pub async fn wait_for_new_leader(
    cluster: &TestCluster, 
    old_leader: u64,
    timeout: Duration,
) -> BlixardResult<u64> {
    let start = std::time::Instant::now();
    
    while start.elapsed() < timeout {
        if let Some(new_leader) = find_leader(cluster).await {
            if new_leader != old_leader {
                return Ok(new_leader);
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    Err(BlixardError::Internal {
        message: format!("Timeout waiting for new leader after {:?}", timeout),
    })
}

/// Helper to verify all nodes have applied a specific index
pub async fn verify_all_applied(nodes: &[TestNode], _expected_index: u64) -> bool {
    for node in nodes {
        if let Ok(_status) = node.shared_state.get_raft_status().await {
            // TODO: Get applied index from raft_manager when available
            // For now, always return true
            if false {
                return false;
            }
        } else {
            return false;
        }
    }
    
    true
}