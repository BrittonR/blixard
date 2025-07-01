//! Transport-aware client for CLI commands
//!
//! This module provides a unified client interface that can use either
//! gRPC or Iroh transport based on configuration.

use blixard_core::{
    error::{BlixardError, BlixardResult},
    proto::{
        cluster_service_client::ClusterServiceClient,
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
        config::TransportConfig,
        iroh_client::{IrohClient, IrohClusterServiceClient},
        iroh_cluster_service::{
            JoinClusterRequest, JoinClusterResponse,
            LeaveClusterRequest, LeaveClusterResponse,
            ClusterStatusRequest as IrohClusterStatusRequest,
            ClusterStatusResponse as IrohClusterStatusResponse,
        },
    },
};
use crate::node_discovery::NodeDiscovery;
use std::sync::Arc;
use std::net::SocketAddr;
use tonic::transport::Channel;

/// Client transport type
#[derive(Debug, Clone)]
pub enum ClientTransport {
    Grpc(ClusterServiceClient<Channel>),
    Iroh(IrohClusterServiceClient),
}

/// Unified client that supports both gRPC and Iroh transports
pub struct UnifiedClient {
    transport: ClientTransport,
}

impl UnifiedClient {
    /// Create a new client based on transport configuration
    pub async fn new(addr: &str, transport_config: Option<&TransportConfig>) -> BlixardResult<Self> {
        // Determine transport type from config or use default
        let use_iroh = match transport_config {
            Some(TransportConfig::Iroh(_)) => true,
            Some(TransportConfig::Dual { .. }) => {
                // For dual mode, prefer Iroh for CLI clients
                true
            }
            Some(TransportConfig::Grpc(_)) => false,
            None => {
                // Default to Iroh as per configuration
                true
            }
        };

        let transport = if use_iroh {
            // Create Iroh client
            Self::create_iroh_client(addr).await?
        } else {
            // Create gRPC client
            Self::create_grpc_client(addr).await?
        };

        Ok(Self { transport })
    }

