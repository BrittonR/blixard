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
        
        // Create Iroh endpoint for client with BLIXARD_ALPN protocol
        let endpoint = iroh::Endpoint::builder()
            .discovery_n0()
            .alpns(vec![blixard_core::transport::BLIXARD_ALPN.to_vec()])
            .bind()
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
        let vm_config = blixard_core::iroh_types::VmConfig {
            name: request.name.clone(),
            cpu_cores: request.vcpus,
            memory_mb: request.memory_mb,
            disk_gb: 10, // Default disk size
            owner: String::new(),
            metadata: std::collections::HashMap::new(),
        };
        
        tracing::debug!("Creating VM with config: {:?}", vm_config);
        
        let response = self.client.create_vm(vm_config).await
            .map_err(|e| {
                tracing::error!("Failed to create VM: {:?}", e);
                e
            })?;
        Ok(response.into_inner())
    }

    pub async fn start_vm(&mut self, request: StartVmRequest) -> BlixardResult<StartVmResponse> {
        let response = self.client.start_vm(request).await?;
        Ok(response.into_inner())
    }

    pub async fn stop_vm(&mut self, request: StopVmRequest) -> BlixardResult<StopVmResponse> {
        let response = self.client.stop_vm(request).await?;
        Ok(response.into_inner())
    }

    pub async fn delete_vm(&mut self, request: DeleteVmRequest) -> BlixardResult<DeleteVmResponse> {
        let response = self.client.delete_vm(request).await?;
        Ok(response.into_inner())
    }

    pub async fn get_vm_status(&mut self, request: GetVmStatusRequest) -> BlixardResult<GetVmStatusResponse> {
        let vm_info = self.client.get_vm_status(request.name).await?;
        Ok(GetVmStatusResponse {
            found: vm_info.is_some(),
            vm_info,
        })
    }

    pub async fn list_vms(&mut self, _request: ListVmsRequest) -> BlixardResult<ListVmsResponse> {
        let vms = self.client.list_vms().await?;
        Ok(ListVmsResponse { vms })
    }

    // Cluster operations

    pub async fn get_cluster_status(&mut self, _request: ClusterStatusRequest) -> BlixardResult<ClusterStatusResponse> {
        let (leader_id, nodes, term) = self.client.get_cluster_status().await?;
        Ok(ClusterStatusResponse {
            leader_id,
            nodes,
            term,
        })
    }

    pub async fn join_cluster(&mut self, request: JoinRequest) -> BlixardResult<JoinResponse> {
        // Extract P2P info if available
        let (p2p_node_id, p2p_addresses, p2p_relay_url) = if let Some(node_addr) = request.p2p_node_addr {
            let node_id_str = node_addr.node_id.to_string();
            let addresses: Vec<String> = node_addr.direct_addresses()
                .map(|addr| addr.to_string())
                .collect();
            let relay_url = node_addr.relay_url.map(|url| url.to_string());
            (Some(node_id_str), addresses, relay_url)
        } else {
            (None, Vec::new(), None)
        };
        
        let (success, message, peers, voters) = self.client
            .join_cluster(request.node_id, request.bind_address, p2p_node_id, p2p_addresses, p2p_relay_url)
            .await?;
        
        // Peers are already in the right format
        
        Ok(JoinResponse {
            success,
            message,
            peers,
            voters,
        })
    }

    pub async fn leave_cluster(&mut self, request: LeaveRequest) -> BlixardResult<LeaveResponse> {
        let response = self.client.leave_cluster(request).await?;
        Ok(response.into_inner())
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