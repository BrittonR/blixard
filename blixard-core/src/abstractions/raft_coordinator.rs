//! Raft coordination traits for breaking circular dependencies
//!
//! These traits provide interfaces for Raft-related operations without
//! requiring direct access to the full shared state.

use crate::{
    error::BlixardResult,
    abstractions::node_events::{EventBus, NodeEvent},
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Represents the current status of the Raft consensus module
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RaftStatus {
    /// Current Raft term
    pub term: u64,
    
    /// Whether this node is the current leader
    pub is_leader: bool,
    
    /// ID of the current leader (if known)
    pub leader_id: Option<u64>,
    
    /// Index of the latest committed entry
    pub commit_index: u64,
    
    /// Index of the latest applied entry
    pub applied_index: u64,
    
    /// Number of connected peers
    pub connected_peers: usize,
    
    /// Total cluster size
    pub cluster_size: usize,
    
    /// Whether the node is part of a stable cluster
    pub has_quorum: bool,
}

impl Default for RaftStatus {
    fn default() -> Self {
        Self {
            term: 0,
            is_leader: false,
            leader_id: None,
            commit_index: 0,
            applied_index: 0,
            connected_peers: 0,
            cluster_size: 1,
            has_quorum: false,
        }
    }
}

/// Trait for coordinating Raft operations without tight coupling
#[async_trait]
pub trait RaftCoordinator: Send + Sync {
    /// Update the Raft status and notify interested parties
    async fn update_raft_status(&self, status: RaftStatus) -> BlixardResult<()>;
    
    /// Get the current Raft status
    async fn get_raft_status(&self) -> RaftStatus;
    
    /// Check if this node is currently the leader
    async fn is_leader(&self) -> bool {
        self.get_raft_status().await.is_leader
    }
    
    /// Get the current leader ID (if known)
    async fn get_leader_id(&self) -> Option<u64> {
        self.get_raft_status().await.leader_id
    }
    
    /// Get the current term
    async fn get_current_term(&self) -> u64 {
        self.get_raft_status().await.term
    }
    
    /// Check if the cluster has quorum
    async fn has_quorum(&self) -> bool {
        self.get_raft_status().await.has_quorum
    }
    
    /// Notify about cluster membership changes
    async fn notify_membership_change(
        &self,
        added_nodes: Vec<u64>,
        removed_nodes: Vec<u64>,
    ) -> BlixardResult<()>;
}

/// Implementation of RaftCoordinator that uses an event bus
pub struct EventDrivenRaftCoordinator {
    /// Current Raft status
    current_status: tokio::sync::RwLock<RaftStatus>,
    
    /// Event bus for notifying about status changes
    event_bus: Arc<dyn EventBus>,
}

impl EventDrivenRaftCoordinator {
    /// Create a new event-driven Raft coordinator
    pub fn new(event_bus: Arc<dyn EventBus>) -> Self {
        Self {
            current_status: tokio::sync::RwLock::new(RaftStatus::default()),
            event_bus,
        }
    }
    
    /// Create a new coordinator with initial status
    pub fn with_initial_status(event_bus: Arc<dyn EventBus>, initial_status: RaftStatus) -> Self {
        Self {
            current_status: tokio::sync::RwLock::new(initial_status),
            event_bus,
        }
    }
}

#[async_trait]
impl RaftCoordinator for EventDrivenRaftCoordinator {
    async fn update_raft_status(&self, status: RaftStatus) -> BlixardResult<()> {
        // Update the stored status
        let old_status = {
            let mut current = self.current_status.write().await;
            let old = current.clone();
            *current = status.clone();
            old
        };
        
        // Emit event if the status actually changed
        if old_status != status {
            let event = NodeEvent::RaftStatusUpdate {
                term: status.term,
                is_leader: status.is_leader,
                leader_id: status.leader_id,
                commit_index: status.commit_index,
                applied_index: status.applied_index,
            };
            
            self.event_bus.emit_event(event).await?;
        }
        
        Ok(())
    }
    
    async fn get_raft_status(&self) -> RaftStatus {
        self.current_status.read().await.clone()
    }
    
    async fn notify_membership_change(
        &self,
        added_nodes: Vec<u64>,
        removed_nodes: Vec<u64>,
    ) -> BlixardResult<()> {
        let event = NodeEvent::ClusterMembershipChange {
            added_nodes,
            removed_nodes,
        };
        
        self.event_bus.emit_event(event).await
    }
}

/// Mock implementation for testing
#[cfg(test)]
pub struct MockRaftCoordinator {
    status: tokio::sync::RwLock<RaftStatus>,
    update_calls: tokio::sync::RwLock<Vec<RaftStatus>>,
}

#[cfg(test)]
impl MockRaftCoordinator {
    pub fn new() -> Self {
        Self {
            status: tokio::sync::RwLock::new(RaftStatus::default()),
            update_calls: tokio::sync::RwLock::new(Vec::new()),
        }
    }
    
    pub fn with_status(status: RaftStatus) -> Self {
        Self {
            status: tokio::sync::RwLock::new(status),
            update_calls: tokio::sync::RwLock::new(Vec::new()),
        }
    }
    
    pub async fn get_update_calls(&self) -> Vec<RaftStatus> {
        self.update_calls.read().await.clone()
    }
}

#[cfg(test)]
#[async_trait]
impl RaftCoordinator for MockRaftCoordinator {
    async fn update_raft_status(&self, status: RaftStatus) -> BlixardResult<()> {
        *self.status.write().await = status.clone();
        self.update_calls.write().await.push(status);
        Ok(())
    }
    
    async fn get_raft_status(&self) -> RaftStatus {
        self.status.read().await.clone()
    }
    
    async fn notify_membership_change(
        &self,
        _added_nodes: Vec<u64>,
        _removed_nodes: Vec<u64>,
    ) -> BlixardResult<()> {
        // Mock implementation - just return success
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abstractions::node_events::NodeEvent;
    use std::sync::Arc;
    
    // Mock event bus for testing
    struct MockEventBus {
        events: tokio::sync::RwLock<Vec<NodeEvent>>,
    }
    
    impl MockEventBus {
        fn new() -> Self {
            Self {
                events: tokio::sync::RwLock::new(Vec::new()),
            }
        }
        
        async fn get_events(&self) -> Vec<NodeEvent> {
            self.events.read().await.clone()
        }
    }
    
    #[async_trait]
    impl EventBus for MockEventBus {
        async fn register_handler(&self, _handler: Box<dyn crate::abstractions::node_events::NodeEventHandler>) -> BlixardResult<()> {
            Ok(())
        }
        
        async fn unregister_handler(&self, _handler_name: &str) -> BlixardResult<()> {
            Ok(())
        }
        
        async fn emit_event(&self, event: NodeEvent) -> BlixardResult<()> {
            self.events.write().await.push(event);
            Ok(())
        }
        
        async fn handler_count(&self) -> usize {
            0
        }
    }
    
    #[tokio::test]
    async fn test_raft_coordinator_status_update() {
        let event_bus = Arc::new(MockEventBus::new());
        let coordinator = EventDrivenRaftCoordinator::new(event_bus.clone());
        
        let status = RaftStatus {
            term: 5,
            is_leader: true,
            leader_id: Some(1),
            commit_index: 10,
            applied_index: 10,
            connected_peers: 2,
            cluster_size: 3,
            has_quorum: true,
        };
        
        coordinator.update_raft_status(status.clone()).await.unwrap();
        
        // Check that status was updated
        let current_status = coordinator.get_raft_status().await;
        assert_eq!(current_status, status);
        
        // Check that event was emitted
        let events = event_bus.get_events().await;
        assert_eq!(events.len(), 1);
        
        match &events[0] {
            NodeEvent::RaftStatusUpdate { term, is_leader, leader_id, commit_index, applied_index } => {
                assert_eq!(*term, 5);
                assert_eq!(*is_leader, true);
                assert_eq!(*leader_id, Some(1));
                assert_eq!(*commit_index, 10);
                assert_eq!(*applied_index, 10);
            }
            _ => panic!("Expected RaftStatusUpdate event"),
        }
    }
    
    #[tokio::test]
    async fn test_raft_coordinator_no_duplicate_events() {
        let event_bus = Arc::new(MockEventBus::new());
        let coordinator = EventDrivenRaftCoordinator::new(event_bus.clone());
        
        let status = RaftStatus {
            term: 1,
            is_leader: false,
            leader_id: None,
            commit_index: 0,
            applied_index: 0,
            connected_peers: 1,
            cluster_size: 1,
            has_quorum: false,
        };
        
        // Update with same status twice
        coordinator.update_raft_status(status.clone()).await.unwrap();
        coordinator.update_raft_status(status).await.unwrap();
        
        // Should only have one event (no duplicate events for same status)
        let events = event_bus.get_events().await;
        assert_eq!(events.len(), 1);
    }
}