//! Test harness for executing operations against a Blixard cluster
//!
//! This module provides the interface between the fuzzer and the actual
//! Blixard system, executing operations and capturing state.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::BlixardResult;
use crate::raft_manager::ProposalData;
use crate::test_helpers::{TestCluster, TestNode};
use crate::types::{VmCommand, VmConfig};
use crate::vopr::fuzzer_engine::Coverage;
use crate::vopr::operation_generator::{ByzantineBehavior, ClientOp, Operation};
use crate::vopr::state_tracker::{
    NetworkPartition, NodeRole, NodeState, ResourceUsage, StateSnapshot, VmState,
};
use crate::vopr::time_accelerator::TimeAccelerator;

/// Test harness that manages a Blixard cluster for fuzzing
pub struct TestHarness {
    /// The test cluster
    cluster: Arc<RwLock<Option<TestCluster>>>,

    /// Time accelerator
    time: Arc<TimeAccelerator>,

    /// Active network partitions
    partitions: Arc<RwLock<Vec<NetworkPartition>>>,

    /// Byzantine nodes and their behaviors
    byzantine_nodes: Arc<RwLock<HashMap<u64, ByzantineBehavior>>>,

    /// Message drop configurations
    message_drops: Arc<RwLock<HashMap<(u64, u64), f64>>>,

    /// Coverage tracking
    coverage: Arc<RwLock<Coverage>>,
}

impl TestHarness {
    /// Create a new test harness
    pub fn new(time: Arc<TimeAccelerator>) -> Self {
        Self {
            cluster: Arc::new(RwLock::new(None)),
            time,
            partitions: Arc::new(RwLock::new(Vec::new())),
            byzantine_nodes: Arc::new(RwLock::new(HashMap::new())),
            message_drops: Arc::new(RwLock::new(HashMap::new())),
            coverage: Arc::new(RwLock::new(Coverage::default())),
        }
    }

    /// Execute an operation
    pub async fn execute_operation(&self, operation: &Operation) -> BlixardResult<()> {
        match operation {
            Operation::StartNode { node_id } => self.start_node(*node_id).await,
            Operation::StopNode { node_id } => self.stop_node(*node_id).await,
            Operation::RestartNode { node_id } => self.restart_node(*node_id).await,
            Operation::ClientRequest {
                client_id,
                request_id,
                operation,
            } => {
                self.execute_client_operation(*client_id, *request_id, operation)
                    .await
            }
            Operation::NetworkPartition {
                partition_a,
                partition_b,
            } => {
                self.create_network_partition(partition_a.clone(), partition_b.clone())
                    .await
            }
            Operation::NetworkHeal => self.heal_network().await,
            Operation::ClockJump { node_id, delta_ms } => {
                self.clock_jump(*node_id, *delta_ms).await
            }
            Operation::ClockDrift {
                node_id,
                drift_rate_ppm,
            } => self.clock_drift(*node_id, *drift_rate_ppm).await,
            Operation::ByzantineNode { node_id, behavior } => {
                self.make_byzantine(*node_id, behavior.clone()).await
            }
            Operation::DropMessage {
                from,
                to,
                probability,
            } => self.configure_message_drop(*from, *to, *probability).await,
            Operation::DuplicateMessage { from, to, count } => {
                self.configure_message_duplicate(*from, *to, *count).await
            }
            Operation::ReorderMessages {
                from,
                to,
                window_size,
            } => {
                self.configure_message_reorder(*from, *to, *window_size)
                    .await
            }
            Operation::CheckInvariant { invariant } => {
                // Invariants are checked after each operation in the main loop
                Ok(())
            }
            Operation::WaitForProgress { timeout_ms } => self.wait_for_progress(*timeout_ms).await,
            _ => Ok(()),
        }
    }

    /// Start a new node
    async fn start_node(&self, node_id: u64) -> BlixardResult<()> {
        let mut cluster_guard = self.cluster.write().await;

        // Initialize cluster if needed
        if cluster_guard.is_none() {
            *cluster_guard = Some(TestCluster::new(0).await?);
        }

        // Register node with time accelerator
        self.time.register_node(node_id);

        // For now, we'll need to recreate the cluster with the new node
        // This is a limitation of the current TestCluster API
        let current_size = cluster_guard.as_ref().unwrap().nodes().len();
        if node_id as usize > current_size {
            // Create a new cluster with more nodes
            *cluster_guard = Some(TestCluster::new(node_id as usize).await?);
        }

        // Track coverage
        let mut cov = self.coverage.write().await;
        cov.basic_blocks
            .insert(hash_operation("StartNode", node_id));

        Ok(())
    }