    /// Create a gRPC client
    async fn create_grpc_client(addr: &str) -> BlixardResult<ClientTransport> {
        let client = ClusterServiceClient::connect(format!("http://{}", addr))
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to connect via gRPC to {}: {}", addr, e),
            })?;
        Ok(ClientTransport::Grpc(client))
    }

    /// Create an Iroh client
    async fn create_iroh_client(addr: &str) -> BlixardResult<ClientTransport> {
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
        
        Ok(ClientTransport::Iroh(client))
    }

    // VM operations

    pub async fn create_vm(&mut self, request: CreateVmRequest) -> BlixardResult<CreateVmResponse> {
        match &mut self.transport {
            ClientTransport::Grpc(client) => {
                let response = client.create_vm(tonic::Request::new(request))
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("gRPC error: {}", e),
                    })?;
                Ok(response.into_inner())
            }
            ClientTransport::Iroh(client) => {
                let (success, message, vm_id) = client.create_vm(
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
        }
    }

    pub async fn start_vm(&mut self, request: StartVmRequest) -> BlixardResult<StartVmResponse> {
        match &mut self.transport {
            ClientTransport::Grpc(client) => {
                let response = client.start_vm(tonic::Request::new(request))
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("gRPC error: {}", e),
                    })?;
                Ok(response.into_inner())
            }
            ClientTransport::Iroh(client) => {
                let (success, message) = client.start_vm(request.name).await?;
                Ok(StartVmResponse { success, message })
            }
        }
    }

    pub async fn stop_vm(&mut self, request: StopVmRequest) -> BlixardResult<StopVmResponse> {
        match &mut self.transport {
            ClientTransport::Grpc(client) => {
                let response = client.stop_vm(tonic::Request::new(request))
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("gRPC error: {}", e),
                    })?;
                Ok(response.into_inner())
            }
            ClientTransport::Iroh(client) => {
                let (success, message) = client.stop_vm(request.name).await?;
                Ok(StopVmResponse { success, message })
            }
        }
    }

    pub async fn delete_vm(&mut self, request: DeleteVmRequest) -> BlixardResult<DeleteVmResponse> {
        match &mut self.transport {
            ClientTransport::Grpc(client) => {
                let response = client.delete_vm(tonic::Request::new(request))
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("gRPC error: {}", e),
                    })?;
                Ok(response.into_inner())
            }
            ClientTransport::Iroh(client) => {
                let (success, message) = client.delete_vm(request.name).await?;
                Ok(DeleteVmResponse { success, message })
            }
        }
    }

    pub async fn get_vm_status(&mut self, request: GetVmStatusRequest) -> BlixardResult<GetVmStatusResponse> {
        match &mut self.transport {
            ClientTransport::Grpc(client) => {
                let response = client.get_vm_status(tonic::Request::new(request))
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("gRPC error: {}", e),
                    })?;
                Ok(response.into_inner())
            }
            ClientTransport::Iroh(client) => {
                let vm_info = client.get_vm_status(request.name).await?;
                Ok(GetVmStatusResponse {
                    found: vm_info.is_some(),
                    vm_info: vm_info.map(|info| blixard_core::proto::VmInfo {
                        name: info.name,
                        state: info.state as i32,
                        node_id: info.node_id,
                        vcpus: info.vcpus,
                        memory_mb: info.memory_mb,
                        ip_address: info.ip_address.unwrap_or_default(),
                    }),
                })
            }
        }
    }

    pub async fn list_vms(&mut self, request: ListVmsRequest) -> BlixardResult<ListVmsResponse> {
        match &mut self.transport {
            ClientTransport::Grpc(client) => {
                let response = client.list_vms(tonic::Request::new(request))
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("gRPC error: {}", e),
                    })?;
                Ok(response.into_inner())
            }
            ClientTransport::Iroh(client) => {
                let vms = client.list_vms().await?;
                let vms = vms.into_iter().map(|info| blixard_core::proto::VmInfo {
                    name: info.name,
                    state: info.state as i32,
                    node_id: info.node_id,
                    vcpus: info.vcpus,
                    memory_mb: info.memory_mb,
                    ip_address: info.ip_address.unwrap_or_default(),
                }).collect();
                Ok(ListVmsResponse { vms })
            }
        }
    }

    // Cluster operations

    pub async fn get_cluster_status(&mut self, request: ClusterStatusRequest) -> BlixardResult<ClusterStatusResponse> {
        match &mut self.transport {
            ClientTransport::Grpc(client) => {
                let response = client.get_cluster_status(tonic::Request::new(request))
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("gRPC error: {}", e),
                    })?;
                Ok(response.into_inner())
            }
            ClientTransport::Iroh(client) => {
                let (leader_id, nodes, term) = client.get_cluster_status().await?;
                
                // Convert to gRPC response format
                let nodes = nodes.into_iter().map(|n| blixard_core::proto::NodeInfo {
                    id: n.id,
                    address: n.address,
                    state: match n.state {
                        blixard_core::types::NodeState::Unknown => blixard_core::proto::NodeState::NodeStateUnknown as i32,
                        blixard_core::types::NodeState::Follower => blixard_core::proto::NodeState::NodeStateFollower as i32,
                        blixard_core::types::NodeState::Candidate => blixard_core::proto::NodeState::NodeStateCandidate as i32,
                        blixard_core::types::NodeState::Leader => blixard_core::proto::NodeState::NodeStateLeader as i32,
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
        }
    }

    pub async fn join_cluster(&mut self, request: JoinRequest) -> BlixardResult<JoinResponse> {
        match &mut self.transport {
            ClientTransport::Grpc(client) => {
                let response = client.join_cluster(tonic::Request::new(request))
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("gRPC error: {}", e),
                    })?;
                Ok(response.into_inner())
            }
            ClientTransport::Iroh(client) => {
                let (success, message, peers, voters) = client
                    .join_cluster(request.node_id, request.bind_address)
                    .await?;
                
                // Convert to gRPC response format
                let peers = peers.into_iter().map(|p| blixard_core::proto::NodeInfo {
                    id: p.id,
                    address: p.address,
                    state: match p.state {
                        blixard_core::types::NodeState::Unknown => blixard_core::proto::NodeState::NodeStateUnknown as i32,
                        blixard_core::types::NodeState::Follower => blixard_core::proto::NodeState::NodeStateFollower as i32,
                        blixard_core::types::NodeState::Candidate => blixard_core::proto::NodeState::NodeStateCandidate as i32,
                        blixard_core::types::NodeState::Leader => blixard_core::proto::NodeState::NodeStateLeader as i32,
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
        }
    }

    pub async fn leave_cluster(&mut self, request: LeaveRequest) -> BlixardResult<LeaveResponse> {
        match &mut self.transport {
            ClientTransport::Grpc(client) => {
                let response = client.leave_cluster(tonic::Request::new(request))
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("gRPC error: {}", e),
                    })?;
                Ok(response.into_inner())
            }
            ClientTransport::Iroh(client) => {
                let (success, message) = client.leave_cluster(request.node_id).await?;
                Ok(LeaveResponse { success, message })
            }
        }
    }

    // Advanced VM operations

    pub async fn create_vm_with_scheduling(&mut self, request: CreateVmWithSchedulingRequest) -> BlixardResult<CreateVmWithSchedulingResponse> {
        match &mut self.transport {
            ClientTransport::Grpc(client) => {
                let response = client.create_vm_with_scheduling(tonic::Request::new(request))
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("gRPC error: {}", e),
                    })?;
                Ok(response.into_inner())
            }
            ClientTransport::Iroh(_client) => {
                // TODO: Implement create_vm_with_scheduling for Iroh
                Err(BlixardError::NotImplemented {
                    feature: "create_vm_with_scheduling for Iroh transport".to_string(),
                })
            }
        }
    }

    pub async fn get_cluster_resource_summary(&mut self, request: ClusterResourceSummaryRequest) -> BlixardResult<ClusterResourceSummaryResponse> {
        match &mut self.transport {
            ClientTransport::Grpc(client) => {
                let response = client.get_cluster_resource_summary(tonic::Request::new(request))
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("gRPC error: {}", e),
                    })?;
                Ok(response.into_inner())
            }
            ClientTransport::Iroh(_client) => {
                // TODO: Implement get_cluster_resource_summary for Iroh
                Err(BlixardError::NotImplemented {
                    feature: "get_cluster_resource_summary for Iroh transport".to_string(),
                })
            }
        }
    }

    pub async fn schedule_vm_placement(&mut self, request: ScheduleVmPlacementRequest) -> BlixardResult<ScheduleVmPlacementResponse> {
        match &mut self.transport {
            ClientTransport::Grpc(client) => {
                let response = client.schedule_vm_placement(tonic::Request::new(request))
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("gRPC error: {}", e),
                    })?;
                Ok(response.into_inner())
            }
            ClientTransport::Iroh(_client) => {
                // TODO: Implement schedule_vm_placement for Iroh
                Err(BlixardError::NotImplemented {
                    feature: "schedule_vm_placement for Iroh transport".to_string(),
                })
            }
        }
    }

    pub async fn migrate_vm(&mut self, request: MigrateVmRequest) -> BlixardResult<MigrateVmResponse> {
        match &mut self.transport {
            ClientTransport::Grpc(client) => {
                let response = client.migrate_vm(tonic::Request::new(request))
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("gRPC error: {}", e),
                    })?;
                Ok(response.into_inner())
            }
            ClientTransport::Iroh(_client) => {
                // TODO: Implement migrate_vm for Iroh
                Err(BlixardError::NotImplemented {
                    feature: "migrate_vm for Iroh transport".to_string(),
                })
            }
        }
    }
}

/// Helper to load transport configuration from environment or defaults
pub fn get_transport_config() -> Option<TransportConfig> {
    // Check environment variable for transport preference
    if let Ok(transport) = std::env::var("BLIXARD_TRANSPORT") {
        match transport.as_str() {
            "grpc" => Some(TransportConfig::Grpc(Default::default())),
            "iroh" => Some(TransportConfig::Iroh(Default::default())),
            _ => None,
        }
    } else {
        // Default to None, which will use the default (Iroh)
        None
    }
}