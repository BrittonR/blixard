//! Transport abstraction layer for dual gRPC/Iroh support
//!
//! This module provides a unified interface for both gRPC and Iroh transports,
//! allowing gradual migration from gRPC to Iroh-based communication.

pub mod config;
pub mod service_factory;
pub mod client_factory;
pub mod metrics;
// pub mod iroh_grpc_bridge; // Not used - we implemented custom RPC instead
pub mod services;
pub mod dual_service_runner;
pub mod iroh_peer_connector;
pub mod iroh_protocol;
pub mod iroh_service;
pub mod iroh_health_service;
pub mod iroh_status_service;
pub mod iroh_vm_service;
pub mod iroh_raft_transport;
pub mod raft_transport_adapter;
pub mod iroh_cluster_service;
pub mod cluster_operations_adapter;
pub mod iroh_client;

// Iroh security modules
pub mod iroh_middleware;
pub mod iroh_secure_vm_service;
pub mod secure_iroh_protocol_handler;

#[cfg(test)]
mod tests;

use crate::error::{BlixardError, BlixardResult};
use std::net::SocketAddr;
use tonic::transport::Channel;

/// ALPN protocol identifier for Blixard RPC
pub const BLIXARD_ALPN: &[u8] = b"blixard/rpc/1";

/// Transport types supported by Blixard
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportType {
    Grpc,
    Iroh,
}

/// Unified transport abstraction
pub enum Transport {
    /// Traditional gRPC transport
    Grpc(GrpcTransport),
    /// Iroh P2P transport
    Iroh(IrohTransport),
    /// Dual transport mode for gradual migration
    Dual(GrpcTransport, IrohTransport),
}

/// gRPC transport implementation
pub struct GrpcTransport {
    bind_addr: SocketAddr,
}

/// Iroh transport implementation  
pub struct IrohTransport {
    endpoint: iroh::Endpoint,
    node_id: iroh::NodeId,
}

impl Transport {
    /// Get the preferred transport type for a given service
    pub fn preferred_for_service(&self, service: &str) -> TransportType {
        match self {
            Transport::Grpc(_) => TransportType::Grpc,
            Transport::Iroh(_) => TransportType::Iroh,
            Transport::Dual(_, _) => {
                // TODO: Implement migration strategy logic
                TransportType::Grpc
            }
        }
    }
}