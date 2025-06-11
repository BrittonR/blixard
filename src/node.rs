use tokio::net::TcpListener;
use tokio::sync::{oneshot, mpsc, RwLock};
use tokio::task::JoinHandle;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{BlixardError, BlixardResult};
use crate::types::{NodeConfig, VmState, VmCommand};

use raft::{Config, RawNode, GetEntriesContext};
use redb::{Database, TableDefinition, ReadableTable};
use slog::{o, Drain};

const VM_STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("vm_states");
const CLUSTER_STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("cluster_state");

// Raft storage tables
const RAFT_LOG_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("raft_log");
const RAFT_HARD_STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("raft_hard_state");
const RAFT_CONF_STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("raft_conf_state");

#[derive(Clone)]
pub struct RedbRaftStorage {
    database: Arc<Database>,
}

/// A Blixard cluster node
pub struct Node {
    config: NodeConfig,
    shutdown_tx: Option<oneshot::Sender<()>>,
    handle: Option<JoinHandle<BlixardResult<()>>>,
    raft_node: Option<RawNode<RedbRaftStorage>>,
    vm_states: Arc<RwLock<HashMap<String, VmState>>>,
    database: Option<Arc<Database>>,
    vm_command_tx: Option<mpsc::UnboundedSender<VmCommand>>,
}

impl Node {
    /// Create a new node
    pub fn new(config: NodeConfig) -> Self {
        Self {
            config,
            shutdown_tx: None,
            handle: None,
            raft_node: None,
            vm_states: Arc::new(RwLock::new(HashMap::new())),
            database: None,
            vm_command_tx: None,
        }
    }

    /// Initialize the node with database and channels
    pub async fn initialize(&mut self) -> BlixardResult<()> {
        // Initialize database
        let database = Database::create(&format!("{}/blixard.db", self.config.data_dir))
            .map_err(|e| BlixardError::Storage {
                operation: "create database".to_string(),
                source: Box::new(e),
            })?;
        self.database = Some(Arc::new(database));

        // Create VM command channel
        let (vm_command_tx, vm_command_rx) = mpsc::unbounded_channel();
        self.vm_command_tx = Some(vm_command_tx);

        // Start VM command processor
        self.start_vm_processor(vm_command_rx).await?;

        // Initialize Raft
        self.init_raft().await?;

        Ok(())
    }

    async fn init_raft(&mut self) -> BlixardResult<()> {
        let raft_config = Config {
            id: self.config.id,
            election_tick: 10,
            heartbeat_tick: 3,
            ..Default::default()
        };

        let storage = RedbRaftStorage {
            database: self.database.as_ref().unwrap().clone(),
        };
        
        // Create a simple logger for Raft
        let drain = slog::Discard;
        let logger = slog::Logger::root(drain, o!());
        
        self.raft_node = Some(RawNode::new(&raft_config, storage, &logger).map_err(|e| {
            BlixardError::Raft {
                operation: "initialize raft node".to_string(),
                source: Box::new(e),
            }
        })?);

        Ok(())
    }

    async fn start_vm_processor(&self, mut vm_command_rx: mpsc::UnboundedReceiver<VmCommand>) -> BlixardResult<()> {
        let vm_states = Arc::clone(&self.vm_states);
        let database = self.database.as_ref().unwrap().clone();

        tokio::spawn(async move {
            while let Some(command) = vm_command_rx.recv().await {
                if let Err(e) = Self::process_vm_command(command, &vm_states, &database).await {
                    tracing::error!("Error processing VM command: {}", e);
                }
            }
        });

        Ok(())
    }

    async fn process_vm_command(
        command: VmCommand,
        vm_states: &Arc<RwLock<HashMap<String, VmState>>>,
        database: &Arc<Database>,
    ) -> BlixardResult<()> {
        match command {
            VmCommand::Create { config: _, node_id: _ } => {
                // TODO: Interface with microvm.nix to create VM
                Err(BlixardError::NotImplemented {
                    feature: "VM creation via microvm.nix".to_string(),
                })
            }
            VmCommand::Start { name: _ } => {
                // TODO: Interface with microvm.nix to start VM
                Err(BlixardError::NotImplemented {
                    feature: "VM start via microvm.nix".to_string(),
                })
            }
            VmCommand::Stop { name: _ } => {
                // TODO: Interface with microvm.nix to stop VM
                Err(BlixardError::NotImplemented {
                    feature: "VM stop via microvm.nix".to_string(),
                })
            }
            VmCommand::Delete { name: _ } => {
                // TODO: Interface with microvm.nix to delete VM
                Err(BlixardError::NotImplemented {
                    feature: "VM deletion via microvm.nix".to_string(),
                })
            }
            VmCommand::UpdateStatus { name, status } => {
                let mut states = vm_states.write().await;
                if let Some(vm_state) = states.get_mut(&name) {
                    vm_state.status = status;
                    vm_state.updated_at = chrono::Utc::now();
                    
                    // Persist to database
                    Self::persist_vm_state(database, vm_state).await?;
                }
                Ok(())
            }
        }
    }

