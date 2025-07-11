//! Handler implementations for Raft event processing
//!
//! This module provides concrete implementations of the event handler traits
//! that bridge to the RaftManager's existing methods.

use crate::error::{BlixardError, BlixardResult};
use crate::raft_storage::RedbRaftStorage;

use super::event_loop::{TickHandler, ProposalHandler, MessageHandler, ConfChangeHandler};
use super::messages::{RaftConfChange, RaftProposal};
use super::config_manager::RaftConfigManager;
use super::snapshot::RaftSnapshotManager;

use async_trait::async_trait;
use raft::prelude::RawNode;
use raft::StateRole;
use std::sync::{Arc, Weak};
use std::collections::HashMap;
use tokio::sync::{oneshot, RwLock};
use slog::{info, warn, Logger};
use tracing::instrument;

/// Handler for tick events that delegates to RaftManager
pub struct RaftTickHandler {
    raft_node: Arc<RwLock<RawNode<RedbRaftStorage>>>,
    snapshot_manager: Arc<RaftSnapshotManager>,
    on_ready_fn: Arc<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = BlixardResult<()>> + Send>> + Send + Sync>,
    shared_state: Weak<crate::node_shared::SharedNodeState>,
    logger: Logger,
}

impl RaftTickHandler {
    pub fn new(
        raft_node: Arc<RwLock<RawNode<RedbRaftStorage>>>,
        snapshot_manager: Arc<RaftSnapshotManager>,
        on_ready_fn: Arc<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = BlixardResult<()>> + Send>> + Send + Sync>,
        shared_state: Weak<crate::node_shared::SharedNodeState>,
        logger: Logger,
    ) -> Self {
        Self {
            raft_node,
            snapshot_manager,
            on_ready_fn,
            shared_state,
            logger,
        }
    }
}

#[async_trait]
impl TickHandler for RaftTickHandler {
    #[instrument(skip(self))]
    async fn handle_tick(&self) -> BlixardResult<()> {
        {
            let mut node = self.raft_node.write().await;
            node.tick();
        }

        // Process any ready state that results from the tick
        (self.on_ready_fn)().await?;

        // Check if we need to send snapshots to lagging followers
        // Always check since we successfully processed ready state
        {
            let mut node = self.raft_node.write().await;
            if node.raft.state == StateRole::Leader {
                self.snapshot_manager.check_and_send_snapshots(&mut node).await?;
            }
        }

        Ok(())
    }
}

/// Handler for proposal events that delegates to RaftManager
pub struct RaftProposalHandler {
    raft_node: Arc<RwLock<RawNode<RedbRaftStorage>>>,
    pending_proposals: Arc<RwLock<HashMap<Vec<u8>, oneshot::Sender<BlixardResult<()>>>>>,
    on_ready_fn: Arc<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = BlixardResult<()>> + Send>> + Send + Sync>,
    logger: Logger,
}

impl RaftProposalHandler {
    pub fn new(
        raft_node: Arc<RwLock<RawNode<RedbRaftStorage>>>,
        pending_proposals: Arc<RwLock<HashMap<Vec<u8>, oneshot::Sender<BlixardResult<()>>>>>,
        on_ready_fn: Arc<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = BlixardResult<()>> + Send>> + Send + Sync>,
        logger: Logger,
    ) -> Self {
        Self {
            raft_node,
            pending_proposals,
            on_ready_fn,
            logger,
        }
    }
}

#[async_trait]
impl ProposalHandler for RaftProposalHandler {
    #[instrument(skip(self, proposal), fields(proposal_id = %hex::encode(&proposal.id)))]
    async fn handle_proposal(&self, proposal: RaftProposal) -> BlixardResult<()> {
        info!(self.logger, "[PROPOSAL-HANDLER] Processing proposal");

        // Store the response channel for later
        if let Some(response_tx) = proposal.response_tx {
            let mut pending = self.pending_proposals.write().await;
            pending.insert(proposal.id.clone(), response_tx);
        }

        // Serialize the proposal data
        let data = bincode::serialize(&proposal.data).map_err(|e| BlixardError::Serialization {
            operation: "serialize proposal".to_string(),
            source: Box::new(e),
        })?;

        // Propose through Raft
        {
            let mut node = self.raft_node.write().await;
            
            if node.raft.state != StateRole::Leader {
                // Remove from pending since we're not leader
                let mut pending = self.pending_proposals.write().await;
                if let Some(response_tx) = pending.remove(&proposal.id) {
                    let _ = response_tx.send(Err(BlixardError::NotLeader {
                        operation: "propose".to_string(),
                        leader_id: Some(node.raft.leader_id),
                    }));
                }
                return Ok(());
            }

            node.propose(proposal.id, data)?;
        }

        // Process ready state to handle the proposal
        (self.on_ready_fn)().await?;

        Ok(())
    }
}

/// Handler for Raft message events that delegates to RaftManager
pub struct RaftMessageHandler {
    raft_node: Arc<RwLock<RawNode<RedbRaftStorage>>>,
    on_ready_fn: Arc<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = BlixardResult<()>> + Send>> + Send + Sync>,
    logger: Logger,
}

impl RaftMessageHandler {
    pub fn new(
        raft_node: Arc<RwLock<RawNode<RedbRaftStorage>>>,
        on_ready_fn: Arc<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = BlixardResult<()>> + Send>> + Send + Sync>,
        logger: Logger,
    ) -> Self {
        Self {
            raft_node,
            on_ready_fn,
            logger,
        }
    }
}