    /// Stop a node
    async fn stop_node(&self, node_id: u64) -> BlixardResult<()> {
        let cluster_guard = self.cluster.read().await;
        if let Some(cluster) = cluster_guard.as_ref() {
            // Note: TestCluster doesn't have a stop_node method
            // We'll simulate this by tracking stopped nodes separately

            // Track coverage
            let mut cov = self.coverage.write().await;
            cov.basic_blocks.insert(hash_operation("StopNode", node_id));
        }
        Ok(())
    }

    /// Restart a node
    async fn restart_node(&self, node_id: u64) -> BlixardResult<()> {
        self.stop_node(node_id).await?;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        self.start_node(node_id).await?;
        Ok(())
    }

    /// Execute a client operation
    async fn execute_client_operation(
        &self,
        client_id: u64,
        request_id: u64,
        operation: &ClientOp,
    ) -> BlixardResult<()> {
        let cluster_guard = self.cluster.read().await;
        if let Some(cluster) = cluster_guard.as_ref() {
            match operation {
                ClientOp::CreateVm { vm_id, cpu, memory } => {
                    let config = VmConfig {
                        name: vm_id.clone(),
                        config_path: "/tmp/test.nix".to_string(),
                        vcpus: *cpu,
                        memory: *memory,
                        tenant_id: "default".to_string(),
                        ip_address: None,
                        metadata: None,
                        anti_affinity: None,
                        priority: 500,
                        preemptible: true,
                        locality_preference: Default::default(),
                        health_check_config: None,
                    };

                    // Send proposal through Raft
                    let proposal = RaftProposal {
                        id: uuid::Uuid::new_v4().as_bytes().to_vec(),
                        data: ProposalData::CreateVm(VmCommand::Create { config, node_id: 1 }),
                        response_tx: None,
                    };

                    if let Ok(leader_id) = cluster.get_leader_id().await {
                        // Send to leader node - in real implementation would use Raft message passing
                    }
                }
                ClientOp::StartVm { vm_id } => {
                    // Similar pattern for Start command
                }
                ClientOp::StopVm { vm_id } => {
                    // Similar pattern for Stop command
                }
                ClientOp::DeleteVm { vm_id } => {
                    // Similar pattern for Delete command
                }
                ClientOp::SubmitTask { task_id, command } => {
                    // Task submission would go here
                }
                ClientOp::GetVmStatus { vm_id } => {
                    // Read operations
                }
                ClientOp::GetClusterStatus => {
                    // Read operations
                }
                ClientOp::ListVms => {
                    // Read operations
                }
            }

            // Track coverage
            let mut cov = self.coverage.write().await;
            cov.basic_blocks.insert(hash_client_op(operation));
        }

        Ok(())
    }

    /// Create a network partition
    async fn create_network_partition(
        &self,
        partition_a: Vec<u64>,
        partition_b: Vec<u64>,
    ) -> BlixardResult<()> {
        let mut partitions = self.partitions.write().await;
        partitions.push(NetworkPartition {
            group_a: partition_a.clone(),
            group_b: partition_b.clone(),
        });

        // Note: TestCluster doesn't have partition methods yet
        // We track partitions for state snapshot purposes

        Ok(())
    }

    /// Heal network partitions
    async fn heal_network(&self) -> BlixardResult<()> {
        let mut partitions = self.partitions.write().await;
        partitions.clear();

        // Clear tracked partitions

        Ok(())
    }

    /// Apply clock jump to a node
    async fn clock_jump(&self, node_id: u64, delta_ms: i64) -> BlixardResult<()> {
        self.time.jump_node_time(
            node_id,
            std::time::Duration::from_millis(delta_ms.abs() as u64),
            delta_ms > 0,
        );
        Ok(())
    }

    /// Apply clock drift to a node
    async fn clock_drift(&self, node_id: u64, drift_rate_ppm: i64) -> BlixardResult<()> {
        self.time.set_clock_drift(node_id, drift_rate_ppm * 1000); // Convert ppm to nanos/sec
        Ok(())
    }

