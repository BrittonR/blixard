use anyhow::Result;
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::debug;

use crate::types::*;
use crate::state_machine::{ServiceInfo, NodeInfo as StateMachineNodeInfo, ClusterEvent};

// Define tables
const VM_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("vms");
const NODE_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("nodes");
const CONFIG_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("config");
const SERVICE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("services");
const RAFT_LOG_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("raft_log");
const RAFT_STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("raft_state");
const CLUSTER_EVENT_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("cluster_events");

pub struct Storage {
    db: Database,
}

impl std::fmt::Debug for Storage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Storage")
            .field("db", &"<redb::Database>")
            .finish()
    }
}

impl Storage {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let db = Database::create(path)?;
        
        // Initialize tables
        let write_txn = db.begin_write()?;
        {
            let _ = write_txn.open_table(VM_TABLE)?;
            let _ = write_txn.open_table(NODE_TABLE)?;
            let _ = write_txn.open_table(CONFIG_TABLE)?;
            let _ = write_txn.open_table(SERVICE_TABLE)?;
            let _ = write_txn.open_table(RAFT_LOG_TABLE)?;
            let _ = write_txn.open_table(RAFT_STATE_TABLE)?;
            let _ = write_txn.open_table(CLUSTER_EVENT_TABLE)?;
        }
        write_txn.commit()?;
        
