//! Raft bootstrap coordination for single-node cluster initialization
//!
//! This module handles the decision logic and execution for bootstrapping
//! a single-node Raft cluster when no peers are configured.

use crate::error::BlixardResult;
use crate::raft_storage::RedbRaftStorage;

use raft::{RawNode, StateRole};
use slog::{error, info, Logger};
use std::sync::Weak;
use tracing::instrument;

/// Coordinates Raft cluster bootstrap operations
pub struct RaftBootstrapCoordinator {
    node_id: u64,
    shared_state: Weak<crate::node_shared::SharedNodeState>,
    logger: Logger,
}

impl RaftBootstrapCoordinator {
    /// Create a new bootstrap coordinator
    pub fn new(
        node_id: u64,
        shared_state: Weak<crate::node_shared::SharedNodeState>,
        logger: Logger,
    ) -> Self {
        Self {
            node_id,
            shared_state,
            logger,
        }
    }

    /// Determine if this node should bootstrap as a single-node cluster
    pub async fn should_bootstrap(&self, peers: &std::collections::HashMap<u64, String>) -> bool {
        let has_join_addr = self.has_join_address();

        // Only bootstrap if we have no peers AND no join address
        let should_bootstrap = peers.is_empty() && !has_join_addr;

        if should_bootstrap {
            info!(
                self.logger,
                "No peers configured and no join address, will bootstrap as single-node cluster"
            );
        } else {
            let peer_ids: Vec<u64> = peers.keys().cloned().collect();
            info!(self.logger, "Not bootstrapping"; 
                  "peers" => ?peer_ids, 
                  "has_join_addr" => has_join_addr);
        }

        should_bootstrap
    }

    /// Execute bootstrap process if needed
    #[instrument(skip(self, raft_node, on_ready_fn), fields(node_id = self.node_id))]
    pub async fn execute_bootstrap_if_needed(
        &self,
        raft_node: &tokio::sync::RwLock<RawNode<RedbRaftStorage>>,
        peers: &std::collections::HashMap<u64, String>,
        on_ready_fn: impl Fn() -> std::pin::Pin<
            Box<dyn std::future::Future<Output = BlixardResult<()>> + Send>,
        >,
    ) -> BlixardResult<()> {
        if self.should_bootstrap(peers).await {
            info!(self.logger, "Executing single-node cluster bootstrap");

            match self.bootstrap_single_node(raft_node, on_ready_fn).await {
                Ok(_) => {
                    info!(self.logger, "[BOOTSTRAP] Bootstrap succeeded");
                    Ok(())
                }
                Err(e) => {
                    error!(self.logger, "[BOOTSTRAP] Bootstrap failed"; "error" => %e);
                    Err(e)
                }
            }
        } else {
            Ok(())
        }
    }

    /// Bootstrap a single-node cluster
    #[instrument(skip(self, raft_node, on_ready_fn))]
    async fn bootstrap_single_node(
        &self,
        raft_node: &tokio::sync::RwLock<RawNode<RedbRaftStorage>>,
        on_ready_fn: impl Fn() -> std::pin::Pin<
            Box<dyn std::future::Future<Output = BlixardResult<()>> + Send>,
        >,
    ) -> BlixardResult<()> {
        {
            let mut node = raft_node.write().await;

            // Campaign to become leader
            let _ = node.campaign();

            info!(self.logger, "[BOOTSTRAP] After bootstrap campaign";
                "state" => ?node.raft.state,
                "commit" => node.raft.raft_log.committed,
                "applied" => node.raft.raft_log.applied
            );
        }

        // Process ready immediately to handle the bootstrap entry
        on_ready_fn().await?;

        // Propose an empty entry to establish leadership
        {
            let mut node = raft_node.write().await;
            if node.raft.state == StateRole::Leader {
                info!(
                    self.logger,
                    "[BOOTSTRAP] Proposing empty entry to establish leadership"
                );
                node.propose(vec![], vec![])?;
            }
        }

        // Process ready again to commit the empty entry
        on_ready_fn().await?;

        Ok(())
    }

    /// Check if we have a join address configured
    fn has_join_address(&self) -> bool {
        if let Some(shared) = self.shared_state.upgrade() {
            shared.config.join_addr.is_some()
        } else {
            false
        }
    }
}

/// Factory for creating bootstrap coordinators
pub struct BootstrapCoordinatorFactory;

impl BootstrapCoordinatorFactory {
    /// Create a new bootstrap coordinator with proper configuration
    pub fn create(
        node_id: u64,
        shared_state: Weak<crate::node_shared::SharedNodeState>,
        logger: Logger,
    ) -> RaftBootstrapCoordinator {
        RaftBootstrapCoordinator::new(node_id, shared_state, logger)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_test_coordinator() -> RaftBootstrapCoordinator {
        let logger = super::super::utils::create_raft_logger(1);
        RaftBootstrapCoordinator::new(1, std::sync::Weak::new(), logger)
    }

    #[tokio::test]
    async fn test_should_bootstrap_with_no_peers_no_join_addr() {
        let coordinator = create_test_coordinator();
        let peers = HashMap::new();

        let result = coordinator.should_bootstrap(&peers).await;
        assert!(result);
    }

    #[tokio::test]
    async fn test_should_not_bootstrap_with_peers() {
        let coordinator = create_test_coordinator();
        let mut peers = HashMap::new();
        peers.insert(2, "127.0.0.1:8001".to_string());

        let result = coordinator.should_bootstrap(&peers).await;
        assert!(!result);
    }
}
