//! State management traits for breaking circular dependencies
//!
//! These traits provide interfaces for accessing shared state without
//! requiring tight coupling between components.

use crate::{error::BlixardResult, node_shared::PeerInfo, types::NodeConfig};
use async_trait::async_trait;
use redb::Database;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc;

/// Trait for managing node configuration
#[async_trait]
pub trait ConfigManager: Send + Sync {
    /// Get the current node configuration
    async fn get_config(&self) -> NodeConfig;

    /// Get the node ID
    async fn get_node_id(&self) -> u64 {
        self.get_config().await.id
    }

    /// Get the data directory path
    async fn get_data_dir(&self) -> String {
        self.get_config().await.data_dir
    }

    /// Check if the node is configured for development mode
    async fn is_dev_mode(&self) -> bool {
        // Could be extended to check for dev-specific configuration
        false
    }
}

/// Trait for managing database access
#[async_trait]
pub trait DatabaseManager: Send + Sync {
    /// Get the shared database instance
    async fn get_database(&self) -> Option<Arc<Database>>;

    /// Check if the database is available and initialized
    async fn is_database_available(&self) -> bool {
        self.get_database().await.is_some()
    }
}

/// Trait for managing peer connections and information
#[async_trait]
pub trait PeerManager: Send + Sync {
    /// Add a new peer to the known peers list
    async fn add_peer(&self, peer_id: u64, peer_info: PeerInfo) -> BlixardResult<()>;

    /// Remove a peer from the known peers list
    async fn remove_peer(&self, peer_id: u64) -> BlixardResult<()>;

    /// Get information about a specific peer
    async fn get_peer(&self, peer_id: u64) -> Option<PeerInfo>;

    /// Get all known peers
    async fn get_all_peers(&self) -> HashMap<u64, PeerInfo>;

    /// Update the connection status of a peer
    async fn update_peer_connection_status(
        &self,
        peer_id: u64,
        connected: bool,
    ) -> BlixardResult<()>;

    /// Get the list of connected peers
    async fn get_connected_peers(&self) -> Vec<u64>;

    /// Get the total number of known peers
    async fn peer_count(&self) -> usize {
        self.get_all_peers().await.len()
    }
}

/// Trait for managing communication channels
#[async_trait]
pub trait ChannelManager: Send + Sync {
    /// Get the channel for sending Raft proposals
    async fn get_raft_proposal_tx(&self) -> Option<mpsc::UnboundedSender<RaftProposal>>;

    /// Get the channel for sending Raft messages
    async fn get_raft_message_tx(
        &self,
    ) -> Option<mpsc::UnboundedSender<(u64, raft::prelude::Message)>>;

    /// Set the Raft proposal channel (called during initialization)
    async fn set_raft_proposal_tx(
        &self,
        tx: mpsc::UnboundedSender<RaftProposal>,
    ) -> BlixardResult<()>;

    /// Set the Raft message channel (called during initialization)
    async fn set_raft_message_tx(
        &self,
        tx: mpsc::UnboundedSender<(u64, raft::prelude::Message)>,
    ) -> BlixardResult<()>;
}

/// Trait for managing node lifecycle state
#[async_trait]
pub trait LifecycleManager: Send + Sync {
    /// Check if the node is currently running
    async fn is_running(&self) -> bool;

    /// Check if the node has been initialized
    async fn is_initialized(&self) -> bool;

    /// Mark the node as initialized
    async fn set_initialized(&self, initialized: bool) -> BlixardResult<()>;

    /// Mark the node as running
    async fn set_running(&self, running: bool) -> BlixardResult<()>;

    /// Get the node startup time
    async fn get_startup_time(&self) -> Option<std::time::SystemTime>;

    /// Get the node uptime
    async fn get_uptime(&self) -> Option<std::time::Duration> {
        if let Some(startup_time) = self.get_startup_time().await {
            std::time::SystemTime::now()
                .duration_since(startup_time)
                .ok()
        } else {
            None
        }
    }
}

/// Placeholder for Raft proposal type (would be defined elsewhere)
#[derive(Debug, Clone)]
pub struct RaftProposal {
    pub proposal_type: String,
    pub data: Vec<u8>,
}

/// Combined trait for comprehensive state management
/// This trait combines all the individual management traits for convenience
#[async_trait]
pub trait StateManager:
    ConfigManager + DatabaseManager + PeerManager + ChannelManager + LifecycleManager + Send + Sync
{
    /// Get a comprehensive snapshot of the current node state
    async fn get_state_snapshot(&self) -> NodeStateSnapshot {
        NodeStateSnapshot {
            config: self.get_config().await,
            is_running: self.is_running().await,
            is_initialized: self.is_initialized().await,
            database_available: self.is_database_available().await,
            peer_count: self.peer_count().await,
            connected_peers: self.get_connected_peers().await,
            uptime: self.get_uptime().await,
        }
    }
}

/// A snapshot of the current node state for debugging and monitoring
#[derive(Debug, Clone)]
pub struct NodeStateSnapshot {
    pub config: NodeConfig,
    pub is_running: bool,
    pub is_initialized: bool,
    pub database_available: bool,
    pub peer_count: usize,
    pub connected_peers: Vec<u64>,
    pub uptime: Option<std::time::Duration>,
}

/// Mock implementation for testing
#[cfg(test)]
pub struct MockStateManager {
    config: NodeConfig,
    database: Option<Arc<Database>>,
    peers: tokio::sync::RwLock<HashMap<u64, PeerInfo>>,
    is_running: tokio::sync::RwLock<bool>,
    is_initialized: tokio::sync::RwLock<bool>,
    startup_time: Option<std::time::SystemTime>,
    raft_proposal_tx: tokio::sync::RwLock<Option<mpsc::UnboundedSender<RaftProposal>>>,
    raft_message_tx:
        tokio::sync::RwLock<Option<mpsc::UnboundedSender<(u64, raft::prelude::Message)>>>,
}

