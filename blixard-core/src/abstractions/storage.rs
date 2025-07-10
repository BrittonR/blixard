//! Storage abstractions for repository pattern
//!
//! This module provides trait-based abstractions for storage operations,
//! enabling testing without direct database dependencies.

use crate::{
    error::BlixardResult,
    raft_manager::TaskSpec,
    types::{VmConfig, VmStatus},
};
use async_trait::async_trait;
use std::collections::HashMap;

/// Repository for VM-related storage operations
#[async_trait]
pub trait VmRepository: Send + Sync {
    /// Create a new VM configuration
    async fn create(&self, vm: &VmConfig) -> BlixardResult<()>;

    /// Get VM configuration by name
    async fn get(&self, name: &str) -> BlixardResult<Option<VmConfig>>;

    /// List all VM configurations
    async fn list(&self) -> BlixardResult<Vec<VmConfig>>;

    /// Update VM configuration
    async fn update(&self, vm: &VmConfig) -> BlixardResult<()>;

    /// Delete VM configuration
    async fn delete(&self, name: &str) -> BlixardResult<()>;

    /// Get VM status
    async fn get_status(&self, name: &str) -> BlixardResult<Option<VmStatus>>;

    /// Update VM status
    async fn update_status(&self, name: &str, status: VmStatus) -> BlixardResult<()>;

    /// List all VMs with their status
    async fn list_with_status(&self) -> BlixardResult<Vec<(VmConfig, VmStatus)>>;
}

/// Repository for task-related storage operations
#[async_trait]
pub trait TaskRepository: Send + Sync {
    /// Create a new task
    async fn create(&self, task_id: &str, task: &TaskSpec) -> BlixardResult<()>;

    /// Get task by ID
    async fn get(&self, task_id: &str) -> BlixardResult<Option<TaskSpec>>;

    /// Update task status
    async fn update_status(&self, task_id: &str, status: &str) -> BlixardResult<()>;

    /// List tasks by status
    async fn list_by_status(&self, status: &str) -> BlixardResult<Vec<(String, TaskSpec)>>;

    /// Delete task
    async fn delete(&self, task_id: &str) -> BlixardResult<()>;
}

/// Repository for node-related storage operations
#[async_trait]
pub trait NodeRepository: Send + Sync {
    /// Store node information
    async fn store_node_info(&self, node_id: u64, bind_address: &str) -> BlixardResult<()>;

    /// Get node information
    async fn get_node_info(&self, node_id: u64) -> BlixardResult<Option<String>>;

    /// List all nodes
    async fn list_nodes(&self) -> BlixardResult<HashMap<u64, String>>;

    /// Remove node
    async fn remove_node(&self, node_id: u64) -> BlixardResult<()>;

    /// Update node health status
    async fn update_node_health(&self, node_id: u64, healthy: bool) -> BlixardResult<()>;
}

// Production implementations using redb

use redb::Database;
use std::sync::Arc;

/// Production VM repository using redb
pub struct RedbVmRepository {
    database: Arc<Database>,
}

impl RedbVmRepository {
    /// Create new repository instance
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }
}

#[async_trait]
impl VmRepository for RedbVmRepository {
    async fn create(&self, vm: &VmConfig) -> BlixardResult<()> {
        use crate::raft_storage::VM_STATE_TABLE;

        let db = self.database.clone();
        let vm_name = vm.name.clone();
        let vm_bytes = bincode::serialize(vm)?;

        tokio::task::spawn_blocking(move || {
            let write_txn = db.begin_write()?;
            {
                let mut table = write_txn.open_table(VM_STATE_TABLE)?;
                table.insert(vm_name.as_str(), vm_bytes.as_slice())?;
            }
            write_txn.commit()?;
            Ok(())
        })
        .await
        .map_err(|e| crate::error::BlixardError::Internal {
            message: format!("Task join error: {}", e),
        })?
    }

    async fn get(&self, name: &str) -> BlixardResult<Option<VmConfig>> {
        use crate::raft_storage::VM_STATE_TABLE;

        let db = self.database.clone();
        let vm_name = name.to_string();

        tokio::task::spawn_blocking(move || {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(VM_STATE_TABLE)?;

            match table.get(vm_name.as_str())? {
                Some(bytes) => {
                    let vm: VmConfig = bincode::deserialize(bytes.value())?;
                    Ok(Some(vm))
                }
                None => Ok(None),
            }
        })
        .await
        .map_err(|e| crate::error::BlixardError::Internal {
            message: format!("Task join error: {}", e),
        })?
    }

    async fn list(&self) -> BlixardResult<Vec<VmConfig>> {
        use crate::raft_storage::VM_STATE_TABLE;

        let db = self.database.clone();

        tokio::task::spawn_blocking(move || {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(VM_STATE_TABLE)?;

            let mut vms = Vec::new();
            let iter = table.range::<&str>(..)?;
            for entry in iter {
                let (_, bytes) = entry?;
                let vm: VmConfig = bincode::deserialize(bytes.value())?;
                vms.push(vm);
            }

            Ok(vms)
        })
        .await
        .map_err(|e| crate::error::BlixardError::Internal {
            message: format!("Task join error: {}", e),
        })?
    }

