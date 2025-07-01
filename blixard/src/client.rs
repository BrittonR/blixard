//! Iroh client for CLI commands
//!
//! This module provides the Iroh client interface for all CLI operations.

use blixard_core::{
    error::{BlixardError, BlixardResult},
    iroh_types::{
        CreateVmRequest, CreateVmResponse,
        StartVmRequest, StartVmResponse,
        StopVmRequest, StopVmResponse,
        DeleteVmRequest, DeleteVmResponse,
        GetVmStatusRequest, GetVmStatusResponse,
        ListVmsRequest, ListVmsResponse,
        ClusterStatusRequest, ClusterStatusResponse,
        JoinRequest, JoinResponse,
        LeaveRequest, LeaveResponse,
        CreateVmWithSchedulingRequest, CreateVmWithSchedulingResponse,
        ClusterResourceSummaryRequest, ClusterResourceSummaryResponse,
        ScheduleVmPlacementRequest, ScheduleVmPlacementResponse,
        MigrateVmRequest, MigrateVmResponse,
    },
    transport::{
        iroh_client::IrohClusterServiceClient,
    },
};
use crate::node_discovery::NodeDiscovery;
use std::sync::Arc;

/// Iroh client for all operations
pub struct IrohClient {
    client: IrohClusterServiceClient,
}

