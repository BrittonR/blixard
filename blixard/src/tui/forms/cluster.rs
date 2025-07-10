//! Cluster-related forms for the TUI

#[derive(Debug, Clone)]
pub struct CreateClusterForm {
    pub name: String,
    pub node_count: String,
    pub replication_factor: String,
    pub network_mode: String,
    pub data_persistence: bool,
    pub current_field: CreateClusterField,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CreateClusterField {
    Name,
    NodeCount,
    ReplicationFactor,
    NetworkMode,
    DataPersistence,
}

impl CreateClusterForm {
    pub fn new() -> Self {
        Self {
            name: "my-cluster".to_string(),
            node_count: "3".to_string(),
            replication_factor: "3".to_string(),
            network_mode: "bridge".to_string(),
            data_persistence: true,
            current_field: CreateClusterField::Name,
        }
    }
}