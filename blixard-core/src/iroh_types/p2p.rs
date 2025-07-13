//! P2P status request/response types

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetP2pStatusRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetP2pStatusResponse {
    pub enabled: bool,
    pub node_id: u64,
    pub node_id_base64: String,
    pub direct_addresses: Vec<String>,
    pub relay_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapInfo {
    pub node_id: u64,                    // Cluster node ID
    pub p2p_node_id: String,            // Iroh P2P node ID (base64)
    pub p2p_addresses: Vec<String>,     // Direct P2P addresses
    pub p2p_relay_url: Option<String>,  // Optional relay URL
}