impl IrohClient {
    /// Create a new Iroh client
    pub async fn new(addr: &str) -> BlixardResult<Self> {
        // Use node discovery to get Iroh connection info
        let mut discovery = NodeDiscovery::new();
        let node_info = discovery.discover_node(addr).await?;
        
        // Create Iroh NodeAddr
        let node_addr = discovery.create_node_addr(&node_info)?;
        
        // Create Iroh endpoint for client
        let endpoint = iroh::Endpoint::builder()
            .discovery(Box::new(iroh::dns::DnsDiscovery::n0_dns()))
            .bind(0)
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to create Iroh endpoint: {}", e),
            })?;
        
        // Create Iroh cluster service client
        let client = IrohClusterServiceClient::new(Arc::new(endpoint), node_addr);
        
        Ok(Self { client })
    }

    // VM operations

    pub async fn create_vm(&mut self, request: CreateVmRequest) -> BlixardResult<CreateVmResponse> {
        let (success, message, vm_id) = self.client.create_vm(
            request.name.clone(),
            request.config_path,
            request.vcpus,
            request.memory_mb,
        ).await?;
        
        Ok(CreateVmResponse {
            success,
            message,
            vm_id,
        })
    }

    pub async fn start_vm(&mut self, request: StartVmRequest) -> BlixardResult<StartVmResponse> {
        let (success, message) = self.client.start_vm(request.name).await?;
        Ok(StartVmResponse { success, message })
    }

    pub async fn stop_vm(&mut self, request: StopVmRequest) -> BlixardResult<StopVmResponse> {
        let (success, message) = self.client.stop_vm(request.name).await?;
        Ok(StopVmResponse { success, message })
    }

    pub async fn delete_vm(&mut self, request: DeleteVmRequest) -> BlixardResult<DeleteVmResponse> {
        let (success, message) = self.client.delete_vm(request.name).await?;
        Ok(DeleteVmResponse { success, message })
    }

    pub async fn get_vm_status(&mut self, request: GetVmStatusRequest) -> BlixardResult<GetVmStatusResponse> {
        let vm_info = self.client.get_vm_status(request.name).await?;
        Ok(GetVmStatusResponse {
            found: vm_info.is_some(),
            vm_info: vm_info.map(|info| blixard_core::iroh_types::VmInfo {
                name: info.name,
                state: info.state as i32,
                node_id: info.node_id,
                vcpus: info.vcpus,
                memory_mb: info.memory_mb,
                ip_address: info.ip_address.unwrap_or_default(),
            }),
        })
    }

    pub async fn list_vms(&mut self, _request: ListVmsRequest) -> BlixardResult<ListVmsResponse> {
        let vms = self.client.list_vms().await?;
        let vms = vms.into_iter().map(|info| blixard_core::iroh_types::VmInfo {
            name: info.name,
            state: info.state as i32,
            node_id: info.node_id,
            vcpus: info.vcpus,
            memory_mb: info.memory_mb,
            ip_address: info.ip_address.unwrap_or_default(),
        }).collect();
        Ok(ListVmsResponse { vms })
    }

    // Cluster operations

    pub async fn get_cluster_status(&mut self, _request: ClusterStatusRequest) -> BlixardResult<ClusterStatusResponse> {
        let (leader_id, nodes, term) = self.client.get_cluster_status().await?;
        
        // Convert to response format
        let nodes = nodes.into_iter().map(|n| blixard_core::iroh_types::NodeInfo {
            id: n.id,
            address: n.address,
            state: match n.state {
                blixard_core::types::NodeState::Unknown => 0,
                blixard_core::types::NodeState::Follower => 1,
                blixard_core::types::NodeState::Candidate => 2,
                blixard_core::types::NodeState::Leader => 3,
            },
            p2p_node_id: n.p2p_node_id,
            p2p_addresses: n.p2p_addresses,
            p2p_relay_url: n.p2p_relay_url,
        }).collect();
        
        Ok(ClusterStatusResponse {
            leader_id,
            nodes,
            term,
        })
    }

    pub async fn join_cluster(&mut self, request: JoinRequest) -> BlixardResult<JoinResponse> {
        let (success, message, peers, voters) = self.client
            .join_cluster(request.node_id, request.bind_address)
            .await?;
        
        // Convert to response format
        let peers = peers.into_iter().map(|p| blixard_core::iroh_types::NodeInfo {
            id: p.id,
            address: p.address,
            state: match p.state {
                blixard_core::types::NodeState::Unknown => 0,
                blixard_core::types::NodeState::Follower => 1,
                blixard_core::types::NodeState::Candidate => 2,
                blixard_core::types::NodeState::Leader => 3,
            },
            p2p_node_id: p.p2p_node_id,
            p2p_addresses: p.p2p_addresses,
            p2p_relay_url: p.p2p_relay_url,
        }).collect();
        
        Ok(JoinResponse {
            success,
            message,
            peers,
            voters,
        })
    }

    pub async fn leave_cluster(&mut self, request: LeaveRequest) -> BlixardResult<LeaveResponse> {
        let (success, message) = self.client.leave_cluster(request.node_id).await?;
        Ok(LeaveResponse { success, message })
    }

    // Advanced VM operations (not implemented in Iroh yet)

    pub async fn create_vm_with_scheduling(&mut self, _request: CreateVmWithSchedulingRequest) -> BlixardResult<CreateVmWithSchedulingResponse> {
        Err(BlixardError::NotImplemented {
            feature: "create_vm_with_scheduling".to_string(),
        })
    }

    pub async fn get_cluster_resource_summary(&mut self, _request: ClusterResourceSummaryRequest) -> BlixardResult<ClusterResourceSummaryResponse> {
        Err(BlixardError::NotImplemented {
            feature: "get_cluster_resource_summary".to_string(),
        })
    }

    pub async fn schedule_vm_placement(&mut self, _request: ScheduleVmPlacementRequest) -> BlixardResult<ScheduleVmPlacementResponse> {
        Err(BlixardError::NotImplemented {
            feature: "schedule_vm_placement".to_string(),
        })
    }

    pub async fn migrate_vm(&mut self, _request: MigrateVmRequest) -> BlixardResult<MigrateVmResponse> {
        Err(BlixardError::NotImplemented {
            feature: "migrate_vm".to_string(),
        })
    }
    
    // VM Health Monitoring operations
    
    pub async fn get_vm_health_status(&mut self, vm_name: String) -> BlixardResult<blixard_core::iroh_types::GetVmHealthStatusResponse> {
        self.client.get_vm_health_status(vm_name).await
    }
    
    pub async fn add_vm_health_check(&mut self, vm_name: String, health_check: blixard_core::vm_health_types::HealthCheck) -> BlixardResult<blixard_core::iroh_types::AddVmHealthCheckResponse> {
        self.client.add_vm_health_check(vm_name, health_check).await
    }
    
    pub async fn list_vm_health_checks(&mut self, vm_name: String) -> BlixardResult<blixard_core::iroh_types::ListVmHealthChecksResponse> {
        self.client.list_vm_health_checks(vm_name).await
    }
    
    pub async fn remove_vm_health_check(&mut self, vm_name: String, check_name: String) -> BlixardResult<blixard_core::iroh_types::RemoveVmHealthCheckResponse> {
        self.client.remove_vm_health_check(vm_name, check_name).await
    }
    
    pub async fn toggle_vm_health_monitoring(&mut self, vm_name: String, enable: bool) -> BlixardResult<blixard_core::iroh_types::ToggleVmHealthMonitoringResponse> {
        self.client.toggle_vm_health_monitoring(vm_name, enable).await
    }
    
    pub async fn configure_vm_recovery_policy(&mut self, vm_name: String, policy: blixard_core::vm_auto_recovery::RecoveryPolicy) -> BlixardResult<blixard_core::iroh_types::ConfigureVmRecoveryPolicyResponse> {
        self.client.configure_vm_recovery_policy(vm_name, policy).await
    }
}

// For backward compatibility, export as UnifiedClient
pub type UnifiedClient = IrohClient;

/// Helper to create a client (no longer needs transport config)
pub fn get_transport_config() -> Option<blixard_core::transport::config::TransportConfig> {
    None
}