        Ok(Self { db })
    }
    
    /// Create a test storage with temporary database
    pub fn new_test() -> Result<Self> {
        use tempfile::tempdir;
        let temp_dir = tempdir()?;
        let db_path = temp_dir.path().join("test.db");
        Self::new(db_path)
    }
    
    // VM operations
    pub fn save_vm(&self, vm: &VmState) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(VM_TABLE)?;
            let data = bincode::serialize(vm)?;
            table.insert(vm.name.as_str(), data.as_slice())?;
        }
        write_txn.commit()?;
        debug!("Saved VM: {}", vm.name);
        Ok(())
    }
    
    pub fn get_vm(&self, name: &str) -> Result<Option<VmState>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(VM_TABLE)?;
        
        match table.get(name)? {
            Some(data) => {
                let vm = bincode::deserialize(data.value())?;
                Ok(Some(vm))
            }
            None => Ok(None),
        }
    }
    
    pub fn list_vms(&self) -> Result<Vec<VmState>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(VM_TABLE)?;
        
        let mut vms = Vec::new();
        for result in table.iter()? {
            let (_key, value) = result?;
            let vm: VmState = bincode::deserialize(value.value())?;
            vms.push(vm);
        }
        
        Ok(vms)
    }
    
    pub fn delete_vm(&self, name: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(VM_TABLE)?;
            table.remove(name)?;
        }
        write_txn.commit()?;
        debug!("Deleted VM: {}", name);
        Ok(())
    }
    
    // Node operations
    pub fn save_node_info(&self, node_id: u64, info: &NodeInfo) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(NODE_TABLE)?;
            let data = bincode::serialize(info)?;
            table.insert(&node_id, data.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }
    
    pub fn get_node_info(&self, node_id: u64) -> Result<Option<NodeInfo>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(NODE_TABLE)?;
        
        match table.get(&node_id)? {
            Some(data) => {
                let info = bincode::deserialize(data.value())?;
                Ok(Some(info))
            }
            None => Ok(None),
        }
    }
    
    // Configuration
    pub fn save_config(&self, key: &str, value: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(CONFIG_TABLE)?;
            table.insert(key, value)?;
        }
        write_txn.commit()?;
        Ok(())
    }
    
    pub fn get_config(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(CONFIG_TABLE)?;
        
        match table.get(key)? {
            Some(data) => Ok(Some(data.value().to_vec())),
            None => Ok(None),
        }
    }
    
    // Service operations
    pub fn store_service(&self, service: &ServiceInfo) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SERVICE_TABLE)?;
            let data = bincode::serialize(service)?;
            table.insert(service.name.as_str(), data.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }
    
    pub fn get_service(&self, name: &str) -> Result<Option<ServiceInfo>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SERVICE_TABLE)?;
        
        let result = table.get(name)?
            .map(|data| bincode::deserialize(data.value()))
            .transpose()?;
        Ok(result)
    }
    
    pub fn delete_service(&self, name: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SERVICE_TABLE)?;
            table.remove(name)?;
        }
        write_txn.commit()?;
        Ok(())
    }
    
    pub fn list_services(&self) -> Result<std::collections::HashMap<String, ServiceInfo>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SERVICE_TABLE)?;
        
        let mut services = std::collections::HashMap::new();
        for result in table.iter()? {
            let (key, value) = result?;
            let service: ServiceInfo = bincode::deserialize(value.value())?;
            services.insert(key.value().to_string(), service);
        }
        
        Ok(services)
    }
    
    // Node operations (updated for state machine)
    pub fn store_node(&self, id: u64, node: &StateMachineNodeInfo) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(NODE_TABLE)?;
            let data = bincode::serialize(node)?;
            table.insert(&id, data.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }
    
    pub fn get_node(&self, id: u64) -> Result<Option<StateMachineNodeInfo>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(NODE_TABLE)?;
        
        match table.get(&id)? {
            Some(data) => {
                let node = bincode::deserialize(data.value())?;
                Ok(Some(node))
            }
            None => Ok(None),
        }
    }
    
    pub fn delete_node(&self, id: u64) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(NODE_TABLE)?;
            table.remove(&id)?;
        }
        write_txn.commit()?;
        Ok(())
    }
    
    pub fn list_nodes(&self) -> Result<std::collections::HashMap<u64, StateMachineNodeInfo>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(NODE_TABLE)?;
        
        let mut nodes = std::collections::HashMap::new();
        for result in table.iter()? {
            let (key, value) = result?;
            let node: StateMachineNodeInfo = bincode::deserialize(value.value())?;
            nodes.insert(key.value(), node);
        }
        
        Ok(nodes)
    }
    
    // VM operations (async wrappers for consistency)
    pub fn store_vm(&self, vm: &VmState) -> Result<()> {
        self.save_vm(vm)
    }
    
    pub fn get_vm_async(&self, name: &str) -> Result<Option<VmState>> {
        self.get_vm(name)
    }
    
    pub fn delete_vm_async(&self, name: &str) -> Result<()> {
        self.delete_vm(name)
    }
    
    pub fn list_vms_async(&self) -> Result<std::collections::HashMap<String, VmState>> {
        let vms = self.list_vms()?;
        let mut map = std::collections::HashMap::new();
        for vm in vms {
            map.insert(vm.name.clone(), vm);
        }
        Ok(map)
    }
    
    // Cluster event operations
    pub fn store_cluster_event(&self, event: &ClusterEvent) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(CLUSTER_EVENT_TABLE)?;
            let timestamp = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64;
            let data = bincode::serialize(event)?;
            table.insert(&timestamp, data.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }
    
    // Raft log operations
    pub fn append_raft_log(&self, index: u64, entry: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(RAFT_LOG_TABLE)?;
            table.insert(&index, entry)?;
        }
        write_txn.commit()?;
        Ok(())
    }
    
    pub fn get_raft_log(&self, index: u64) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(RAFT_LOG_TABLE)?;
        
        let result = table.get(&index)?
            .map(|data| data.value().to_vec());
        Ok(result)
    }
    
    pub fn get_raft_logs(&self, start: u64, end: u64) -> Result<Vec<(u64, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(RAFT_LOG_TABLE)?;
        
        let mut logs = Vec::new();
        for entry in table.range(start..=end)? {
            let (key, value) = entry?;
            logs.push((key.value(), value.value().to_vec()));
        }
        
        Ok(logs)
    }
    
    pub fn truncate_raft_log(&self, after_index: u64) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(RAFT_LOG_TABLE)?;
            
            // Get all entries after the specified index
            let mut to_remove = Vec::new();
            for entry in table.range((after_index + 1)..)? {
                let (k, _) = entry?;
                to_remove.push(k.value());
            }
            
            // Remove them
            for index in to_remove {
                table.remove(&index)?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }
    
    pub fn save_raft_state(&self, key: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(RAFT_STATE_TABLE)?;
            table.insert(key, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }
    
    pub fn get_raft_state(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(RAFT_STATE_TABLE)?;
        
        let result = table.get(key)?
            .map(|data| data.value().to_vec());
        Ok(result)
    }
    
    // Clear all data (for snapshot restoration)
    pub fn clear_all(&self) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            // Clear all tables
            let mut vm_table = write_txn.open_table(VM_TABLE)?;
            let mut node_table = write_txn.open_table(NODE_TABLE)?;
            let mut service_table = write_txn.open_table(SERVICE_TABLE)?;
            
            // Collect keys to remove (can't modify while iterating)
            let vm_keys: Vec<String> = vm_table.iter()?
                .map(|r| r.map(|(k, _)| k.value().to_string()))
                .collect::<Result<Vec<_>, _>>()?;
            let node_keys: Vec<u64> = node_table.iter()?
                .map(|r| r.map(|(k, _)| k.value()))
                .collect::<Result<Vec<_>, _>>()?;
            let service_keys: Vec<String> = service_table.iter()?
                .map(|r| r.map(|(k, _)| k.value().to_string()))
                .collect::<Result<Vec<_>, _>>()?;
            
            // Remove all entries
            for key in vm_keys {
                vm_table.remove(key.as_str())?;
            }
            for key in node_keys {
                node_table.remove(&key)?;
            }
            for key in service_keys {
                service_table.remove(key.as_str())?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }
    
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: u64,
    pub addr: String,
    pub tailscale_hostname: Option<String>,
    pub last_seen: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[test]
    fn test_storage_basic() -> Result<()> {
        let dir = tempdir()?;
        let storage = Storage::new(dir.path().join("test.db"))?;
        
        let vm = VmState {
            name: "test-vm".to_string(),
            config: VmConfig {
                name: "test-vm".to_string(),
                config_path: "/tmp/test.nix".to_string(),
                vcpus: 2,
                memory: 512,
            },
            status: VmStatus::Stopped,
            node_id: 1,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        
        // Save and retrieve
        storage.save_vm(&vm)?;
        let retrieved = storage.get_vm("test-vm")?;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "test-vm");
        
        // List
        let vms = storage.list_vms()?;
        assert_eq!(vms.len(), 1);
        
        // Delete
        storage.delete_vm("test-vm")?;
        let deleted = storage.get_vm("test-vm")?;
        assert!(deleted.is_none());
        
        Ok(())
    }
}