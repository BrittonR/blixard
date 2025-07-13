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