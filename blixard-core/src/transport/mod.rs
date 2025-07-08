//! Iroh transport layer for Blixard
//!
//! This module provides the Iroh P2P transport implementation for all
//! Blixard communication.

pub mod cluster_operations_adapter;
pub mod config;
pub mod iroh_client;
pub mod iroh_cluster_service;
pub mod iroh_health_service;
pub mod iroh_peer_connector;
pub mod iroh_protocol;
pub mod iroh_raft_transport;
pub mod iroh_service;
pub mod iroh_service_runner;
pub mod iroh_status_service;
pub mod iroh_vm_service;
pub mod metrics;
pub mod raft_transport_adapter;
pub mod services;

// Iroh security modules
pub mod iroh_middleware;
pub mod iroh_secure_vm_service;
pub mod secure_iroh_protocol_handler;

#[cfg(test)]
mod tests;

use crate::error::{BlixardError, BlixardResult};
use std::net::SocketAddr;

/// ALPN protocol identifier for Blixard RPC
pub const BLIXARD_ALPN: &[u8] = b"blixard/rpc/1";

/// Iroh transport implementation  
pub struct IrohTransport {
    pub endpoint: iroh::Endpoint,
    pub node_id: iroh::NodeId,
}

impl IrohTransport {
    /// Create a new Iroh transport
    pub fn new(endpoint: iroh::Endpoint) -> Self {
        let node_id = endpoint.node_id();
        Self { endpoint, node_id }
    }

    /// Get the node ID
    pub fn node_id(&self) -> iroh::NodeId {
        self.node_id
    }

    /// Get the endpoint
    pub fn endpoint(&self) -> &iroh::Endpoint {
        &self.endpoint
    }
}
