//! Node module - distributed orchestration node implementation
//!
//! This module contains the core Node implementation split into logical components:
//! - `core`: Main Node struct and basic operations
//! - `initialization`: Node setup and component initialization
//! - `cluster`: Cluster join/leave operations and worker management
//! - `lifecycle`: Node start/stop operations
//! - `transport`: Raft transport setup and message handling

mod core;
mod initialization;
mod cluster;
mod lifecycle;
mod transport;

// Re-export the main Node struct and its implementation
pub use core::Node;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::NodeConfig;

    #[tokio::test]
    async fn test_node_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
        let config = NodeConfig {
            id: 1,
            data_dir: "/tmp/test".to_string(),
            bind_addr: "127.0.0.1:0".parse().expect("valid test address"),
            join_addr: None,
            use_tailscale: false,
            vm_backend: "mock".to_string(),
            transport_config: None,
            topology: Default::default(),
        };

        let mut node = Node::new(config);

        // Start node
        node.start().await?;
        assert!(node.is_running().await);

        // Stop node
        node.stop().await?;
        assert!(!node.is_running().await);
        
        Ok(())
    }
}