    /// Make a node Byzantine
    async fn make_byzantine(&self, node_id: u64, behavior: ByzantineBehavior) -> BlixardResult<()> {
        let mut byzantine = self.byzantine_nodes.write().await;
        byzantine.insert(node_id, behavior);

        // Apply Byzantine behavior to node
        // This would require modifying the node's behavior in the test cluster

        Ok(())
    }

    /// Configure message dropping
    async fn configure_message_drop(
        &self,
        from: u64,
        to: u64,
        probability: f64,
    ) -> BlixardResult<()> {
        let mut drops = self.message_drops.write().await;
        drops.insert((from, to), probability);

        // Note: Message filtering would be implemented via test infrastructure

        Ok(())
    }

    /// Configure message duplication
    async fn configure_message_duplicate(
        &self,
        from: u64,
        to: u64,
        count: u32,
    ) -> BlixardResult<()> {
        // Note: Message duplication would be implemented via test infrastructure

        Ok(())
    }

    /// Configure message reordering
    async fn configure_message_reorder(
        &self,
        from: u64,
        to: u64,
        window_size: u32,
    ) -> BlixardResult<()> {
        // Note: Message reordering would be implemented via test infrastructure

        Ok(())
    }

    /// Wait for system progress
    async fn wait_for_progress(&self, timeout_ms: u64) -> BlixardResult<()> {
        let timeout = std::time::Duration::from_millis(timeout_ms);
        let start = std::time::Instant::now();

        while start.elapsed() < timeout {
            // Advance simulated time
            self.time.advance(std::time::Duration::from_millis(10));

            // Allow async tasks to progress
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }

        Ok(())
    }

    /// Capture current state snapshot
    pub async fn capture_state_snapshot(&self) -> StateSnapshot {
        let cluster_guard = self.cluster.read().await;

        let mut nodes = HashMap::new();
        let mut views = HashMap::new();
        let mut commit_points = HashMap::new();
        let mut vms = HashMap::new();

        if let Some(cluster) = cluster_guard.as_ref() {
            // Collect node states
            let node_ids: Vec<u64> = cluster.nodes().keys().copied().collect();
            for node_id in node_ids {
                // For now, create mock state - would integrate with real cluster state
                let role = if cluster.get_leader_id().await.ok() == Some(node_id) {
                    NodeRole::Leader
                } else {
                    NodeRole::Follower
                };

                nodes.insert(
                    node_id,
                    NodeState {
                        id: node_id,
                        role,
                        is_running: true, // Would track stopped nodes
                        last_heartbeat: self.time.now(node_id).duration_since_epoch().as_nanos(),
                        log_length: 0,    // Would get from Raft
                        applied_index: 0, // Would get from Raft
                        clock_skew_ms: 0, // Would calculate from time accelerator
                        byzantine: self
                            .byzantine_nodes
                            .read()
                            .await
                            .get(&node_id)
                            .map(|b| format!("{:?}", b)),
                    },
                );

                views.insert(node_id, 1); // Would get actual term
                commit_points.insert(node_id, 0); // Would get actual commit index
            }

            // VM states would be collected from the cluster's VM manager
        }

        StateSnapshot {
            timestamp: self.time.now(1).duration_since_epoch().as_nanos(), // Use node 1's time as reference
            nodes,
            views,
            commit_points,
            vms,
            partitions: self.partitions.read().await.clone(),
            messages_in_flight: 0, // Would track from message filter
            resources: ResourceUsage {
                total_cpu_percent: 0.0,
                total_memory_mb: 0,
                total_disk_io_mb: 0,
                network_bandwidth_mbps: 0.0,
            },
            violations: vec![],
        }
    }

    /// Get current coverage
    pub fn get_coverage(&self) -> Coverage {
        // In a real implementation, this would collect coverage from instrumented code
        Coverage::default()
    }
}

/// Represents the state of the test cluster
pub struct ClusterState {
    pub nodes: Vec<u64>,
    pub leader: Option<u64>,
    pub vms: HashMap<String, VmState>,
}

// Helper functions

fn hash_operation(op_type: &str, node_id: u64) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    op_type.hash(&mut hasher);
    node_id.hash(&mut hasher);
    hasher.finish()
}

fn hash_client_op(op: &ClientOp) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{:?}", op).hash(&mut hasher);
    hasher.finish()
}
