//! Cluster management request/response types

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStatusRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStatusResponse {
    pub leader_id: u64,
    pub nodes: Vec<NodeInfo>,
    pub term: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinRequest {
    pub node_id: u64,
    pub bind_address: String,
    /// Optional P2P node address information for the joining node
    pub p2p_node_addr: Option<iroh::NodeAddr>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinResponse {
    pub success: bool,
    pub message: String,
    pub peers: Vec<NodeInfo>,
    pub voters: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaveRequest {
    pub node_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaveResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: u64,
    pub address: String,
    pub state: i32, // Maps to NodeState enum
    pub p2p_node_id: String,
    pub p2p_addresses: Vec<String>,
    pub p2p_relay_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum NodeState {
    NodeStateUnknown = 0,
    NodeStateFollower = 1,
    NodeStateCandidate = 2,
    NodeStateLeader = 3,
}

// Protocol compatibility conversions
impl From<NodeState> for i32 {
    fn from(state: NodeState) -> Self {
        state as i32
    }
}

impl TryFrom<i32> for NodeState {
    type Error = crate::error::BlixardError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(NodeState::NodeStateUnknown),
            1 => Ok(NodeState::NodeStateFollower),
            2 => Ok(NodeState::NodeStateCandidate),
            3 => Ok(NodeState::NodeStateLeader),
            _ => Err(crate::error::BlixardError::ConfigurationError {
                component: "node_state".to_string(),
                message: format!("Invalid node state code {}. Valid codes: 0-3", value),
            }),
        }
    }
}

impl NodeInfo {
    pub fn new(id: u64, address: String, p2p_node_id: String) -> Self {
        Self {
            id,
            address,
            state: NodeState::NodeStateUnknown as i32,
            p2p_node_id,
            p2p_addresses: Vec::new(),
            p2p_relay_url: String::new(),
        }
    }
}