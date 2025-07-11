//! Event handling traits for component communication
//!
//! These traits define the interfaces for handling various node events,
//! allowing components to communicate through well-defined interfaces
//! rather than direct method calls.

use crate::{
    error::BlixardResult,
    types::VmStatus,
    raft_manager::{WorkerStatus, TaskResult},
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Events that can occur within the node system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeEvent {
    /// Raft consensus status has changed
    RaftStatusUpdate {
        term: u64,
        is_leader: bool,
        leader_id: Option<u64>,
        commit_index: u64,
        applied_index: u64,
    },
    
    /// Peer connection status has changed
    PeerConnectionChange {
        peer_id: u64,
        address: String,
        connected: bool,
    },
    
    /// VM status has changed
    VmStatusChange {
        vm_name: String,
        old_status: VmStatus,
        new_status: VmStatus,
        node_id: u64,
    },
    
    /// Worker status has changed
    WorkerStatusChange {
        worker_id: u64,
        old_status: WorkerStatus,
        new_status: WorkerStatus,
    },
    
    /// Task has completed
    TaskCompleted {
        task_id: String,
        worker_id: u64,
        result: TaskResult,
    },
    
    /// Cluster membership has changed
    ClusterMembershipChange {
        added_nodes: Vec<u64>,
        removed_nodes: Vec<u64>,
    },
}

/// Handler for node events
#[async_trait]
pub trait NodeEventHandler: Send + Sync {
    /// Handle a node event
    async fn handle_event(&self, event: NodeEvent) -> BlixardResult<()>;
    
    /// Get the name/identifier for this handler (for debugging)
    fn handler_name(&self) -> &'static str;
    
    /// Check if this handler is interested in a specific event type
    fn handles_event_type(&self, event: &NodeEvent) -> bool {
        // By default, handle all events
        match event {
            NodeEvent::RaftStatusUpdate { .. } => self.handles_raft_status_updates(),
            NodeEvent::PeerConnectionChange { .. } => self.handles_peer_connection_changes(),
            NodeEvent::VmStatusChange { .. } => self.handles_vm_status_changes(),
            NodeEvent::WorkerStatusChange { .. } => self.handles_worker_status_changes(),
            NodeEvent::TaskCompleted { .. } => self.handles_task_completion(),
            NodeEvent::ClusterMembershipChange { .. } => self.handles_cluster_membership_changes(),
        }
    }
    
    // Override these methods to specify which events this handler cares about
    fn handles_raft_status_updates(&self) -> bool { true }
    fn handles_peer_connection_changes(&self) -> bool { true }
    fn handles_vm_status_changes(&self) -> bool { true }
    fn handles_worker_status_changes(&self) -> bool { true }
    fn handles_task_completion(&self) -> bool { true }
    fn handles_cluster_membership_changes(&self) -> bool { true }
}

/// Event bus for distributing events to handlers
#[async_trait]
pub trait EventBus: Send + Sync {
    /// Register a new event handler
    async fn register_handler(&self, handler: Box<dyn NodeEventHandler>) -> BlixardResult<()>;
    
    /// Unregister an event handler by name
    async fn unregister_handler(&self, handler_name: &str) -> BlixardResult<()>;
    
    /// Emit an event to all registered handlers
    async fn emit_event(&self, event: NodeEvent) -> BlixardResult<()>;
    
    /// Get the number of registered handlers
    async fn handler_count(&self) -> usize;
}

/// Specific event handler traits for common patterns

#[async_trait]
pub trait RaftEventHandler: Send + Sync {
    async fn on_raft_status_update(
        &self,
        term: u64,
        is_leader: bool,
        leader_id: Option<u64>,
        commit_index: u64,
        applied_index: u64,
    ) -> BlixardResult<()>;
    
    async fn on_cluster_membership_change(
        &self,
        added_nodes: Vec<u64>,
        removed_nodes: Vec<u64>,
    ) -> BlixardResult<()>;
}

#[async_trait]
pub trait VmEventHandler: Send + Sync {
    async fn on_vm_status_change(
        &self,
        vm_name: &str,
        old_status: VmStatus,
        new_status: VmStatus,
        node_id: u64,
    ) -> BlixardResult<()>;
}

#[async_trait]
pub trait PeerEventHandler: Send + Sync {
    async fn on_peer_connection_change(
        &self,
        peer_id: u64,
        address: &str,
        connected: bool,
    ) -> BlixardResult<()>;
}

/// Adapter to convert specific handlers to general NodeEventHandler
pub struct RaftEventAdapter<T: RaftEventHandler> {
    handler: T,
}

impl<T: RaftEventHandler> RaftEventAdapter<T> {
    pub fn new(handler: T) -> Self {
        Self { handler }
    }
}

#[async_trait]
impl<T: RaftEventHandler> NodeEventHandler for RaftEventAdapter<T> {
    async fn handle_event(&self, event: NodeEvent) -> BlixardResult<()> {
        match event {
            NodeEvent::RaftStatusUpdate { term, is_leader, leader_id, commit_index, applied_index } => {
                self.handler.on_raft_status_update(term, is_leader, leader_id, commit_index, applied_index).await
            }
            NodeEvent::ClusterMembershipChange { added_nodes, removed_nodes } => {
                self.handler.on_cluster_membership_change(added_nodes, removed_nodes).await
            }
            _ => Ok(()), // Ignore non-Raft events
        }
    }
    
    fn handler_name(&self) -> &'static str {
        "RaftEventAdapter"
    }
    
    fn handles_raft_status_updates(&self) -> bool { true }
    fn handles_cluster_membership_changes(&self) -> bool { true }
    fn handles_peer_connection_changes(&self) -> bool { false }
    fn handles_vm_status_changes(&self) -> bool { false }
    fn handles_worker_status_changes(&self) -> bool { false }
    fn handles_task_completion(&self) -> bool { false }
}