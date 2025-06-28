//! Client factory for creating transport-agnostic clients

use crate::error::{BlixardError, BlixardResult};
use crate::proto::cluster_service_client::ClusterServiceClient;
use crate::proto::blixard_service_client::BlixardServiceClient;
use crate::transport::config::{TransportConfig, ServiceType, RaftTransportPreference};
use crate::transport::service_factory::TransportType;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::Channel;

/// Peer address information supporting multiple transports
#[derive(Debug, Clone)]
pub struct PeerAddress {
    /// Node ID
    pub node_id: u64,
    
    /// gRPC address
    pub grpc_addr: Option<SocketAddr>,
    
    /// Iroh node address
    pub iroh_node_addr: Option<iroh::NodeAddr>,
}

/// Factory for creating clients with the appropriate transport
pub struct ClientFactory {
    transport_config: TransportConfig,
    iroh_endpoint: Option<Arc<iroh::Endpoint>>,
    
    /// Cached performance metrics for adaptive transport selection
    transport_metrics: Arc<parking_lot::RwLock<TransportMetrics>>,
}

#[derive(Default)]
struct TransportMetrics {
    /// Average latency in milliseconds for each transport
    grpc_latency_ms: f64,
    iroh_latency_ms: f64,
    
    /// Sample counts
    grpc_samples: u64,
    iroh_samples: u64,
}

impl ClientFactory {
    pub fn new(transport_config: TransportConfig, iroh_endpoint: Option<Arc<iroh::Endpoint>>) -> Self {
        Self {
            transport_config,
            iroh_endpoint,
            transport_metrics: Arc::new(parking_lot::RwLock::new(TransportMetrics::default())),
        }
    }
    
    /// Create a cluster service client for the given peer
    pub async fn create_cluster_client(
        &self,
        peer_addr: &PeerAddress,
    ) -> BlixardResult<ClusterServiceClient<Channel>> {
        let transport_type = self.select_transport_for_peer(peer_addr, ServiceType::Status);
        
        match transport_type {
            TransportType::Grpc => self.create_grpc_client(peer_addr).await,
            TransportType::Iroh => self.create_iroh_client(peer_addr).await,
        }
    }
    
    /// Create a cluster service client specifically for Raft messages
    pub async fn create_raft_client(
        &self,
        peer_addr: &PeerAddress,
    ) -> BlixardResult<ClusterServiceClient<Channel>> {
        let transport_type = self.select_raft_transport(peer_addr);
        
        match transport_type {
            TransportType::Grpc => self.create_grpc_client(peer_addr).await,
            TransportType::Iroh => self.create_iroh_client(peer_addr).await,
        }
    }
    
    /// Create a gRPC client
    async fn create_grpc_client(
        &self,
        peer_addr: &PeerAddress,
    ) -> BlixardResult<ClusterServiceClient<Channel>> {
        let grpc_addr = peer_addr.grpc_addr
            .ok_or_else(|| BlixardError::Internal {
                message: format!("No gRPC address for peer {}", peer_addr.node_id),
            })?;
        
        let channel = Channel::from_shared(format!("http://{}", grpc_addr))
            .map_err(|e| BlixardError::GrpcError(e.to_string()))?
            .connect()
            .await
            .map_err(|e| BlixardError::GrpcError(e.to_string()))?;
        
        Ok(ClusterServiceClient::new(channel))
    }
    
    /// Create an Iroh-based client
    async fn create_iroh_client(
        &self,
        peer_addr: &PeerAddress,
    ) -> BlixardResult<ClusterServiceClient<Channel>> {
        let _endpoint = self.iroh_endpoint.as_ref()
            .ok_or_else(|| BlixardError::Internal {
                message: "Iroh endpoint not initialized".to_string(),
            })?;
        
        let _node_addr = peer_addr.iroh_node_addr.as_ref()
            .ok_or_else(|| BlixardError::Internal {
                message: format!("No Iroh address for peer {}", peer_addr.node_id),
            })?;
        
        // TODO: Implement Iroh client creation using our custom RPC
        // For now, return not implemented
        Err(BlixardError::NotImplemented {
            feature: "Iroh client creation in client factory".to_string(),
        })
    }
    
    /// Select transport type for a given peer and service
    fn select_transport_for_peer(
        &self,
        peer_addr: &PeerAddress,
        service_type: ServiceType,
    ) -> TransportType {
        match &self.transport_config {
            TransportConfig::Grpc(_) => TransportType::Grpc,
            TransportConfig::Iroh(_) => TransportType::Iroh,
            TransportConfig::Dual { strategy, .. } => {
                // Check if we should prefer Iroh for this service type
                if strategy.prefer_iroh_for.contains(&service_type) {
                    // Check if the peer supports Iroh
                    if peer_addr.iroh_node_addr.is_some() {
                        TransportType::Iroh
                    } else if strategy.fallback_to_grpc {
                        TransportType::Grpc
                    } else {
                        TransportType::Iroh // Will fail if peer doesn't support it
                    }
                } else {
                    TransportType::Grpc
                }
            }
        }
    }
    
    /// Select transport specifically for Raft messages
    fn select_raft_transport(&self, peer_addr: &PeerAddress) -> TransportType {
        match &self.transport_config {
            TransportConfig::Grpc(_) => TransportType::Grpc,
            TransportConfig::Iroh(_) => TransportType::Iroh,
            TransportConfig::Dual { strategy, .. } => {
                match strategy.raft_transport {
                    RaftTransportPreference::AlwaysGrpc => TransportType::Grpc,
                    RaftTransportPreference::AlwaysIroh => TransportType::Iroh,
                    RaftTransportPreference::Adaptive { latency_threshold_ms } => {
                        // Use adaptive selection based on measured latencies
                        let metrics = self.transport_metrics.read();
                        
                        // Need enough samples to make a decision
                        if metrics.grpc_samples < 10 || metrics.iroh_samples < 10 {
                            // Not enough data, use gRPC as default
                            TransportType::Grpc
                        } else if metrics.iroh_latency_ms < latency_threshold_ms {
                            // Iroh meets the latency requirement
                            TransportType::Iroh
                        } else {
                            // Iroh is too slow for Raft
                            TransportType::Grpc
                        }
                    }
                }
            }
        }
    }
    
    /// Update transport metrics after a successful RPC
    pub fn record_latency(&self, transport: TransportType, latency_ms: f64) {
        let mut metrics = self.transport_metrics.write();
        
        match transport {
            TransportType::Grpc => {
                // Update moving average
                metrics.grpc_latency_ms = 
                    (metrics.grpc_latency_ms * metrics.grpc_samples as f64 + latency_ms) 
                    / (metrics.grpc_samples + 1) as f64;
                metrics.grpc_samples += 1;
            }
            TransportType::Iroh => {
                // Update moving average
                metrics.iroh_latency_ms = 
                    (metrics.iroh_latency_ms * metrics.iroh_samples as f64 + latency_ms) 
                    / (metrics.iroh_samples + 1) as f64;
                metrics.iroh_samples += 1;
            }
        }
    }
}

/// Extension trait for adding transport selection to clients
pub trait TransportClientExt {
    /// Get the transport type used by this client
    fn transport_type(&self) -> TransportType;
}

// We would implement this trait for our client types