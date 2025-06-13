use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use std::sync::Arc;

use crate::error::{BlixardError, BlixardResult};
use crate::types::{NodeConfig, VmCommand};
use crate::vm_manager::VmManager;
use crate::raft_manager::{RaftManager, RaftProposal, ProposalData};
use crate::node_shared::SharedNodeState;
use crate::peer_connector::PeerConnector;

use redb::Database;

/// A Blixard cluster node
pub struct Node {
    /// Shared state that is Send + Sync
    shared: Arc<SharedNodeState>,
    /// Non-sync runtime handles
    handle: Option<JoinHandle<BlixardResult<()>>>,
    raft_handle: Option<JoinHandle<BlixardResult<()>>>,
}

impl Node {
    /// Create a new node
    pub fn new(config: NodeConfig) -> Self {
        Self {
            shared: Arc::new(SharedNodeState::new(config)),
            handle: None,
            raft_handle: None,
        }
    }
    
    /// Get a shared reference to the node state
    /// This is what should be passed to the gRPC server
    pub fn shared(&self) -> Arc<SharedNodeState> {
        Arc::clone(&self.shared)
    }

    /// Initialize the node with database and channels
    pub async fn initialize(&mut self) -> BlixardResult<()> {
        // Initialize database
        let data_dir = self.shared.config.data_dir.clone();
        let db_path = format!("{}/blixard.db", data_dir);
        
        // Try to open existing database first, create if it doesn't exist
        let database = match std::fs::metadata(&db_path) {
            Ok(_) => {
                // Database exists, open it
                Database::open(&db_path)
                    .map_err(|e| BlixardError::Storage {
                        operation: "open database".to_string(),
                        source: Box::new(e),
                    })?
            }
            Err(_) => {
                // Database doesn't exist, create it
                Database::create(&db_path)
                    .map_err(|e| BlixardError::Storage {
                        operation: "create database".to_string(),
                        source: Box::new(e),
                    })?
            }
        };
        let db_arc = Arc::new(database);
        
        // Initialize all database tables
        crate::storage::init_database_tables(&db_arc)?;
        
        self.shared.set_database(db_arc.clone()).await;

        // Initialize VM manager
        let (vm_manager, vm_command_rx) = VmManager::new(db_arc.clone());
        let command_tx = vm_manager.command_tx.clone();
        
        // Load existing VMs from database
        vm_manager.load_from_database().await?;
        
        vm_manager.start_processor(vm_command_rx);
        self.shared.set_vm_manager(vm_manager, command_tx).await;

        // Initialize Raft manager
        // If join_addr is provided, don't bootstrap as single node
        let peers = if let Some(join_addr) = &self.shared.config.join_addr {
            // We're joining an existing cluster, add a dummy peer to prevent bootstrap
            // The actual peer will be discovered during join
            vec![(1, join_addr.to_string())] // Use node 1 as default leader
        } else {
            vec![] // Start with no peers, bootstrap as single node
        };
        let node_id = self.shared.get_id();
        
        // If we're bootstrapping (no peers AND no join_addr), initialize storage first
        if peers.is_empty() && self.shared.config.join_addr.is_none() {
            let storage = crate::storage::RedbRaftStorage { database: db_arc.clone() };
            storage.initialize_single_node(node_id)?;
        }
        
        let (raft_manager, proposal_tx, message_tx, conf_change_tx, outgoing_rx) = RaftManager::new(
            node_id,
            db_arc.clone(),
            peers,
            Arc::downgrade(&self.shared),
        )?;
        
        self.shared.set_raft_proposal_tx(proposal_tx).await;
        self.shared.set_raft_message_tx(message_tx).await;
        self.shared.set_raft_conf_change_tx(conf_change_tx).await;
        
        // Create peer connector
        let peer_connector = Arc::new(PeerConnector::new(self.shared.clone()));
        
        // Start peer connection maintenance
        peer_connector.clone().start_connection_maintenance();
        
        // Start task to handle outgoing Raft messages
        let peer_connector_clone = peer_connector.clone();
        let _outgoing_handle = tokio::spawn(async move {
            let mut outgoing_rx = outgoing_rx;
            while let Some((to, msg)) = outgoing_rx.recv().await {
                if let Err(e) = peer_connector_clone.send_raft_message(to, msg).await {
                    tracing::warn!("Failed to send Raft message to {}: {}", to, e);
                }
            }
            Ok::<(), BlixardError>(())
        });
        
        // Start Raft manager in background
        let raft_handle = tokio::spawn(async move {
            raft_manager.run().await
        });
        self.raft_handle = Some(raft_handle);
        
        // If we have a join address, send join request after initialization
        if self.shared.config.join_addr.is_some() {
            // Give the Raft manager a moment to start
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            
            if let Err(e) = self.send_join_request().await {
                tracing::error!("Failed to join cluster: {}", e);
                return Err(e);
            }
        }
        
        Ok(())
    }


