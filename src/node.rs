use crate::config::Config;
use crate::error::{BlixardError, Result};
use crate::cluster::RaftNode;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

pub struct Node {
    config: Config,
    node_id: String,
    raft_node: Arc<RwLock<RaftNode>>,
}

impl Node {
    pub async fn init_primary(config: Config, node_id: String) -> Result<Self> {
        info!("Initializing primary node with ID: {}", node_id);
        
        let raft_node = RaftNode::new_primary(config.clone(), node_id.clone()).await?;
        
        Ok(Self {
            config,
            node_id,
            raft_node: Arc::new(RwLock::new(raft_node)),
        })
    }
    
    pub async fn init_secondary(config: Config, node_id: String, primary_addr: String) -> Result<Self> {
        info!("Initializing secondary node with ID: {} connecting to primary at: {}", node_id, primary_addr);
        
        let raft_node = RaftNode::new_secondary(config.clone(), node_id.clone(), primary_addr).await?;
        
        Ok(Self {
            config,
            node_id,
            raft_node: Arc::new(RwLock::new(raft_node)),
        })
    }
    
    pub async fn join_cluster(config: Config, node_id: String, cluster_addr: String) -> Result<Self> {
        info!("Joining cluster at: {} with node ID: {}", cluster_addr, node_id);
        
        let raft_node = RaftNode::join_cluster(config.clone(), node_id.clone(), cluster_addr).await?;
        
        Ok(Self {
            config,
            node_id,
            raft_node: Arc::new(RwLock::new(raft_node)),
        })
    }
    
    pub fn node_id(&self) -> &str {
        &self.node_id
    }
    
    pub async fn is_leader(&self) -> bool {
        let node = self.raft_node.read().await;
        node.is_leader()
    }
    
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down node: {}", self.node_id);
        let mut node = self.raft_node.write().await;
        node.shutdown().await
    }
}