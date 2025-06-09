use anyhow::Result;
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, error};

use crate::types::*;

// Define tables
const VM_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("vms");
const NODE_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("nodes");
const CONFIG_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("config");

pub struct Storage {
    db: Database,
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
        }
        write_txn.commit()?;
        
        Ok(Self { db })
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