    /// Send join request to a cluster
    pub async fn send_join_request(&self) -> BlixardResult<()> {
        if let Some(join_addr) = &self.shared.config.join_addr {
            tracing::info!("Sending join request to {}", join_addr);
            // Create gRPC client to join node
            let endpoint = format!("http://{}", join_addr);
            match tonic::transport::Channel::from_shared(endpoint.clone()) {
                Ok(channel) => {
                    match channel.connect().await {
                        Ok(channel) => {
                            let mut client = crate::proto::cluster_service_client::ClusterServiceClient::new(channel);
                            let join_request = crate::proto::JoinRequest {
                                node_id: self.shared.get_id(),
                                bind_address: self.shared.get_bind_addr().to_string(),
                            };
                            
                            match client.join_cluster(join_request).await {
                                Ok(response) => {
                                    let resp = response.into_inner();
                                    if resp.success {
                                        tracing::info!("Successfully joined cluster at {}", join_addr);
                                        // Add peers from response
                                        for peer in resp.peers {
                                            if peer.id != self.shared.get_id() {
                                                let _ = self.shared.add_peer(peer.id, peer.address).await;
                                            }
                                        }
                                    } else {
                                        return Err(BlixardError::ClusterJoin {
                                            reason: resp.message,
                                        });
                                    }
                                }
                                Err(e) => {
                                    return Err(BlixardError::ClusterJoin {
                                        reason: format!("Failed to send join request: {}", e),
                                    });
                                }
                            }
                        }
                        Err(e) => {
                            return Err(BlixardError::ClusterJoin {
                                reason: format!("Failed to connect to join address {}: {}", join_addr, e),
                            });
                        }
                    }
                }
                Err(e) => {
                    return Err(BlixardError::ClusterJoin {
                        reason: format!("Invalid join address {}: {}", join_addr, e),
                    });
                }
            }
        }
        Ok(())
    }

    /// Send a VM command for processing
    pub async fn send_vm_command(&self, command: VmCommand) -> BlixardResult<()> {
        self.shared.send_vm_command(command).await
    }

    /// Get the node ID
    pub fn get_id(&self) -> u64 {
        self.shared.get_id()
    }

    /// Get the bind address
    pub fn get_bind_addr(&self) -> &std::net::SocketAddr {
        self.shared.get_bind_addr()
    }

    /// List all VMs and their status
    pub async fn list_vms(&self) -> BlixardResult<Vec<(crate::types::VmConfig, crate::types::VmStatus)>> {
        self.shared.list_vms().await
    }

    /// Get status of a specific VM
    pub async fn get_vm_status(&self, name: &str) -> BlixardResult<Option<(crate::types::VmConfig, crate::types::VmStatus)>> {
        self.shared.get_vm_status(name).await
    }

    /// Join a cluster
    pub async fn join_cluster(&mut self, peer_addr: Option<std::net::SocketAddr>) -> BlixardResult<()> {
        let node_id = self.shared.get_id();
        let address = self.shared.get_bind_addr().to_string();
        let capabilities = crate::raft_manager::WorkerCapabilities {
            cpu_cores: num_cpus::get() as u32,
            memory_mb: 8192, // TODO: Get actual memory
            disk_gb: 100, // TODO: Get actual disk space
            features: vec!["microvm".to_string()],
        };
        
        if peer_addr.is_none() {
            // Bootstrap mode - directly register in database and bootstrap as leader
            if let Some(db) = self.shared.get_database().await {
                let write_txn = db.begin_write()?;
                {
                    let mut worker_table = write_txn.open_table(crate::storage::WORKER_TABLE)?;
                    let mut status_table = write_txn.open_table(crate::storage::WORKER_STATUS_TABLE)?;
                    
                    let worker_data = bincode::serialize(&(address.clone(), &capabilities))?;
                    worker_table.insert(node_id.to_le_bytes().as_slice(), worker_data.as_slice())?;
                    status_table.insert(node_id.to_le_bytes().as_slice(), [crate::raft_manager::WorkerStatus::Online as u8].as_slice())?;
                }
                write_txn.commit()?;
                
                // Bootstrap Raft as a single-node cluster
                // Note: We need to access the RaftManager to call bootstrap_single_node
                // This is a limitation of the current architecture
                // For now, the test will need to wait for leader election
                
                tracing::info!("Node {} registered as worker in bootstrap mode", node_id);
            }
        } else {
            // Join existing cluster via Raft proposal
            let proposal_data = ProposalData::RegisterWorker {
                node_id,
                address,
                capabilities,
            };
            
            let (response_tx, response_rx) = oneshot::channel();
            let proposal = RaftProposal {
                id: uuid::Uuid::new_v4().as_bytes().to_vec(),
                data: proposal_data,
                response_tx: Some(response_tx),
            };
            
            self.shared.send_raft_proposal(proposal).await?;
            
            // Wait for response
            response_rx.await.map_err(|_| BlixardError::Internal {
                message: "Join proposal response channel closed".to_string(),
            })??;
            
            // TODO: Send join request to the peer via gRPC
        }
        
        Ok(())
    }

