//! Iroh endpoint manager for simplified endpoint creation
//!
//! This is a stub implementation to maintain compilation compatibility.

use crate::error::{BlixardError, BlixardResult};
use iroh::{Endpoint, NodeAddr, Watcher};

/// Manager for Iroh endpoints
pub struct IrohEndpointManager {
    endpoint: Endpoint,
    node_addr: NodeAddr,
}

impl IrohEndpointManager {
    /// Create a new endpoint manager with specific port
    pub async fn new_with_port(alpns: Vec<Vec<u8>>, port: u16) -> BlixardResult<Self> {
        let bind_addr = format!("0.0.0.0:{}", port);
        
        let endpoint = iroh::Endpoint::builder()
            .alpns(alpns)
            .bind_addr_v4(bind_addr.parse().map_err(|e| BlixardError::Internal {
                message: format!("Failed to parse bind address: {}", e),
            })?)
            .bind()
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to create endpoint: {}", e),
            })?;
        
        // Wait a moment for endpoint to initialize and get node_addr
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        // Get the node_addr from the watcher
        let mut node_addr_watcher = endpoint.node_addr();
        let mut node_addr = if let Some(addr) = node_addr_watcher.get() {
            addr
        } else {
            // Fallback: construct a basic NodeAddr with just the node ID
            let node_id = endpoint.node_id();
            iroh::NodeAddr::new(node_id)
        };
        
        // Add direct addresses if not present
        if node_addr.direct_addresses().next().is_none() {
            // Add the bind address as a direct address
            let socket_addr: std::net::SocketAddr = format!("127.0.0.1:{}", port)
                .parse()
                .map_err(|e| BlixardError::Internal {
                    message: format!("Failed to parse socket address: {}", e),
                })?;
            node_addr = node_addr.with_direct_addresses([socket_addr]);
        }
        
        Ok(Self {
            endpoint,
            node_addr,
        })
    }
    
    /// Get the endpoint
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }
    
    /// Get the node address
    pub fn node_addr(&self) -> &NodeAddr {
        &self.node_addr
    }
    
    /// Get the node ticket
    pub fn node_ticket(&self) -> String {
        // Create a proper NodeTicket from NodeAddr
        let ticket = iroh_base::ticket::NodeTicket::new(self.node_addr.clone());
        ticket.to_string()
    }
}