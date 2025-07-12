//! Node-related forms for the TUI

use crate::tui::types::node::NodeInfo;

#[derive(Debug, Clone)]
pub struct CreateNodeForm {
    pub id: String,
    pub bind_address: String,
    pub data_dir: String,
    pub peers: String,
    pub vm_backend: String,
    pub daemon_mode: bool,
    pub current_field: CreateNodeField,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CreateNodeField {
    Id,
    BindAddress,
    DataDir,
    Peers,
    VmBackend,
    DaemonMode,
}

impl CreateNodeForm {
    pub fn new() -> Self {
        Self {
            id: String::new(),
            bind_address: "127.0.0.1:7001".to_string(),
            data_dir: "./data".to_string(),
            peers: String::new(),
            vm_backend: "microvm".to_string(),
            daemon_mode: false,
            current_field: CreateNodeField::Id,
        }
    }

    pub fn new_with_smart_defaults(existing_nodes: &[NodeInfo]) -> Self {
        let mut form = Self::new();

        // Find next available node ID
        let existing_ids: Vec<u64> = existing_nodes.iter().map(|n| n.id).collect();
        let mut next_id = 1;
        while existing_ids.contains(&next_id) {
            next_id += 1;
        }
        form.id = next_id.to_string();

        // Smart bind address generation
        if let Some(last_node) = existing_nodes.last() {
            if let Some(port) = last_node.address.split(':').last() {
                if let Ok(port_num) = port.parse::<u16>() {
                    form.bind_address = format!("127.0.0.1:{}", port_num + 1);
                }
            }
        }

        // Smart data directory
        form.data_dir = format!("./data/node-{}", next_id);

        // Peers from existing nodes
        if !existing_nodes.is_empty() {
            let peer_addrs: Vec<String> = existing_nodes
                .iter()
                .map(|n| n.address.clone())
                .collect();
            form.peers = peer_addrs.join(",");
        }

        form
    }
}

#[derive(Debug, Clone)]
pub struct EditNodeForm {
    pub node_id: u64,
    pub bind_address: String,
    pub data_dir: String,
    pub vm_backend: String,
    pub current_field: EditNodeField,
    pub original_address: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EditNodeField {
    BindAddress,
    DataDir,
    VmBackend,
}

impl EditNodeForm {
    pub fn from_node_info(node: &NodeInfo) -> Self {
        Self {
            node_id: node.id,
            bind_address: node.address.clone(),
            data_dir: format!("./data/node-{}", node.id), // Default guess
            vm_backend: "microvm".to_string(),
            current_field: EditNodeField::BindAddress,
            original_address: node.address.clone(),
        }
    }
}