#[async_trait]
impl MessageHandler for RaftMessageHandler {
    #[instrument(skip(self, msg), fields(from, msg_type = ?msg.msg_type()))]
    async fn handle_message(&self, from: u64, msg: raft::prelude::Message) -> BlixardResult<()> {
        tracing::Span::current().record("from", from);

        info!(self.logger, "[MESSAGE-HANDLER] Processing message"; 
            "from" => from, "type" => ?msg.msg_type());

        {
            let mut node = self.raft_node.write().await;
            if let Err(e) = node.step(msg) {
                warn!(self.logger, "[MESSAGE-HANDLER] Failed to step message"; "error" => %e);
                return Err(BlixardError::Raft {
                    operation: "step message".to_string(),
                    source: Box::new(e),
                });
            }
        }

        // Process ready state after stepping the message
        (self.on_ready_fn)().await?;

        Ok(())
    }
}

/// Handler for configuration change events that delegates to RaftManager
pub struct RaftConfChangeHandler {
    raft_node: Arc<RwLock<RawNode<RedbRaftStorage>>>,
    config_manager: Arc<RaftConfigManager>,
    logger: Logger,
}

impl RaftConfChangeHandler {
    pub fn new(
        raft_node: Arc<RwLock<RawNode<RedbRaftStorage>>>,
        config_manager: Arc<RaftConfigManager>,
        logger: Logger,
    ) -> Self {
        Self {
            raft_node,
            config_manager,
            logger,
        }
    }
}

#[async_trait]
impl ConfChangeHandler for RaftConfChangeHandler {
    async fn handle_conf_change(&self, conf_change: RaftConfChange) -> BlixardResult<()> {
        let mut node = self.raft_node.write().await;
        self.config_manager.handle_conf_change(conf_change, &mut node).await
    }
}

/// Factory for creating handler implementations
pub struct HandlerFactory;

impl HandlerFactory {
    /// Create a complete set of handlers for a RaftManager
    pub fn create_handlers(
        raft_node: Arc<RwLock<RawNode<RedbRaftStorage>>>,
        config_manager: Arc<RaftConfigManager>,
        snapshot_manager: Arc<RaftSnapshotManager>,
        pending_proposals: Arc<RwLock<HashMap<Vec<u8>, oneshot::Sender<BlixardResult<()>>>>>,
        on_ready_fn: Arc<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = BlixardResult<()>> + Send>> + Send + Sync>,
        shared_state: Weak<crate::node_shared::SharedNodeState>,
        logger: Logger,
    ) -> (
        Box<dyn TickHandler + Send + Sync>,
        Box<dyn ProposalHandler + Send + Sync>, 
        Box<dyn MessageHandler + Send + Sync>,
        Box<dyn ConfChangeHandler + Send + Sync>,
    ) {
        let tick_handler = Box::new(RaftTickHandler::new(
            raft_node.clone(),
            snapshot_manager,
            on_ready_fn.clone(),
            shared_state,
            logger.clone(),
        ));

        let proposal_handler = Box::new(RaftProposalHandler::new(
            raft_node.clone(),
            pending_proposals,
            on_ready_fn.clone(),
            logger.clone(),
        ));

        let message_handler = Box::new(RaftMessageHandler::new(
            raft_node.clone(),
            on_ready_fn,
            logger.clone(),
        ));

        let conf_change_handler = Box::new(RaftConfChangeHandler::new(
            raft_node,
            config_manager,
            logger,
        ));

        (tick_handler, proposal_handler, message_handler, conf_change_handler)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers;
    
    #[cfg(feature = "test-helpers")]
    mod handler_tests {
        use super::*;

        #[tokio::test]
        async fn test_handler_factory_creation() {
            // This test verifies that handlers can be created without panicking
            let (database, _temp_dir) = test_helpers::create_test_database().await;
            let logger = super::super::super::utils::create_raft_logger(1);
            
            // Create minimal required components for testing
            let storage = crate::raft_storage::RedbRaftStorage { database: database.clone() };
            let config = raft::Config { id: 1, ..Default::default() };
            let raft_node = raft::RawNode::new(&config, storage, &logger).unwrap();
            let raft_node = Arc::new(RwLock::new(raft_node));
            
            let config_manager = Arc::new(RaftConfigManager::new(
                1,
                database.clone(),
                Arc::new(RwLock::new(std::collections::HashMap::new())),
                logger.clone(),
                std::sync::Weak::new(),
            ));
            
            let snapshot_manager = Arc::new(RaftSnapshotManager::new(database, logger.clone()));
            let pending_proposals = Arc::new(RwLock::new(HashMap::new()));
            
            let on_ready_fn = Arc::new(|| {
                Box::pin(async { Ok(()) }) as std::pin::Pin<Box<dyn std::future::Future<Output = BlixardResult<()>> + Send>>
            });
            
            let (tick_handler, proposal_handler, message_handler, conf_change_handler) = 
                HandlerFactory::create_handlers(
                    raft_node,
                    config_manager,
                    snapshot_manager,
                    pending_proposals,
                    on_ready_fn,
                    std::sync::Weak::new(),
                    logger,
                );
            
            // Test that all handlers were created successfully
            assert!(!std::ptr::eq(tick_handler.as_ref() as *const _, std::ptr::null()));
            assert!(!std::ptr::eq(proposal_handler.as_ref() as *const _, std::ptr::null()));
            assert!(!std::ptr::eq(message_handler.as_ref() as *const _, std::ptr::null()));
            assert!(!std::ptr::eq(conf_change_handler.as_ref() as *const _, std::ptr::null()));
        }
    }
}