#[cfg(test)]
impl MockStateManager {
    pub fn new(config: NodeConfig) -> Self {
        Self {
            config,
            database: None,
            peers: tokio::sync::RwLock::new(HashMap::new()),
            is_running: tokio::sync::RwLock::new(false),
            is_initialized: tokio::sync::RwLock::new(false),
            startup_time: Some(std::time::SystemTime::now()),
            raft_proposal_tx: tokio::sync::RwLock::new(None),
            raft_message_tx: tokio::sync::RwLock::new(None),
        }
    }

    pub fn with_database(mut self, database: Arc<Database>) -> Self {
        self.database = Some(database);
        self
    }
}

#[cfg(test)]
#[async_trait]
impl ConfigManager for MockStateManager {
    async fn get_config(&self) -> NodeConfig {
        self.config.clone()
    }
}

#[cfg(test)]
#[async_trait]
impl DatabaseManager for MockStateManager {
    async fn get_database(&self) -> Option<Arc<Database>> {
        self.database.clone()
    }
}

#[cfg(test)]
#[async_trait]
impl PeerManager for MockStateManager {
    async fn add_peer(&self, peer_id: u64, peer_info: PeerInfo) -> BlixardResult<()> {
        self.peers.write().await.insert(peer_id, peer_info);
        Ok(())
    }

    async fn remove_peer(&self, peer_id: u64) -> BlixardResult<()> {
        self.peers.write().await.remove(&peer_id);
        Ok(())
    }

    async fn get_peer(&self, peer_id: u64) -> Option<PeerInfo> {
        self.peers.read().await.get(&peer_id).cloned()
    }

    async fn get_all_peers(&self) -> HashMap<u64, PeerInfo> {
        self.peers.read().await.clone()
    }

    async fn update_peer_connection_status(
        &self,
        peer_id: u64,
        connected: bool,
    ) -> BlixardResult<()> {
        if let Some(peer_info) = self.peers.write().await.get_mut(&peer_id) {
            peer_info.is_connected = connected;
        }
        Ok(())
    }

    async fn get_connected_peers(&self) -> Vec<u64> {
        self.peers
            .read()
            .await
            .iter()
            .filter(|(_, peer)| peer.is_connected)
            .map(|(id, _)| *id)
            .collect()
    }
}

#[cfg(test)]
#[async_trait]
impl ChannelManager for MockStateManager {
    async fn get_raft_proposal_tx(&self) -> Option<mpsc::UnboundedSender<RaftProposal>> {
        self.raft_proposal_tx.read().await.clone()
    }

    async fn get_raft_message_tx(
        &self,
    ) -> Option<mpsc::UnboundedSender<(u64, raft::prelude::Message)>> {
        self.raft_message_tx.read().await.clone()
    }

    async fn set_raft_proposal_tx(
        &self,
        tx: mpsc::UnboundedSender<RaftProposal>,
    ) -> BlixardResult<()> {
        *self.raft_proposal_tx.write().await = Some(tx);
        Ok(())
    }

    async fn set_raft_message_tx(
        &self,
        tx: mpsc::UnboundedSender<(u64, raft::prelude::Message)>,
    ) -> BlixardResult<()> {
        *self.raft_message_tx.write().await = Some(tx);
        Ok(())
    }
}

#[cfg(test)]
#[async_trait]
impl LifecycleManager for MockStateManager {
    async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    async fn is_initialized(&self) -> bool {
        *self.is_initialized.read().await
    }

    async fn set_initialized(&self, initialized: bool) -> BlixardResult<()> {
        *self.is_initialized.write().await = initialized;
        Ok(())
    }

    async fn set_running(&self, running: bool) -> BlixardResult<()> {
        *self.is_running.write().await = running;
        Ok(())
    }

    async fn get_startup_time(&self) -> Option<std::time::SystemTime> {
        self.startup_time
    }
}

#[cfg(test)]
#[async_trait]
impl StateManager for MockStateManager {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::NodeConfig;

    #[tokio::test]
    async fn test_mock_state_manager() {
        let config = NodeConfig {
            id: 1,
            address: "127.0.0.1:7001".to_string(),
            data_dir: "/tmp/test".to_string(),
        };

        let state_manager = MockStateManager::new(config.clone());

        // Test config management
        assert_eq!(state_manager.get_node_id().await, 1);
        assert_eq!(state_manager.get_config().await.address, "127.0.0.1:7001");

        // Test lifecycle management
        assert!(!state_manager.is_running().await);
        assert!(!state_manager.is_initialized().await);

        state_manager.set_initialized(true).await.unwrap();
        assert!(state_manager.is_initialized().await);

        // Test peer management
        let peer_info = PeerInfo {
            id: 2,
            address: "127.0.0.1:7002".to_string(),
            is_connected: true,
            p2p_node_id: None,
            p2p_addresses: vec![],
        };

        state_manager.add_peer(2, peer_info.clone()).await.unwrap();
        assert_eq!(state_manager.peer_count().await, 1);
        assert_eq!(state_manager.get_connected_peers().await, vec![2]);

        // Test state snapshot
        let snapshot = state_manager.get_state_snapshot().await;
        assert_eq!(snapshot.config.id, 1);
        assert!(snapshot.is_initialized);
        assert!(!snapshot.is_running);
        assert_eq!(snapshot.peer_count, 1);
        assert_eq!(snapshot.connected_peers, vec![2]);
    }
}