    async fn persist_vm_state(database: &Database, vm_state: &VmState) -> BlixardResult<()> {
        let write_txn = database.begin_write().map_err(|e| BlixardError::Storage {
            operation: "begin write transaction".to_string(),
            source: Box::new(e),
        })?;

        {
            let mut table = write_txn.open_table(VM_STATE_TABLE).map_err(|e| BlixardError::Storage {
                operation: "open vm_states table".to_string(),
                source: Box::new(e),
            })?;

            let serialized = bincode::serialize(vm_state).map_err(|e| BlixardError::Serialization {
                operation: "serialize vm state".to_string(),
                source: Box::new(e),
            })?;

            table.insert(vm_state.name.as_str(), serialized.as_slice()).map_err(|e| BlixardError::Storage {
                operation: "insert vm state".to_string(),
                source: Box::new(e),
            })?;
        }

        write_txn.commit().map_err(|e| BlixardError::Storage {
            operation: "commit transaction".to_string(),
            source: Box::new(e),
        })?;

        Ok(())
    }

    /// Send a VM command for processing
    pub async fn send_vm_command(&self, command: VmCommand) -> BlixardResult<()> {
        if let Some(tx) = &self.vm_command_tx {
            tx.send(command).map_err(|_| BlixardError::Internal {
                message: "Failed to send VM command".to_string(),
            })?;
        }
        Ok(())
    }

    /// Join a cluster
    pub async fn join_cluster(&mut self, _peer_addr: Option<std::net::SocketAddr>) -> BlixardResult<()> {
        // TODO: Implement cluster join logic via Raft
        Err(BlixardError::NotImplemented {
            feature: "cluster join".to_string(),
        })
    }

    /// Leave the cluster
    pub async fn leave_cluster(&mut self) -> BlixardResult<()> {
        // TODO: Implement cluster leave logic
        Err(BlixardError::NotImplemented {
            feature: "cluster leave".to_string(),
        })
    }

    /// Get cluster status
    pub async fn get_cluster_status(&self) -> BlixardResult<(u64, Vec<u64>, u64)> {
        // TODO: Return (leader_id, node_ids, term)
        Err(BlixardError::NotImplemented {
            feature: "cluster status".to_string(),
        })
    }

    /// Start the node
    pub async fn start(&mut self) -> BlixardResult<()> {
        let config = self.config.clone();
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        let handle = tokio::spawn(async move {
            let listener = TcpListener::bind(&config.bind_addr).await?;
            tracing::info!("Node {} listening on {}", config.id, config.bind_addr);

            loop {
                tokio::select! {
                    result = listener.accept() => {
                        match result {
                            Ok((_stream, addr)) => {
                                tracing::debug!("New connection from {}", addr);
                                // Handle connection
                            }
                            Err(e) => {
                                tracing::error!("Accept error: {}", e);
                            }
                        }
                    }
                    _ = &mut shutdown_rx => {
                        tracing::info!("Node {} shutting down", config.id);
                        break;
                    }
                }
            }

            Ok(())
        });

        self.handle = Some(handle);
        Ok(())
    }

    /// Stop the node
    pub async fn stop(&mut self) -> BlixardResult<()> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        
        if let Some(handle) = self.handle.take() {
            match handle.await {
                Ok(result) => result,
                Err(_) => Ok(()), // Task was cancelled
            }
        } else {
            Ok(())
        }
    }

    /// Check if the node is running
    pub fn is_running(&self) -> bool {
        self.handle.as_ref().map_or(false, |h| !h.is_finished())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_node_lifecycle() {
        let config = NodeConfig {
            id: 1,
            data_dir: "/tmp/test".to_string(),
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            join_addr: None,
            use_tailscale: false,
        };

        let mut node = Node::new(config);
        
        // Start node
        node.start().await.unwrap();
        assert!(node.is_running());

        // Stop node
        node.stop().await.unwrap();
        assert!(!node.is_running());
    }
}

impl RedbRaftStorage {
    // Simplified implementation - just return defaults for now
    // TODO: Implement proper persistence later
}

// Implement Raft storage trait with simplified redb backend
impl raft::Storage for RedbRaftStorage {
    fn initial_state(&self) -> raft::Result<raft::RaftState> {
        // For now, just return default state - we'll implement persistence later
        let hard_state = raft::prelude::HardState::default();
        let conf_state = raft::prelude::ConfState::default();
        Ok(raft::RaftState::new(hard_state, conf_state))
    }

    fn entries(&self, _low: u64, _high: u64, _max_size: impl Into<Option<u64>>, _context: GetEntriesContext) -> raft::Result<Vec<raft::prelude::Entry>> {
        // For now, return empty - we'll implement log storage later
        Ok(vec![])
    }

    fn term(&self, _idx: u64) -> raft::Result<u64> {
        // For now, return 0 - we'll implement term lookup later
        Ok(0)
    }

    fn first_index(&self) -> raft::Result<u64> {
        Ok(1)
    }

    fn last_index(&self) -> raft::Result<u64> {
        Ok(0)
    }

    fn snapshot(&self, _request_index: u64, _to: u64) -> raft::Result<raft::prelude::Snapshot> {
        // TODO: Implement snapshot creation from redb state
        Err(raft::Error::Store(raft::StorageError::SnapshotTemporarilyUnavailable))
    }
}