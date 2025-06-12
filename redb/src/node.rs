use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use std::sync::Arc;

use crate::error::{BlixardError, BlixardResult};
use crate::types::{NodeConfig, VmCommand};
use crate::storage::{RedbRaftStorage, init_raft};
use crate::vm_manager::VmManager;

use raft::RawNode;
use redb::Database;

/// A Blixard cluster node
pub struct Node {
    config: NodeConfig,
    shutdown_tx: Option<oneshot::Sender<()>>,
    handle: Option<JoinHandle<BlixardResult<()>>>,
    raft_node: Option<RawNode<RedbRaftStorage>>,
    database: Option<Arc<Database>>,
    vm_manager: Option<VmManager>,
}

impl Node {
    /// Create a new node
    pub fn new(config: NodeConfig) -> Self {
        Self {
            config,
            shutdown_tx: None,
            handle: None,
            raft_node: None,
            database: None,
            vm_manager: None,
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
        let db_arc = Arc::new(database);
        self.database = Some(db_arc.clone());

        // Initialize VM manager
        let (vm_manager, vm_command_rx) = VmManager::new(db_arc.clone());
        vm_manager.start_processor(vm_command_rx);
        self.vm_manager = Some(vm_manager);

        // Initialize Raft
        self.raft_node = Some(init_raft(self.config.id, db_arc)?);
        
        Ok(())
    }


    /// Send a VM command for processing
    pub async fn send_vm_command(&self, command: VmCommand) -> BlixardResult<()> {
        if let Some(vm_manager) = &self.vm_manager {
            vm_manager.send_command(command).await
        } else {
            Err(BlixardError::Internal {
                message: "VM manager not initialized".to_string(),
            })
        }
    }

    /// Get the node ID
    pub fn get_id(&self) -> u64 {
        self.config.id
    }

    /// Get the bind address
    pub fn get_bind_addr(&self) -> &std::net::SocketAddr {
        &self.config.bind_addr
    }

    /// List all VMs and their status
    pub async fn list_vms(&self) -> BlixardResult<Vec<(crate::types::VmConfig, crate::types::VmStatus)>> {
        if let Some(vm_manager) = &self.vm_manager {
            vm_manager.list_vms().await
        } else {
            Ok(Vec::new())
        }
    }

    /// Get status of a specific VM
    pub async fn get_vm_status(&self, name: &str) -> BlixardResult<Option<(crate::types::VmConfig, crate::types::VmStatus)>> {
        if let Some(vm_manager) = &self.vm_manager {
            vm_manager.get_vm_status(name).await
        } else {
            Ok(None)
        }
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