    /// Leave the cluster
    pub async fn leave_cluster(&mut self) -> BlixardResult<()> {
        {
            let proposal_data = ProposalData::RemoveWorker {
                node_id: self.shared.get_id(),
            };
            
            let (response_tx, response_rx) = oneshot::channel();
            let proposal = RaftProposal {
                id: uuid::Uuid::new_v4().as_bytes().to_vec(),
                data: proposal_data,
                response_tx: Some(response_tx),
            };
            
            self.shared.send_raft_proposal(proposal).await?;
            
            // Wait for response
            response_rx.await.map_err(|_| BlixardError::Internal {
                message: "Leave proposal response channel closed".to_string(),
            })??;
        }
        
        Ok(())
    }

    /// Get cluster status
    pub async fn get_cluster_status(&self) -> BlixardResult<(u64, Vec<u64>, u64)> {
        self.shared.get_cluster_status().await
    }

    /// Start the node
    pub async fn start(&mut self) -> BlixardResult<()> {
        let node_id = self.shared.get_id();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        self.shared.set_shutdown_tx(shutdown_tx).await;

        // The node doesn't need its own TCP listener since all communication
        // is handled via the gRPC server. We just need a task to manage the
        // node's lifecycle and respond to shutdown signals.
        let handle = tokio::spawn(async move {
            tracing::info!("Node {} started", node_id);

            // Wait for shutdown signal
            let _ = shutdown_rx.await;
            tracing::info!("Node {} shutting down", node_id);

            Ok(())
        });

        self.handle = Some(handle);
        self.shared.set_running(true).await;
        Ok(())
    }

    /// Stop the node
    pub async fn stop(&mut self) -> BlixardResult<()> {
        if let Some(tx) = self.shared.take_shutdown_tx().await {
            let _ = tx.send(());
        }
        
        // Stop main node handle
        if let Some(handle) = self.handle.take() {
            match handle.await {
                Ok(result) => result?,
                Err(_) => {}, // Task was cancelled
            }
        }
        
        // Stop Raft handle
        if let Some(handle) = self.raft_handle.take() {
            handle.abort(); // Abort the Raft task
        }
        
        self.shared.set_running(false).await;
        
        // Clear database to release file lock
        self.shared.clear_database().await;
        
        Ok(())
    }

    /// Check if the node is running
    pub async fn is_running(&self) -> bool {
        self.shared.is_running().await
    }
    
    /// Check if this node is the Raft leader
    pub async fn is_leader(&self) -> bool {
        self.shared.is_leader().await
    }
    
    /// Send a Raft message to the Raft manager
    pub async fn send_raft_message(&self, from: u64, msg: raft::prelude::Message) -> BlixardResult<()> {
        self.shared.send_raft_message(from, msg).await
    }
    
    /// Submit a task to the cluster
    pub async fn submit_task(&self, task_id: &str, task: crate::raft_manager::TaskSpec) -> BlixardResult<u64> {
        self.shared.submit_task(task_id, task).await
    }
    
    /// Get task status
    pub async fn get_task_status(&self, task_id: &str) -> BlixardResult<Option<(String, Option<crate::raft_manager::TaskResult>)>> {
        self.shared.get_task_status(task_id).await
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
        assert!(node.is_running().await);

        // Stop node
        node.stop().await.unwrap();
        assert!(!node.is_running().await);
    }
}

