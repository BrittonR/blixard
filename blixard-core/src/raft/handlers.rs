//! Handler implementations for Raft event processing
//!
//! This module provides concrete implementations of the event handler traits
//! that bridge to the RaftManager's existing methods.

use crate::error::{BlixardError, BlixardResult};
use crate::raft_storage::RedbRaftStorage;

use super::config_manager::RaftConfigManager;
use super::event_loop::{ConfChangeHandler, MessageHandler, ProposalHandler, TickHandler};
use super::messages::{RaftConfChange, RaftProposal};
use super::snapshot::RaftSnapshotManager;

use async_trait::async_trait;
use raft::prelude::RawNode;
use raft::StateRole;
use slog::{info, warn, Logger};
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use tokio::sync::{oneshot, RwLock};
use tracing::instrument;

// Type aliases for complex types used in handlers
type PendingProposals = Arc<RwLock<HashMap<Vec<u8>, oneshot::Sender<BlixardResult<()>>>>>;
type RaftNodeHandle = Arc<RwLock<RawNode<RedbRaftStorage>>>;
type OnReadyFunction = Arc<
    dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = BlixardResult<()>> + Send>>
        + Send
        + Sync,
>;

/// Handler for tick events that delegates to RaftManager
pub struct RaftTickHandler {
    raft_node: RaftNodeHandle,
    snapshot_manager: Arc<RaftSnapshotManager>,
    on_ready_fn: OnReadyFunction,
    #[allow(dead_code)] // Shared state reserved for future Raft handler coordination
    shared_state: Weak<crate::node_shared::SharedNodeState>,
    #[allow(dead_code)] // Logger reserved for future Raft event logging
    logger: Logger,
}

impl RaftTickHandler {
    pub fn new(
        raft_node: RaftNodeHandle,
        snapshot_manager: Arc<RaftSnapshotManager>,
        on_ready_fn: OnReadyFunction,
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
                self.snapshot_manager
                    .check_and_send_snapshots(&mut node)
                    .await?;
            }
        }

        Ok(())
    }
}

/// Handler for proposal events that delegates to RaftManager
pub struct RaftProposalHandler {
    raft_node: RaftNodeHandle,
    pending_proposals: PendingProposals,
    on_ready_fn: OnReadyFunction,
    #[allow(dead_code)] // Logger reserved for future proposal event logging
    logger: Logger,
}

impl RaftProposalHandler {
    pub fn new(
        raft_node: RaftNodeHandle,
        pending_proposals: PendingProposals,
        on_ready_fn: OnReadyFunction,
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
    raft_node: RaftNodeHandle,
    on_ready_fn: OnReadyFunction,
    logger: Logger,
}

impl RaftMessageHandler {
    pub fn new(
        raft_node: RaftNodeHandle,
        on_ready_fn: OnReadyFunction,
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
    raft_node: RaftNodeHandle,
    config_manager: Arc<RaftConfigManager>,
    #[allow(dead_code)] // Reserved for future logging of configuration changes
    logger: Logger,
}

impl RaftConfChangeHandler {
    pub fn new(
        raft_node: RaftNodeHandle,
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
        self.config_manager
            .handle_conf_change(conf_change, &mut node)
            .await
    }
}

/// Factory for creating handler implementations
pub struct HandlerFactory;

impl HandlerFactory {
    /// Create a complete set of handlers for a RaftManager
    pub fn create_handlers(
        raft_node: RaftNodeHandle,
        config_manager: Arc<RaftConfigManager>,
        snapshot_manager: Arc<RaftSnapshotManager>,
        pending_proposals: PendingProposals,
        on_ready_fn: OnReadyFunction,
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

        (
            tick_handler,
            proposal_handler,
            message_handler,
            conf_change_handler,
        )
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
            let storage = crate::raft_storage::RedbRaftStorage {
                database: database.clone(),
            };
            let config = raft::Config {
                id: 1,
                ..Default::default()
            };
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
                Box::pin(async { Ok(()) })
                    as std::pin::Pin<
                        Box<dyn std::future::Future<Output = BlixardResult<()>> + Send>,
                    >
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
            // Box::new always creates a valid pointer, so we just verify they exist
            // by checking that we can access them as trait objects
            let _: &dyn TickHandler = tick_handler.as_ref();
            let _: &dyn ProposalHandler = proposal_handler.as_ref();
            let _: &dyn MessageHandler = message_handler.as_ref();
            let _: &dyn ConfChangeHandler = conf_change_handler.as_ref();
        }
    }
}
