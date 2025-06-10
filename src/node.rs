use anyhow::{Result, Context};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, debug, error};

use crate::raft_node_v2::RaftNode;
use crate::runtime_traits::{RealRuntime, Runtime, Clock};
use crate::storage::Storage;
use crate::tailscale::TailscaleDiscovery;
use crate::types::*;
use crate::microvm::MicroVm;
use std::collections::HashMap;
use std::net::SocketAddr;

pub struct Node {
    config: NodeConfig,
    storage: Arc<Storage>,
    raft_proposal_tx: Option<mpsc::Sender<crate::state_machine::StateMachineCommand>>,
    initial_peers: HashMap<u64, SocketAddr>,
}

impl Node {
    pub async fn new(config: NodeConfig, initial_peers: HashMap<u64, SocketAddr>) -> Result<Self> {
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
            initial_peers,
        })
    }
    
    pub async fn run(mut self) -> Result<()> {
        info!("Starting Blixard node {}", self.config.id);
        println!("Node {} starting Raft consensus engine...", self.config.id);
        
        // Initialize Raft with initial peers
        let peer_ids: Vec<u64> = self.initial_peers.keys().copied().collect();
        println!("Initializing Raft with {} peers: {:?}", peer_ids.len(), peer_ids);
        
        let mut raft_node = RaftNode::new(
            self.config.id,
            self.config.bind_addr,
            self.storage.clone(),
            peer_ids, // Pass peer IDs for initial configuration
            Arc::new(RealRuntime::new()),
        ).await?;
        
        println!("Raft node created successfully");
        
        // Register peer addresses with network layer (no Raft proposals yet)
        for (peer_id, peer_addr) in &self.initial_peers {
            info!("Registering peer {} at {}", peer_id, peer_addr);
            raft_node.register_peer_address(*peer_id, *peer_addr).await;
        }
        
        // Discover additional peers if using Tailscale
        if self.config.use_tailscale {
            if TailscaleDiscovery::is_available().await {
                info!("Using Tailscale for node discovery");
                let nodes = TailscaleDiscovery::discover_nodes().await?;
                for node in nodes {
                    if let Some(id) = node.node_id {
                        if id != self.config.id && !self.initial_peers.contains_key(&id) {
                            // Parse tailscale IP to SocketAddr (assuming default port)
                            if let Ok(addr) = format!("{}:7000", node.tailscale_ip).parse::<SocketAddr>() {
                                info!("Discovered peer node {} at {}", id, addr);
                                raft_node.add_peer(id, addr).await?;
                            }
                        }
                    }
                }
            } else {
                error!("Tailscale requested but not available");
            }
        }
        
        // Get proposal handle
        self.raft_proposal_tx = Some(raft_node.get_proposal_handle());
        
        // Start background tasks  
        let runtime = Arc::new(RealRuntime::new());
        let monitor_handle = runtime.spawn(Box::pin(self.clone().monitor_vms()));
        let raft_handle = runtime.spawn(Box::pin(raft_node.run()));
        let grpc_handle = runtime.spawn(Box::pin(self.clone().run_grpc_server()));
        
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
                                let command = crate::state_machine::StateMachineCommand::UpdateVmStatus {
                                    name: vm.name.clone(),
                                    status: actual_status,
                                };
                                let _ = tx.send(command).await;
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
            let vm = VmState {
                name: config.name.clone(),
                config,
                status: VmStatus::Creating,
                node_id,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };
            let command = crate::state_machine::StateMachineCommand::CreateVm { vm };
            tx.send(command).await?;
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
                let command = crate::state_machine::StateMachineCommand::UpdateVmStatus {
                    name,
                    status: VmStatus::Running,
                };
                tx.send(command).await?;
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
                let command = crate::state_machine::StateMachineCommand::UpdateVmStatus {
                    name,
                    status: VmStatus::Stopped,
                };
                tx.send(command).await?;
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
            initial_peers: self.initial_peers.clone(),
        }
    }
}