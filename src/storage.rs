use crate::config::Config;
use crate::error::{BlixardError, Result};
use crate::service::ServiceInfo;
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

// Define tables
const SERVICE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("services");
const RAFT_LOG_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("raft_log");
const RAFT_STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("raft_state");

pub struct Storage {
    db: Arc<Database>,
    config: Config,
}

impl Storage {
    pub async fn new(config: &Config) -> Result<Self> {
        // Ensure the database directory exists
        if let Some(parent) = config.storage.db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let db = Database::create(&config.storage.db_path)?;
        
        // Initialize tables
        let write_txn = db.begin_write()?;
        {
            let _ = write_txn.open_table(SERVICE_TABLE)?;
            let _ = write_txn.open_table(RAFT_LOG_TABLE)?;
            let _ = write_txn.open_table(RAFT_STATE_TABLE)?;
        }
        write_txn.commit()?;
        
        info!("Storage initialized at: {:?}", config.storage.db_path);
        
        Ok(Self {
            db: Arc::new(db),
            config: config.clone(),
        })
    }
    
    pub async fn store_service(&self, service: &ServiceInfo) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SERVICE_TABLE)?;
            let data = bincode::serialize(service)?;
            table.insert(service.name.as_str(), data.as_slice())?;
        }
        write_txn.commit()?;
        
        debug!("Stored service: {}", service.name);
        Ok(())
    }
    
    pub async fn get_service(&self, name: &str) -> Result<Option<ServiceInfo>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SERVICE_TABLE)?;
        
        if let Some(data) = table.get(name)? {
            let service: ServiceInfo = bincode::deserialize(data.value())?;
            Ok(Some(service))
        } else {
            Ok(None)
        }
    }
    
    pub async fn list_services(&self) -> Result<HashMap<String, ServiceInfo>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SERVICE_TABLE)?;
        
        let mut services = HashMap::new();
        for entry in table.iter()? {
            let (key, value) = entry?;
            let service: ServiceInfo = bincode::deserialize(value.value())?;
            services.insert(key.value().to_string(), service);
        }
        
        Ok(services)
    }
    
    pub async fn remove_service(&self, name: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SERVICE_TABLE)?;
            table.remove(name)?;
        }
        write_txn.commit()?;
        
        debug!("Removed service: {}", name);
        Ok(())
    }
    
    // Raft storage methods
    pub async fn append_raft_log(&self, index: u64, entry: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(RAFT_LOG_TABLE)?;
            table.insert(&index, entry)?;
        }
        write_txn.commit()?;
        Ok(())
    }
    
    pub async fn get_raft_log(&self, index: u64) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(RAFT_LOG_TABLE)?;
        
        if let Some(data) = table.get(&index)? {
            Ok(Some(data.value().to_vec()))
        } else {
            Ok(None)
        }
    }
    
    pub async fn get_raft_logs(&self, start: u64, end: u64) -> Result<Vec<(u64, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(RAFT_LOG_TABLE)?;
        
        let mut logs = Vec::new();
        for entry in table.range(start..=end)? {
            let (key, value) = entry?;
            logs.push((key.value(), value.value().to_vec()));
        }
        
        Ok(logs)
    }
    
    pub async fn truncate_raft_log(&self, after_index: u64) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(RAFT_LOG_TABLE)?;
            
            // Get all entries after the specified index
            let to_remove: Vec<u64> = table.range((after_index + 1)..)
                .map(|entry| entry.map(|(k, _)| k.value()))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            
            // Remove them
            for index in to_remove {
                table.remove(&index)?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }
    
    pub async fn save_raft_state(&self, key: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(RAFT_STATE_TABLE)?;
            table.insert(key, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }
    
    pub async fn get_raft_state(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(RAFT_STATE_TABLE)?;
        
        if let Some(data) = table.get(key)? {
            Ok(Some(data.value().to_vec()))
        } else {
            Ok(None)
        }
    }
}