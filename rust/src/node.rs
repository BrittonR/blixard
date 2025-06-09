use anyhow::{Result, Context};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, debug, error};

use crate::raft_node::RaftNode;
use crate::storage::Storage;
use crate::tailscale::TailscaleDiscovery;
use crate::types::*;
use crate::microvm::MicroVm;

pub struct Node {
    config: NodeConfig,
    storage: Arc<Storage>,
    raft_proposal_tx: Option<mpsc::Sender<Vec<u8>>>,
}

impl Node {
    pub async fn new(config: NodeConfig) -> Result<Self> {
        info!("Initializing node {} at {}", config.id, config.bind_addr);
        
        // Create storage directory if it doesn't exist
        tokio::fs::create_dir_all(&config.data_dir)
            .await
            .context("Failed to create data directory")?;
        
        // Initialize storage
        let storage_path = format!("{}/blixard.db", config.data_dir);
        let storage = Arc::new(Storage::new(storage_path)?);
        
        Ok(Self {
            config,
            storage,
            raft_proposal_tx: None,
        })
    }
    
    pub async fn run(mut self) -> Result<()> {
        info!("Starting Blixard node {}", self.config.id);
        
        // Discover peers if using Tailscale
        let mut peers = Vec::new();
        if self.config.use_tailscale {
            if TailscaleDiscovery::is_available().await {
                info!("Using Tailscale for node discovery");
                let nodes = TailscaleDiscovery::discover_nodes().await?;
                for node in nodes {
                    if let Some(id) = node.node_id {
                        if id != self.config.id {
                            peers.push(id);
                            info!("Discovered peer node {} at {}", id, node.tailscale_ip);
                        }
                    }
                }
            } else {
                error!("Tailscale requested but not available");
            }
        }
        
        // Initialize Raft
        let raft_node = RaftNode::new(
            self.config.id,
            self.storage.clone(),
            peers,
        )?;
        
        // Get proposal handle
        self.raft_proposal_tx = Some(raft_node.get_proposal_handle());
        
        // Start background tasks
        let monitor_handle = tokio::spawn(self.clone().monitor_vms());
        let raft_handle = tokio::spawn(raft_node.run());
        let grpc_handle = tokio::spawn(self.clone().run_grpc_server());
        
        // Wait for any task to complete (they shouldn't unless there's an error)
        tokio::select! {
            result = monitor_handle => {
                error!("VM monitor task ended: {:?}", result);
            }
            result = raft_handle => {
                error!("Raft task ended: {:?}", result);
            }
            result = grpc_handle => {
                error!("gRPC server task ended: {:?}", result);
            }
        }
        
        Ok(())
    }
    
    /// Monitor local VMs and update their status
    async fn monitor_vms(self) -> Result<()> {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
        
        loop {
            interval.tick().await;
            
            // Get all VMs assigned to this node
            let vms = self.storage.list_vms()?;
            let local_vms: Vec<_> = vms.into_iter()
                .filter(|vm| vm.node_id == self.config.id)
                .collect();
            
            for vm in local_vms {
                // Check actual status from microvm.nix
                match MicroVm::get_status(&vm.name).await {
                    Ok(actual_status) => {
                        if actual_status != vm.status {
                            debug!("VM {} status changed: {:?} -> {:?}", 
                                vm.name, vm.status, actual_status);
                            
                            // Propose status update through Raft
                            if let Some(ref tx) = self.raft_proposal_tx {
                                let command = VmCommand::UpdateStatus {
                                    name: vm.name.clone(),
                                    status: actual_status,
                                };
                                let data = bincode::serialize(&command)?;
                                let _ = tx.send(data).await;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to get status for VM {}: {}", vm.name, e);
                    }
                }
            }
        }
    }
    
    /// Run the gRPC server for client connections
    async fn run_grpc_server(self) -> Result<()> {
        info!("Starting gRPC server on {}", self.config.bind_addr);
        
        // TODO: Implement actual gRPC server
        // For now, just keep the task alive
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        }
    }
    
    /// Handle VM creation
    pub async fn create_vm(&self, config: VmConfig) -> Result<()> {
        // Choose a node for the VM (for now, use this node)
        let node_id = self.config.id;
        
        // Propose through Raft
        if let Some(ref tx) = self.raft_proposal_tx {
            let command = VmCommand::Create { config, node_id };
            let data = bincode::serialize(&command)?;
            tx.send(data).await?;
        }
        
        Ok(())
    }
    
    /// Handle VM start
    pub async fn start_vm(&self, name: String) -> Result<()> {
        // Check if VM exists and is assigned to this node
        if let Some(vm) = self.storage.get_vm(&name)? {
            if vm.node_id == self.config.id {
                // Start the VM locally
                MicroVm::start(&name, &vm.config.config_path).await?;
            }
            
            // Update status through Raft
            if let Some(ref tx) = self.raft_proposal_tx {
                let command = VmCommand::UpdateStatus {
                    name,
                    status: VmStatus::Running,
                };
                let data = bincode::serialize(&command)?;
                tx.send(data).await?;
            }
        }
        
        Ok(())
    }
    
    /// Handle VM stop
    pub async fn stop_vm(&self, name: String) -> Result<()> {
        // Check if VM exists and is assigned to this node
        if let Some(vm) = self.storage.get_vm(&name)? {
            if vm.node_id == self.config.id {
                // Stop the VM locally
                MicroVm::stop(&name).await?;
            }
            
            // Update status through Raft
            if let Some(ref tx) = self.raft_proposal_tx {
                let command = VmCommand::UpdateStatus {
                    name,
                    status: VmStatus::Stopped,
                };
                let data = bincode::serialize(&command)?;
                tx.send(data).await?;
            }
        }
        
        Ok(())
    }
}

impl Clone for Node {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            storage: self.storage.clone(),
            raft_proposal_tx: self.raft_proposal_tx.clone(),
        }
    }
}