    async fn update(&self, vm: &VmConfig) -> BlixardResult<()> {
        // Same as create for now - redb handles updates
        self.create(vm).await
    }

    async fn delete(&self, name: &str) -> BlixardResult<()> {
        use crate::raft_storage::VM_STATE_TABLE;

        let db = self.database.clone();
        let vm_name = name.to_string();

        tokio::task::spawn_blocking(move || {
            let write_txn = db.begin_write()?;
            {
                let mut table = write_txn.open_table(VM_STATE_TABLE)?;
                table.remove(vm_name.as_str())?;
            }
            write_txn.commit()?;
            Ok(())
        })
        .await
        .map_err(|e| crate::error::BlixardError::Internal {
            message: format!("Task join error: {}", e),
        })?
    }

    async fn get_status(&self, name: &str) -> BlixardResult<Option<VmStatus>> {
        use crate::raft_storage::VM_STATE_TABLE;

        let db = self.database.clone();
        let vm_name = name.to_string();

        tokio::task::spawn_blocking(move || {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(VM_STATE_TABLE)?;

            match table.get(vm_name.as_str())? {
                Some(bytes) => {
                    let status: VmStatus = bincode::deserialize(bytes.value())?;
                    Ok(Some(status))
                }
                None => Ok(None),
            }
        })
        .await
        .map_err(|e| crate::error::BlixardError::Internal {
            message: format!("Task join error: {}", e),
        })?
    }

    async fn update_status(&self, name: &str, status: VmStatus) -> BlixardResult<()> {
        use crate::raft_storage::VM_STATE_TABLE;

        let db = self.database.clone();
        let vm_name = name.to_string();
        let status_bytes = bincode::serialize(&status)?;

        tokio::task::spawn_blocking(move || {
            let write_txn = db.begin_write()?;
            {
                let mut table = write_txn.open_table(VM_STATE_TABLE)?;
                table.insert(vm_name.as_str(), status_bytes.as_slice())?;
            }
            write_txn.commit()?;
            Ok(())
        })
        .await
        .map_err(|e| crate::error::BlixardError::Internal {
            message: format!("Task join error: {}", e),
        })?
    }

    async fn list_with_status(&self) -> BlixardResult<Vec<(VmConfig, VmStatus)>> {
        let vms = self.list().await?;
        let mut result = Vec::new();

        for vm in vms {
            let status = self
                .get_status(&vm.name)
                .await?
                .unwrap_or(VmStatus::Stopped);
            result.push((vm, status));
        }

        Ok(result)
    }
}

// Test implementations using in-memory storage

use tokio::sync::RwLock;

/// Mock VM repository for testing
pub struct MockVmRepository {
    vms: Arc<RwLock<HashMap<String, VmConfig>>>,
    statuses: Arc<RwLock<HashMap<String, VmStatus>>>,
}

impl MockVmRepository {
    /// Create new mock repository
    pub fn new() -> Self {
        Self {
            vms: Arc::new(RwLock::new(HashMap::new())),
            statuses: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for MockVmRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VmRepository for MockVmRepository {
    async fn create(&self, vm: &VmConfig) -> BlixardResult<()> {
        self.vms.write().await.insert(vm.name.clone(), vm.clone());
        Ok(())
    }

    async fn get(&self, name: &str) -> BlixardResult<Option<VmConfig>> {
        Ok(self.vms.read().await.get(name).cloned())
    }

    async fn list(&self) -> BlixardResult<Vec<VmConfig>> {
        Ok(self.vms.read().await.values().cloned().collect())
    }

    async fn update(&self, vm: &VmConfig) -> BlixardResult<()> {
        self.vms.write().await.insert(vm.name.clone(), vm.clone());
        Ok(())
    }

    async fn delete(&self, name: &str) -> BlixardResult<()> {
        self.vms.write().await.remove(name);
        self.statuses.write().await.remove(name);
        Ok(())
    }

    async fn get_status(&self, name: &str) -> BlixardResult<Option<VmStatus>> {
        Ok(self.statuses.read().await.get(name).cloned())
    }

    async fn update_status(&self, name: &str, status: VmStatus) -> BlixardResult<()> {
        self.statuses.write().await.insert(name.to_string(), status);
        Ok(())
    }

    async fn list_with_status(&self) -> BlixardResult<Vec<(VmConfig, VmStatus)>> {
        let vms = self.vms.read().await;
        let statuses = self.statuses.read().await;

        let mut result = Vec::new();
        for (name, vm) in vms.iter() {
            let status = statuses.get(name).cloned().unwrap_or(VmStatus::Stopped);
            result.push((vm.clone(), status));
        }

        Ok(result)
    }
}
