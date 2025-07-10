//! IP Allocation Service - Handles asynchronous IP allocation for VMs
//!
//! This service monitors for VMs that need IP allocation and processes
//! them through the Raft consensus system.

use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::{
    error::{BlixardError, BlixardResult},
    ip_pool::{IpAllocationRequest, IpPoolSelectionStrategy},
    raft_manager::{ProposalData, RaftProposal},
    raft_storage::{IP_ALLOCATION_TABLE, VM_STATE_TABLE},
    types::{VmId, VmState},
};
use redb::{Database, ReadableTable};

/// Service that handles IP allocation for VMs
pub struct IpAllocationService {
    database: Arc<Database>,
    proposal_tx: mpsc::UnboundedSender<RaftProposal>,
    node_id: u64,
}

impl IpAllocationService {
    pub fn new(
        database: Arc<Database>,
        proposal_tx: mpsc::UnboundedSender<RaftProposal>,
        node_id: u64,
    ) -> Self {
        Self {
            database,
            proposal_tx,
            node_id,
        }
    }

    /// Start the IP allocation service
    pub async fn run(self) {
        info!("Starting IP allocation service");
        
        let mut check_interval = interval(Duration::from_secs(5));
        
        loop {
            check_interval.tick().await;
            
            if let Err(e) = self.process_pending_allocations().await {
                error!("Error processing pending IP allocations: {}", e);
            }
        }
    }

    /// Process VMs that need IP allocation
    async fn process_pending_allocations(&self) -> BlixardResult<()> {
        let pending_vms = self.find_pending_allocations()?;
        
        for (vm_name, node_id) in pending_vms {
            debug!("Processing IP allocation for VM {} on node {}", vm_name, node_id);
            
            // Only process if this node is responsible
            if node_id != self.node_id {
                continue;
            }
            
            // Get VM configuration
            let vm_state = self.get_vm_state(&vm_name)?;
            let vm_state = if let Some(vm_state) = vm_state {
                vm_state
            } else {
                warn!("VM {} not found, skipping IP allocation", vm_name);
                continue;
            };
            
            let vm_id = VmId::from_string(&vm_name);
            
            // Create IP allocation request if VM has IP address configured
            if let Some(ip_address) = &vm_state.config.ip_address {
                let mac_address = format!(
                    "02:00:00:00:{:02x}:01",
                    (self.node_id & 0xFF) as u8
                );
                
                let allocation_request = IpAllocationRequest {
                    vm_id: vm_id.clone(),
                    preferred_pool: None, // Could be enhanced to support pool preferences
                    required_tags: Default::default(),
                    topology_hint: Some(format!("node-{}", self.node_id)),
                    selection_strategy: IpPoolSelectionStrategy::RoundRobin,
                    mac_address,
                };
                
                // Submit allocation request through Raft
                let proposal = RaftProposal {
                    id: Uuid::new_v4().as_bytes().to_vec(),
                    data: ProposalData::AllocateIp {
                        request: allocation_request,
                    },
                    response_tx: None,
                };
                
                if let Err(e) = self.proposal_tx.send(proposal) {
                    error!("Failed to submit IP allocation proposal: {}", e);
                    continue;
                }
                
                info!(
                    "Submitted IP allocation request for VM {}",
                    vm_name
                );
            }
            
            // Update allocation status to prevent reprocessing
            self.update_allocation_status(&vm_name, "processing")?;
        }
        
        Ok(())
    }

    /// Find VMs with pending IP allocations
    fn find_pending_allocations(&self) -> BlixardResult<Vec<(String, u64)>> {
        let read_txn = self.database.begin_read()?;
        let allocation_table = read_txn.open_table(IP_ALLOCATION_TABLE)?;
        
        let mut pending = Vec::new();
        
        for entry in allocation_table.iter()? {
            let (key, value) = entry?;
            let vm_name = key.value().to_string();
            
            // Check if this is a pending allocation
            if let Ok((status, node_id)) = bincode::deserialize::<(String, u64)>(value.value()) {
                if status == "pending" {
                    pending.push((vm_name, node_id));
                }
            }
        }
        
        Ok(pending)
    }

    /// Get VM state from database
    fn get_vm_state(&self, vm_name: &str) -> BlixardResult<Option<VmState>> {
        let read_txn = self.database.begin_read()?;
        let vm_table = read_txn.open_table(VM_STATE_TABLE)?;
        
        if let Some(data) = vm_table.get(vm_name)? {
            let vm_state = bincode::deserialize::<VmState>(data.value())
                .map_err(|e| BlixardError::Serialization {
                    operation: "deserialize vm state".to_string(),
                    source: Box::new(e),
                })?;
            Ok(Some(vm_state))
        } else {
            Ok(None)
        }
    }

    /// Update allocation status in database
    fn update_allocation_status(&self, vm_name: &str, status: &str) -> BlixardResult<()> {
        let write_txn = self.database.begin_write()?;
        
        // Get current node_id and update allocation status
        {
            let mut allocation_table = write_txn.open_table(IP_ALLOCATION_TABLE)?;
            
            // Get current node_id
            let node_id = if let Some(data) = allocation_table.get(vm_name)? {
                if let Ok((_, node_id)) = bincode::deserialize::<(String, u64)>(data.value()) {
                    node_id
                } else {
                    self.node_id
                }
            } else {
                self.node_id
            };
            
            let status_data = bincode::serialize(&(status.to_string(), node_id))
                .map_err(|e| BlixardError::Serialization {
                    operation: "serialize allocation status".to_string(),
                    source: Box::new(e),
                })?;
            
            allocation_table.insert(vm_name, status_data.as_slice())?;
        }
        
        write_txn.commit()?;
        
        Ok(())
    }
}

/// Handle successful IP allocation by updating VM configuration
pub async fn handle_ip_allocation_result(
    database: Arc<Database>,
    vm_id: &VmId,
    allocated_ip: std::net::IpAddr,
    network_idx: usize,
) -> BlixardResult<()> {
    let write_txn = database.begin_write()?;
    
    // Update VM IP mapping
    {
        let mut vm_ip_table = write_txn.open_table(crate::raft_storage::VM_IP_MAPPING_TABLE)?;
        
        // Get existing IPs for this VM
        let mut vm_ips = if let Some(data) = vm_ip_table.get(vm_id.to_string().as_str())? {
            bincode::deserialize::<Vec<std::net::IpAddr>>(data.value())
                .unwrap_or_else(|_| Vec::new())
        } else {
            Vec::new()
        };
        
        // Add new IP if not already present
        if !vm_ips.contains(&allocated_ip) {
            vm_ips.push(allocated_ip);
        }
        
        let ip_data = bincode::serialize(&vm_ips)
            .map_err(|e| BlixardError::Serialization {
                operation: "serialize VM IPs".to_string(),
                source: Box::new(e),
            })?;
        
        vm_ip_table.insert(vm_id.to_string().as_str(), ip_data.as_slice())?;
    }
    
    // Update allocation status
    {
        let mut allocation_table = write_txn.open_table(IP_ALLOCATION_TABLE)?;
        allocation_table.remove(vm_id.to_string().as_str())?;
    }
    
    write_txn.commit()?;
    
    info!(
        "Updated VM {} with allocated IP {} for network {}",
        vm_id, allocated_ip, network_idx
    );
    
    Ok(())
}