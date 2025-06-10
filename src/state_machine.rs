use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::storage::Storage;
use crate::types::*;

/// Commands that can be applied to the state machine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateMachineCommand {
    // Service management
    CreateService { info: ServiceInfo },
    UpdateService { name: String, info: ServiceInfo },
    DeleteService { name: String },
    
    // Node management
    RegisterNode { id: u64, info: NodeInfo },
    UpdateNodeHeartbeat { id: u64, timestamp: chrono::DateTime<chrono::Utc> },
    RemoveNode { id: u64 },
    
    // VM/microVM management
    CreateVm { vm: VmState },
    UpdateVmStatus { name: String, status: VmStatus },
    DeleteVm { name: String },
    
    // Cluster events
    ClusterEvent { event: ClusterEvent },
    
    // Configuration changes (handled specially by Raft)
    AddNode { node_id: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: String,
    pub state: ServiceState,
    pub node: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceState {
    Running,
    Stopped,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: u64,
    pub address: String,
    pub tailscale_hostname: Option<String>,
    pub last_seen: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClusterEvent {
    NodeJoined { node_id: u64 },
    NodeLeft { node_id: u64 },
    LeaderChanged { new_leader: u64 },
}

/// The state machine that applies Raft log entries to storage
pub struct StateMachine {
    storage: Arc<Storage>,
}

impl StateMachine {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }
    
    /// Apply a committed Raft log entry to the state machine
    pub async fn apply(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        let command: StateMachineCommand = bincode::deserialize(data)
            .context("Failed to deserialize command")?;
        
        debug!("Applying command: {:?}", command);
        
        match command {
            StateMachineCommand::CreateService { info } => {
                self.apply_create_service(info).await
            }
            StateMachineCommand::UpdateService { name, info } => {
                self.apply_update_service(name, info).await
            }
            StateMachineCommand::DeleteService { name } => {
                self.apply_delete_service(name).await
            }
            StateMachineCommand::RegisterNode { id, info } => {
                self.apply_register_node(id, info).await
            }
            StateMachineCommand::UpdateNodeHeartbeat { id, timestamp } => {
                self.apply_node_heartbeat(id, timestamp).await
            }
            StateMachineCommand::RemoveNode { id } => {
                self.apply_remove_node(id).await
            }
            StateMachineCommand::CreateVm { vm } => {
                self.apply_create_vm(vm).await
            }
            StateMachineCommand::UpdateVmStatus { name, status } => {
                self.apply_update_vm_status(name, status).await
            }
            StateMachineCommand::DeleteVm { name } => {
                self.apply_delete_vm(name).await
            }
            StateMachineCommand::ClusterEvent { event } => {
                self.apply_cluster_event(event).await
            }
            StateMachineCommand::AddNode { node_id: _ } => {
                // AddNode is handled by Raft as a configuration change
                // It shouldn't reach the state machine
                warn!("AddNode command reached state machine - should be handled as conf change");
                Ok(Vec::new())
            }
        }
    }
    
    // Service management implementations
    async fn apply_create_service(&mut self, info: ServiceInfo) -> Result<Vec<u8>> {
        self.storage.store_service(&info)?;
        info!("Created service: {}", info.name);
        Ok(vec![])
    }
    
    async fn apply_update_service(&mut self, name: String, info: ServiceInfo) -> Result<Vec<u8>> {
        self.storage.store_service(&info)?;
        info!("Updated service: {}", name);
        Ok(vec![])
    }
    
    async fn apply_delete_service(&mut self, name: String) -> Result<Vec<u8>> {
        self.storage.delete_service(&name)?;
        info!("Deleted service: {}", name);
        Ok(vec![])
    }
    
    // Node management implementations
    async fn apply_register_node(&mut self, id: u64, info: NodeInfo) -> Result<Vec<u8>> {
        self.storage.store_node(id, &info)?;
        info!("Registered node: {}", id);
        Ok(vec![])
    }
    
    async fn apply_node_heartbeat(&mut self, id: u64, timestamp: chrono::DateTime<chrono::Utc>) -> Result<Vec<u8>> {
        if let Some(mut node) = self.storage.get_node(id)? {
            node.last_seen = timestamp;
            self.storage.store_node(id, &node)?;
            debug!("Updated heartbeat for node: {}", id);
        } else {
            warn!("Heartbeat for unknown node: {}", id);
        }
        Ok(vec![])
    }
    
    async fn apply_remove_node(&mut self, id: u64) -> Result<Vec<u8>> {
        self.storage.delete_node(id)?;
        info!("Removed node: {}", id);
        Ok(vec![])
    }
    
    // VM management implementations
    async fn apply_create_vm(&mut self, vm: VmState) -> Result<Vec<u8>> {
        self.storage.store_vm(&vm)?;
        info!("Created VM: {}", vm.name);
        Ok(vec![])
    }
    
    async fn apply_update_vm_status(&mut self, name: String, status: VmStatus) -> Result<Vec<u8>> {
        if let Some(mut vm) = self.storage.get_vm_async(&name)? {
            vm.status = status;
            vm.updated_at = chrono::Utc::now();
            self.storage.store_vm(&vm)?;
            info!("Updated VM {} status to {:?}", name, status);
        } else {
            warn!("Update status for unknown VM: {}", name);
        }
        Ok(vec![])
    }
    
    async fn apply_delete_vm(&mut self, name: String) -> Result<Vec<u8>> {
        self.storage.delete_vm_async(&name)?;
        info!("Deleted VM: {}", name);
        Ok(vec![])
    }
    
    // Cluster event implementations
    async fn apply_cluster_event(&mut self, event: ClusterEvent) -> Result<Vec<u8>> {
        match event {
            ClusterEvent::NodeJoined { node_id } => {
                info!("Node {} joined the cluster", node_id);
            }
            ClusterEvent::NodeLeft { node_id } => {
                info!("Node {} left the cluster", node_id);
            }
            ClusterEvent::LeaderChanged { new_leader } => {
                info!("Leader changed to node {}", new_leader);
            }
        }
        // Store event in storage for audit/history
        self.storage.store_cluster_event(&event)?;
        Ok(vec![])
    }
    
    /// Create a snapshot of the current state
    pub async fn snapshot(&self) -> Result<Vec<u8>> {
        // Collect all state into a snapshot
        let services = self.storage.list_services()?;
        let nodes = self.storage.list_nodes()?;
        let vms = self.storage.list_vms_async()?;
        
        let snapshot = StateSnapshot {
            services,
            nodes,
            vms,
            timestamp: chrono::Utc::now(),
        };
        
        bincode::serialize(&snapshot)
            .context("Failed to serialize snapshot")
    }
    
    /// Restore state from a snapshot
    pub async fn restore_snapshot(&mut self, data: &[u8]) -> Result<()> {
        let snapshot: StateSnapshot = bincode::deserialize(data)
            .context("Failed to deserialize snapshot")?;
        
        info!("Restoring snapshot from {}", snapshot.timestamp);
        
        // Clear existing state
        self.storage.clear_all()?;
        
        // Restore services
        for (_, service) in snapshot.services {
            self.storage.store_service(&service)?;
        }
        
        // Restore nodes
        for (id, node) in snapshot.nodes {
            self.storage.store_node(id, &node)?;
        }
        
        // Restore VMs
        for (_, vm) in snapshot.vms {
            self.storage.store_vm(&vm)?;
        }
        
        info!("Snapshot restoration complete");
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct StateSnapshot {
    services: std::collections::HashMap<String, ServiceInfo>,
    nodes: std::collections::HashMap<u64, NodeInfo>,
    vms: std::collections::HashMap<String, VmState>,
    timestamp: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_state_machine_service_lifecycle() {
        let storage = Arc::new(Storage::new_test().unwrap());
        let mut state_machine = StateMachine::new(storage.clone());
        
        // Create service
        let create_cmd = StateMachineCommand::CreateService {
            info: ServiceInfo {
                name: "test-service".to_string(),
                state: ServiceState::Running,
                node: "node1".to_string(),
                timestamp: chrono::Utc::now(),
            },
        };
        
        let data = bincode::serialize(&create_cmd).unwrap();
        state_machine.apply(&data).await.unwrap();
        
        // Verify service exists
        let service = storage.get_service("test-service").unwrap();
        assert!(service.is_some());
        assert_eq!(service.unwrap().state, ServiceState::Running);
        
        // Update service
        let update_cmd = StateMachineCommand::UpdateService {
            name: "test-service".to_string(),
            info: ServiceInfo {
                name: "test-service".to_string(),
                state: ServiceState::Stopped,
                node: "node1".to_string(),
                timestamp: chrono::Utc::now(),
            },
        };
        
        let data = bincode::serialize(&update_cmd).unwrap();
        state_machine.apply(&data).await.unwrap();
        
        // Verify update
        let service = storage.get_service("test-service").unwrap().unwrap();
        assert_eq!(service.state, ServiceState::Stopped);
        
        // Delete service
        let delete_cmd = StateMachineCommand::DeleteService {
            name: "test-service".to_string(),
        };
        
        let data = bincode::serialize(&delete_cmd).unwrap();
        state_machine.apply(&data).await.unwrap();
        
        // Verify deletion
        let service = storage.get_service("test-service").unwrap();
        assert!(service.is_none());
    }
    
    #[tokio::test]
    async fn test_snapshot_restore() {
        let storage = Arc::new(Storage::new_test().unwrap());
        let mut state_machine = StateMachine::new(storage.clone());
        
        // Add some state
        let service_info = ServiceInfo {
            name: "test-service".to_string(),
            state: ServiceState::Running,
            node: "node1".to_string(),
            timestamp: chrono::Utc::now(),
        };
        storage.store_service(&service_info).unwrap();
        
        // Create snapshot
        let snapshot_data = state_machine.snapshot().await.unwrap();
        
        // Clear state
        storage.clear_all().unwrap();
        assert!(storage.get_service("test-service").unwrap().is_none());
        
        // Restore from snapshot
        state_machine.restore_snapshot(&snapshot_data).await.unwrap();
        
        // Verify restoration
        let service = storage.get_service("test-service").unwrap().unwrap();
        assert_eq!(service.name, "test-service");
        assert_eq!(service.state, ServiceState::Running);
    }
}