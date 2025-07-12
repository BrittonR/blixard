//! Core Raft manager implementation
//!
//! This module contains the main RaftManager struct and its core functionality
//! including initialization, the main event loop, and ready state processing.

use crate::config_global;
use crate::error::{BlixardError, BlixardResult};
#[cfg(feature = "observability")]
use crate::metrics_otel::{attributes, metrics, Timer};
use crate::raft_storage::RedbRaftStorage;

use super::bootstrap::RaftBootstrapCoordinator;
use super::config_manager::RaftConfigManager;
use super::event_loop::EventLoopFactory;
use super::handlers::HandlerFactory;
use super::messages::{RaftConfChange, RaftProposal};
use super::snapshot::RaftSnapshotManager;
use super::state_machine::RaftStateMachine;

use raft::prelude::{Config, Entry, RawNode, Ready};
use raft::{GetEntriesContext, StateRole, Storage};
use redb::Database;
use slog::{error, info, warn, Logger};
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::time::{interval, Duration};
use tracing::{debug, instrument};

// Type aliases for complex types
type PendingProposals = Arc<RwLock<HashMap<Vec<u8>, oneshot::Sender<BlixardResult<()>>>>>;
type MessageChannel = mpsc::UnboundedSender<(u64, raft::prelude::Message)>;
type ProposalChannel = mpsc::UnboundedSender<RaftProposal>;
type ConfChangeChannel = mpsc::UnboundedSender<RaftConfChange>;
type RaftNodeHandle = RwLock<RawNode<RedbRaftStorage>>;
type OnReadyFunction = Arc<
    dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = BlixardResult<()>> + Send>>
        + Send
        + Sync,
>;

/// Main Raft manager that handles the Raft protocol
pub struct RaftManager {
    node_id: u64,
    raft_node: RaftNodeHandle,
    storage: RedbRaftStorage,
    logger: Logger,
    shared_state: Weak<crate::node_shared::SharedNodeState>,

    // Subcomponents
    state_machine: Arc<RaftStateMachine>,
    config_manager: RaftConfigManager,
    snapshot_manager: RaftSnapshotManager,

    // Channels for communication
    proposal_rx: mpsc::UnboundedReceiver<RaftProposal>,
    proposal_tx: ProposalChannel,
    message_rx: mpsc::UnboundedReceiver<(u64, raft::prelude::Message)>,
    _message_tx: MessageChannel,
    conf_change_rx: mpsc::UnboundedReceiver<RaftConfChange>,
    conf_change_tx: ConfChangeChannel,

    // Track pending proposals
    pending_proposals: PendingProposals,

    // Message sender for outgoing Raft messages
    outgoing_messages: MessageChannel,
}

impl RaftManager {
    /// Create a new RaftManager instance
    #[instrument(skip(database, shared_state), fields(node_id))]
    pub fn new(
        node_id: u64,
        database: Arc<Database>,
        peers: Vec<(u64, String)>,
        shared_state: Weak<crate::node_shared::SharedNodeState>,
    ) -> BlixardResult<(
        Self,
        ProposalChannel,
        MessageChannel,
        ConfChangeChannel,
        mpsc::UnboundedReceiver<(u64, raft::prelude::Message)>,
    )> {
        tracing::Span::current().record("node_id", node_id);

        // Create Raft configuration
        let cfg = Config {
            id: node_id,
            election_tick: 10,
            heartbeat_tick: 3,
            applied: 0,
            max_size_per_msg: 1024 * 1024,
            max_inflight_msgs: 256,
            ..Default::default()
        };

        // Initialize storage
        let storage = RedbRaftStorage {
            database: database.clone(),
        };

        // Create logger
        let logger = super::utils::create_raft_logger(node_id);

        // Create Raft node with a clone of storage
        let raft_node =
            RawNode::new(&cfg, storage.clone(), &logger).map_err(|e| BlixardError::Raft {
                operation: "create raft node".to_string(),
                source: Box::new(e),
            })?;

        // Create state machine
        let state_machine = Arc::new(RaftStateMachine::new(
            database.clone(),
            shared_state.clone(),
        ));

        // Create subcomponents
        let peers_map = Arc::new(RwLock::new(peers.into_iter().collect()));
        let config_manager = RaftConfigManager::new(
            node_id,
            database.clone(),
            peers_map.clone(),
            logger.clone(),
            shared_state.clone(),
        );
        let snapshot_manager = RaftSnapshotManager::new(database.clone(), logger.clone());

        // Create channels
        let (proposal_tx, proposal_rx) = mpsc::unbounded_channel();
        let (message_tx, message_rx) = mpsc::unbounded_channel();
        let (conf_change_tx, conf_change_rx) = mpsc::unbounded_channel();
        let (outgoing_tx, outgoing_rx) = mpsc::unbounded_channel();

        // Set up batch processing if enabled
        let final_proposal_tx =
            Self::setup_batch_processing(node_id, &shared_state, proposal_tx.clone())?;

        let manager = Self {
            node_id,
            raft_node: RwLock::new(raft_node),
            storage,
            logger,
            shared_state,
            state_machine,
            config_manager,
            snapshot_manager,
            proposal_rx,
            proposal_tx: final_proposal_tx.clone(),
            message_rx,
            _message_tx: message_tx.clone(),
            conf_change_rx,
            conf_change_tx: conf_change_tx.clone(),
            pending_proposals: Arc::new(RwLock::new(HashMap::new())),
            outgoing_messages: outgoing_tx,
        };

        Ok((
            manager,
            final_proposal_tx,
            message_tx,
            conf_change_tx,
            outgoing_rx,
        ))
    }

    /// Setup batch processing if enabled in configuration
    fn setup_batch_processing(
        node_id: u64,
        shared_state: &Weak<crate::node_shared::SharedNodeState>,
        proposal_tx: ProposalChannel,
    ) -> BlixardResult<ProposalChannel> {
        if let Some(_shared) = shared_state.upgrade() {
            let config = config_global::get().map_err(|e| BlixardError::Internal {
                message: format!("Failed to get config: {}", e),
            })?;
            let batch_config = crate::raft_batch_processor::BatchConfig {
                enabled: config.cluster.raft.batch_processing.enabled,
                max_batch_size: config.cluster.raft.batch_processing.max_batch_size,
                batch_timeout_ms: config.cluster.raft.batch_processing.batch_timeout_ms,
                max_batch_bytes: config.cluster.raft.batch_processing.max_batch_bytes,
            };

            if batch_config.enabled {
                // Create batch processor
                let (batch_tx, batch_processor) =
                    crate::raft_batch_processor::create_batch_processor(
                        batch_config,
                        proposal_tx.clone(),
                        node_id,
                    );

                // Spawn batch processor
                tokio::spawn(batch_processor.run());

                debug!("Enabled batch processing for node {}", node_id);
                return Ok(batch_tx);
            }
        }

        // Use direct channel if batch processing is disabled
        Ok(proposal_tx.clone())
    }

    /// Bootstrap a single-node cluster
    #[instrument(skip(self))]
    pub async fn bootstrap_single_node(&self) -> BlixardResult<()> {
        {
            let mut node = self.raft_node.write().await;

            // Campaign to become leader
            let _ = node.campaign();

            info!(self.logger, "[RAFT-BOOTSTRAP] After bootstrap campaign";
                "state" => ?node.raft.state,
                "commit" => node.raft.raft_log.committed,
                "applied" => node.raft.raft_log.applied
            );
        }

        // Process ready immediately to handle the bootstrap entry
        self.on_ready().await?;

        // Propose an empty entry to establish leadership
        {
            let mut node = self.raft_node.write().await;
            if node.raft.state == StateRole::Leader {
                info!(
                    self.logger,
                    "[RAFT-BOOTSTRAP] Proposing empty entry to establish leadership"
                );
                node.propose(vec![], vec![])?;
            }
        }

        // Process ready again to commit the empty entry
        self.on_ready().await?;

        Ok(())
    }

    /// Main run loop for the Raft manager
    #[instrument(skip(self))]
    pub async fn run(self) -> BlixardResult<()> {
        info!(self.logger, "[RAFT-RUN] Starting RaftManager::run() method");

        // Create bootstrap coordinator
        let bootstrap_coordinator = RaftBootstrapCoordinator::new(
            self.node_id,
            self.shared_state.clone(),
            self.logger.clone(),
        );

        // Execute bootstrap if needed
        let peers = self.config_manager.get_peers().await;
        if bootstrap_coordinator.should_bootstrap(&peers).await {
            match self.bootstrap_single_node().await {
                Ok(_) => info!(self.logger, "[RAFT-RUN] Bootstrap succeeded"),
                Err(e) => {
                    error!(self.logger, "[RAFT-RUN] Bootstrap failed, exiting run()"; "error" => %e);
                    return Err(e);
                }
            }
        }

        // Use the legacy run_main_loop approach
        // NOTE: Event loop refactoring is planned for future iteration to improve
        // modularity and testability
        info!(self.logger, "[RAFT-RUN] Using legacy event loop");
        self.run_main_loop().await
    }

    /// Static method to process on_ready state
    #[allow(dead_code)] // Experimental: future event loop architecture
    async fn process_on_ready(
        _raft_node: &Arc<RwLock<RawNode<RedbRaftStorage>>>,
        _snapshot_manager: &Arc<RaftSnapshotManager>,
        _state_machine: &Arc<RaftStateMachine>,
        _shared_state: &Weak<crate::node_shared::SharedNodeState>,
        _storage: &RedbRaftStorage,
        logger: &Logger,
    ) -> BlixardResult<()> {
        // This is a simplified version - in practice we'd need to move the full on_ready logic here
        // For now, return Ok to allow compilation
        info!(logger, "[RAFT-READY] Processing ready state (simplified)");
        Ok(())
    }

    /// Run with the new event loop architecture
    #[allow(dead_code)] // Experimental: future event loop architecture
    async fn run_with_event_loop(
        self,
        raft_node: Arc<RaftNodeHandle>,
        on_ready_fn: OnReadyFunction,
    ) -> BlixardResult<()> {
        info!(self.logger, "[RAFT-RUN] Starting event loop architecture");

        // Create handlers
        let (tick_handler, proposal_handler, message_handler, conf_change_handler) =
            HandlerFactory::create_handlers(
                raft_node,
                Arc::new(self.config_manager),
                Arc::new(self.snapshot_manager),
                self.pending_proposals.clone(),
                on_ready_fn,
                self.shared_state.clone(),
                self.logger.clone(),
            );

        // Create event loop
        let event_loop = EventLoopFactory::create_standard(
            self.proposal_rx,
            self.message_rx,
            self.conf_change_rx,
            tick_handler,
            proposal_handler,
            message_handler,
            conf_change_handler,
            self.logger.clone(),
        );

        // Run the event loop
        event_loop.run().await
    }

    /// Main event processing loop (legacy - will be removed)
    async fn run_main_loop(mut self) -> BlixardResult<()> {
        let mut tick_timer = interval(Duration::from_millis(100));
        let mut tick_count = 0u64;

        info!(self.logger, "[RAFT-RUN] Entering main run loop");

        loop {
            tokio::select! {
                _ = tick_timer.tick() => {
                    tick_count += 1;
                    if tick_count % 50 == 0 {  // Log every 5 seconds
                        info!(self.logger, "[RAFT-TICK] Tick #{}", tick_count);
                    }
                    if let Err(e) = self.tick().await {
                        error!(self.logger, "[RAFT-RUN] tick() failed, exiting run()"; "error" => %e);
                        return Err(e);
                    }
                }
                Some(proposal) = self.proposal_rx.recv() => {
                    info!(self.logger, "[RAFT-LOOP] Received proposal");
                    if let Err(e) = self.handle_proposal(proposal).await {
                        error!(self.logger, "[RAFT-RUN] handle_proposal() failed, exiting run()"; "error" => %e);
                        return Err(e);
                    }
                }
                Some((from, msg)) = self.message_rx.recv() => {
                    info!(self.logger, "[RAFT-LOOP] Received message from {}", from);
                    if let Err(e) = self.handle_raft_message(from, msg).await {
                        error!(self.logger, "[RAFT-RUN] handle_raft_message() failed, exiting run()"; "error" => %e);
                        return Err(e);
                    }
                }
                Some(conf_change) = self.conf_change_rx.recv() => {
                    info!(self.logger, "[RAFT-LOOP] Received configuration change");
                    if let Err(e) = self.handle_conf_change(conf_change).await {
                        error!(self.logger, "[RAFT-RUN] handle_conf_change() failed, exiting run()"; "error" => %e);
                        return Err(e);
                    }
                }
            }
        }
    }

    /// Check if this node should bootstrap as a single-node cluster
    #[allow(dead_code)] // Reserved for future bootstrap decision logic
    async fn should_bootstrap(&self) -> bool {
        let peers = self.config_manager.get_peers().await;
        let has_join_addr = self.has_join_address();

        // Only bootstrap if we have no peers AND no join address
        peers.is_empty() && !has_join_addr
    }

    /// Check if we have a join address configured
    #[allow(dead_code)] // Used by should_bootstrap method
    fn has_join_address(&self) -> bool {
        if let Some(shared) = self.shared_state.upgrade() {
            shared.config.join_addr.is_some()
        } else {
            false
        }
    }

    /// Handle Raft tick (periodic processing)
    async fn tick(&self) -> BlixardResult<()> {
        {
            let mut node = self.raft_node.write().await;
            node.tick();
        }

        // Process any ready state that results from the tick
        let processed_ready = self.on_ready().await?;

        // Check if we need to send snapshots to lagging followers
        if processed_ready {
            let mut node = self.raft_node.write().await;
            if node.raft.state == StateRole::Leader {
                self.snapshot_manager
                    .check_and_send_snapshots(&mut node)
                    .await?;
            }
        }

        Ok(())
    }

    /// Handle a proposal request
    #[instrument(skip(self, proposal), fields(proposal_id = %hex::encode(&proposal.id)))]
    async fn handle_proposal(&self, proposal: RaftProposal) -> BlixardResult<()> {
        #[cfg(feature = "observability")]
        let _timer = Timer::with_attributes(
            metrics().raft_proposal_duration.clone(),
            vec![attributes::operation("handle_proposal")],
        );

        info!(self.logger, "[RAFT-PROPOSAL] Handling proposal");

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
                        leader_id: if node.raft.leader_id == 0 {
                            None
                        } else {
                            Some(node.raft.leader_id)
                        },
                    }));
                }
                return Ok(());
            }

            node.propose(proposal.id, data)?;
        }

        // Process ready state to handle the proposal
        self.on_ready().await?;

        Ok(())
    }

    /// Handle a Raft message from another node
    #[instrument(skip(self, msg), fields(from, msg_type = ?msg.msg_type()))]
    async fn handle_raft_message(
        &self,
        from: u64,
        msg: raft::prelude::Message,
    ) -> BlixardResult<()> {
        #[cfg(feature = "observability")]
        let _timer = Timer::with_attributes(
            metrics().raft_proposal_duration.clone(),
            vec![attributes::operation("handle_message")],
        );

        tracing::Span::current().record("from", from);

        info!(self.logger, "[RAFT-MESSAGE] Handling message"; 
            "from" => from, "type" => ?msg.msg_type());

        {
            let mut node = self.raft_node.write().await;
            if let Err(e) = node.step(msg) {
                warn!(self.logger, "[RAFT-MESSAGE] Failed to step message"; "error" => %e);
                return Err(BlixardError::Raft {
                    operation: "step message".to_string(),
                    source: Box::new(e),
                });
            }
        }

        // Process ready state after stepping the message
        self.on_ready().await?;

        Ok(())
    }

    /// Handle a configuration change request
    async fn handle_conf_change(&self, conf_change: RaftConfChange) -> BlixardResult<()> {
        let mut node = self.raft_node.write().await;
        self.config_manager
            .handle_conf_change(conf_change, &mut node)
            .await
    }

    /// Process ready state and handle all resulting operations
    #[instrument(skip(self))]
    async fn on_ready(&self) -> BlixardResult<bool> {
        let mut node = self.raft_node.write().await;

        // Check if there's a ready state to process
        if !node.has_ready() {
            return Ok(false);
        }

        info!(self.logger, "[RAFT-READY] Processing ready state");
        let mut ready = node.ready();

        // Log ready state details for debugging
        self.log_ready_state_details(&ready, &node);

        // Process snapshot if present
        self.snapshot_manager
            .process_snapshot_if_present(&ready, &self.storage)
            .await?;

        // Save entries to storage
        self.save_entries_to_storage(&ready, &mut node).await?;

        // Save hard state if it changed
        if let Some(hs) = ready.hs() {
            info!(self.logger, "[RAFT-READY] Saving hard state"; 
                "term" => hs.term, "vote" => hs.vote, "commit" => hs.commit);
            node.mut_store().save_hard_state(&hs)?;
        }

        // Send messages to peers
        self.send_messages_to_peers(&mut ready).await?;

        // Process committed entries
        let last_applied_index = self
            .process_committed_entries(&mut ready, &mut node)
            .await?;

        // Advance the Raft node
        self.advance_raft_node(&mut node, ready, last_applied_index)
            .await?;

        // Update applied index
        if last_applied_index > 0 {
            node.advance_apply_to(last_applied_index);
            info!(
                self.logger,
                "[RAFT-READY] Advanced applied index to {}", last_applied_index
            );
        }

        // Check and trigger log compaction if needed
        self.snapshot_manager
            .check_and_trigger_log_compaction(&mut node, &self.storage)
            .await?;

        // Update SharedNodeState with current Raft status
        self.update_shared_state_status(&node).await;

        // Return true to indicate we processed a ready state
        Ok(true)
    }

    /// Save entries to storage
    async fn save_entries_to_storage(
        &self,
        ready: &Ready,
        node: &mut RawNode<RedbRaftStorage>,
    ) -> BlixardResult<()> {
        if !ready.entries().is_empty() {
            info!(
                self.logger,
                "[RAFT-READY] Saving {} entries to storage",
                ready.entries().len()
            );

            // Log detailed entry information
            for (i, entry) in ready.entries().iter().enumerate() {
                info!(self.logger, "[RAFT-READY] Entry details";
                    "entry_num" => i,
                    "index" => entry.index,
                    "term" => entry.term,
                    "entry_type" => ?entry.entry_type(),
                    "context_len" => entry.context.len(),
                    "data_len" => entry.data.len()
                );
            }

            node.mut_store().append(&ready.entries())?;

            // In a single-node cluster, we might need to update the commit index immediately
            let peers = self.config_manager.get_peers().await;
            if peers.is_empty() && node.raft.state == StateRole::Leader {
                // Single node cluster - can commit immediately
                let last_index = ready.entries().last().map(|e| e.index).unwrap_or(0);
                info!(self.logger, "[RAFT-READY] Single node cluster, checking commit"; "last_index" => last_index);
            }
        }
        Ok(())
    }

    /// Send messages to peers
    async fn send_messages_to_peers(&self, ready: &mut Ready) -> BlixardResult<()> {
        if !ready.messages().is_empty() {
            info!(
                self.logger,
                "[RAFT-READY] Sending {} messages",
                ready.messages().len()
            );
            for msg in ready.take_messages() {
                // Add more detailed logging for AppendEntries
                if msg.msg_type() == raft::prelude::MessageType::MsgAppend {
                    info!(self.logger, "[RAFT-MSG] Sending MsgAppend";
                        "to" => msg.to,
                        "from" => msg.from,
                        "index" => msg.index,
                        "log_term" => msg.log_term,
                        "entries_count" => msg.entries.len(),
                        "commit" => msg.commit
                    );
                }
                self.send_raft_message(msg).await?;
            }
        }
        Ok(())
    }

    /// Process committed entries from ready state
    async fn process_committed_entries(
        &self,
        ready: &mut Ready,
        node: &mut RawNode<RedbRaftStorage>,
    ) -> BlixardResult<u64> {
        let committed_entries = ready.take_committed_entries();
        info!(
            self.logger,
            "[RAFT-READY] Committed entries count: {}",
            committed_entries.len()
        );

        // Check if we're behind on applying entries
        if committed_entries.is_empty() && node.raft.raft_log.committed > node.raft.raft_log.applied
        {
            warn!(self.logger, "[RAFT-READY] No committed entries but we're behind on applying";
                "committed" => node.raft.raft_log.committed,
                "applied" => node.raft.raft_log.applied
            );
        }

        let mut last_applied_index = 0u64;
        if !committed_entries.is_empty() {
            info!(
                self.logger,
                "[RAFT-READY] Processing {} committed entries",
                committed_entries.len()
            );

            for (idx, entry) in committed_entries.iter().enumerate() {
                info!(self.logger, "[RAFT-READY] Processing committed entry";
                    "entry_num" => idx,
                    "index" => entry.index,
                    "term" => entry.term,
                    "type" => ?entry.entry_type(),
                    "context_len" => entry.context.len(),
                    "data_len" => entry.data.len()
                );

                last_applied_index = entry.index;

                match entry.entry_type() {
                    raft::prelude::EntryType::EntryNormal => {
                        self.process_normal_entry(entry).await?;
                    }
                    raft::prelude::EntryType::EntryConfChange => {
                        self.process_conf_change_entry(entry, node).await?;
                    }
                    _ => {
                        warn!(self.logger, "[RAFT-READY] Unknown entry type"; "type" => ?entry.entry_type());
                    }
                }
            }
        }

        Ok(last_applied_index)
    }

    /// Process a normal entry (non-configuration change)
    async fn process_normal_entry(&self, entry: &Entry) -> BlixardResult<()> {
        info!(self.logger, "[RAFT-READY] Processing normal entry";
            "index" => entry.index,
            "context_len" => entry.context.len(),
            "has_data" => !entry.data.is_empty()
        );

        // Apply normal entry to state machine
        let result = self.state_machine.apply_entry(&entry).await;

        // Send response to waiting proposal if any
        if !entry.context.is_empty() {
            let pending_proposals = Arc::clone(&self.pending_proposals);
            let mut pending = pending_proposals.write().await;
            if let Some(response_tx) = pending.remove(&entry.context) {
                info!(self.logger, "[RAFT-READY] Sending proposal response"; "success" => result.is_ok());
                if let Err(_) = response_tx.send(result) {
                    warn!(
                        self.logger,
                        "[RAFT-READY] Failed to send proposal response - receiver dropped"
                    );
                }
            } else {
                warn!(self.logger, "[RAFT-READY] No pending proposal found for context";
                    "context_len" => entry.context.len()
                );
            }
        }

        Ok(())
    }

    /// Process a configuration change entry
    async fn process_conf_change_entry(
        &self,
        entry: &Entry,
        node: &mut RawNode<RedbRaftStorage>,
    ) -> BlixardResult<()> {
        use protobuf::Message as ProtobufMessage;
        use raft::prelude::ConfChange;

        info!(self.logger, "[RAFT-CONF] Processing EntryConfChange entry";
            "entry_index" => entry.index,
            "entry_data_len" => entry.data.len()
        );

        let mut cc = ConfChange::default();
        if let Err(e) = cc.merge_from_bytes(&entry.data) {
            error!(self.logger, "[RAFT-CONF] Failed to parse ConfChange"; "error" => %e);

            // Send error response if we have a pending proposal
            if !entry.context.is_empty() {
                let mut pending = self.pending_proposals.write().await;
                if let Some(response_tx) = pending.remove(&entry.context) {
                    let _ = response_tx.send(Err(BlixardError::Internal {
                        message: format!("Failed to parse ConfChange: {:?}", e),
                    }));
                }
            }
            return Err(BlixardError::Internal {
                message: format!("Failed to parse ConfChange: {:?}", e),
            });
        }

        // Apply the configuration change
        self.config_manager
            .apply_conf_change(&cc, entry, node)
            .await?;

        Ok(())
    }

    /// Advance the Raft node and handle light ready
    async fn advance_raft_node(
        &self,
        node: &mut RawNode<RedbRaftStorage>,
        ready: Ready,
        last_applied_index: u64,
    ) -> BlixardResult<()> {
        info!(self.logger, "[RAFT-READY] About to call advance on ready state";
            "applied_before" => node.raft.raft_log.applied,
            "committed_before" => node.raft.raft_log.committed
        );
        let mut light_rd = node.advance(ready);
        info!(self.logger, "[RAFT-READY] Advanced ready state";
            "applied_after" => node.raft.raft_log.applied,
            "committed_after" => node.raft.raft_log.committed
        );

        // Handle messages from LightReady
        if !light_rd.messages().is_empty() {
            info!(
                self.logger,
                "[RAFT-READY] Sending {} messages from LightReady",
                light_rd.messages().len()
            );
            for msg in light_rd.take_messages() {
                self.send_raft_message(msg).await?;
            }
        }

        // Update commit index if needed
        let had_commit_update = if let Some(commit) = light_rd.commit_index() {
            info!(self.logger, "[RAFT-READY] Light ready has commit index"; "commit" => commit);
            // Store commit index update
            let mut hs = node.raft.hard_state();
            hs.set_commit(commit);
            node.mut_store().save_hard_state(&hs)?;
            true
        } else {
            false
        };

        // CRITICAL: Check if new entries were committed during this ready cycle
        self.handle_newly_committed_entries(node, last_applied_index, had_commit_update)
            .await?;

        Ok(())
    }

    /// Handle newly committed entries that weren't in the original ready state
    async fn handle_newly_committed_entries(
        &self,
        node: &mut RawNode<RedbRaftStorage>,
        mut last_applied_index: u64,
        had_commit_update: bool,
    ) -> BlixardResult<()> {
        let current_applied = if last_applied_index > 0 {
            last_applied_index
        } else {
            node.raft.raft_log.applied
        };

        if had_commit_update && node.raft.raft_log.committed > current_applied {
            // We have newly committed entries that weren't in ready.committed_entries()
            // Fetch and process them now
            let start = current_applied + 1;
            let end = node.raft.raft_log.committed + 1;

            info!(self.logger, "[RAFT-READY] Fetching newly committed entries after light ready";
                "start" => start,
                "end" => end,
                "last_applied" => last_applied_index,
                "committed" => node.raft.raft_log.committed
            );

            if let Ok(entries) =
                node.store()
                    .entries(start, end, None, GetEntriesContext::empty(false))
            {
                info!(
                    self.logger,
                    "[RAFT-READY] Processing {} newly committed entries",
                    entries.len()
                );

                // Process these entries
                for entry in entries.iter() {
                    last_applied_index = entry.index;
                    self.process_normal_entry(entry).await?;
                }

                // Update applied index for the newly processed entries
                if last_applied_index > 0 {
                    node.advance_apply_to(last_applied_index);
                    info!(
                        self.logger,
                        "[RAFT-READY] Advanced applied index to {}", last_applied_index
                    );
                }
            }
        }

        Ok(())
    }

    /// Send a Raft message to another node
    async fn send_raft_message(&self, msg: raft::prelude::Message) -> BlixardResult<()> {
        let to = msg.to;
        let from = msg.from;
        let msg_type = msg.msg_type();

        debug!(
            "Sending Raft message: to={}, from={}, type={:?}",
            to, from, msg_type
        );

        // Send through the outgoing channel for the transport layer to handle
        if let Err(_) = self.outgoing_messages.send((to, msg)) {
            warn!(self.logger, "[RAFT-MESSAGE] Failed to send message - channel closed"; 
                "to" => to, "type" => ?msg_type);
        }

        Ok(())
    }

    /// Update SharedNodeState with current Raft status
    async fn update_shared_state_status(&self, node: &RawNode<RedbRaftStorage>) {
        info!(self.logger, "[RAFT-READY] After processing - raft state";
            "term" => node.raft.term,
            "commit" => node.raft.raft_log.committed,
            "applied" => node.raft.raft_log.applied,
            "last_index" => node.raft.raft_log.last_index()
        );

        // Update SharedNodeState with current Raft status
        if let Some(shared) = self.shared_state.upgrade() {
            let is_leader = node.raft.state == StateRole::Leader;
            let leader_id = if node.raft.leader_id == 0 {
                None
            } else {
                Some(node.raft.leader_id)
            };

            let status = crate::node_shared::RaftStatus {
                is_leader,
                node_id: self.node_id,
                leader_id,
                term: node.raft.term,
                state: format!("{:?}", node.raft.state).to_lowercase(),
            };

            shared.update_raft_status(status);
        }
    }

    /// Log ready state details for debugging
    fn log_ready_state_details(&self, ready: &Ready, node: &RawNode<RedbRaftStorage>) {
        // Debug messages in ready state
        for msg in ready.messages() {
            info!(self.logger, "[RAFT-READY] Has message in ready"; 
                "from" => msg.from, "to" => msg.to, "type" => ?msg.msg_type());
        }

        // Log the current state of the raft log BEFORE processing
        info!(self.logger, "[RAFT-READY] Raft log state BEFORE processing";
            "applied" => node.raft.raft_log.applied,
            "committed" => node.raft.raft_log.committed,
            "last_index" => node.raft.raft_log.last_index()
        );

        info!(self.logger, "[RAFT-READY] Ready state details";
            "messages" => ready.messages().len(),
            "committed_entries" => ready.committed_entries().len(),
            "entries" => ready.entries().len(),
            "has_snapshot" => !ready.snapshot().is_empty()
        );

        // Debug: Check if there are committed entries before processing
        if !ready.committed_entries().is_empty() {
            info!(
                self.logger,
                "[RAFT-READY] Found {} committed entries in ready state",
                ready.committed_entries().len()
            );
            for (i, entry) in ready.committed_entries().iter().enumerate() {
                info!(self.logger, "[RAFT-READY] Committed entry {}: index={}, type={:?}, context_len={}, data_len={}", 
                    i, entry.index, entry.entry_type(), entry.context.len(), entry.data.len());
            }
        } else {
            // If no committed entries but we're behind, something is wrong
            if node.raft.raft_log.committed > node.raft.raft_log.applied {
                warn!(self.logger, "[RAFT-READY] No committed entries but behind on applied!";
                    "committed" => node.raft.raft_log.committed,
                    "applied" => node.raft.raft_log.applied
                );
            }
        }
    }

    // Public API methods

    /// Get the proposal sender channel
    pub fn proposal_sender(&self) -> ProposalChannel {
        self.proposal_tx.clone()
    }

    /// Get the configuration change sender channel
    pub fn conf_change_sender(&self) -> ConfChangeChannel {
        self.conf_change_tx.clone()
    }

    /// Report snapshot status to Raft
    pub async fn report_snapshot(
        &self,
        to: u64,
        status: raft::SnapshotStatus,
    ) -> BlixardResult<()> {
        let mut node = self.raft_node.write().await;
        self.snapshot_manager
            .report_snapshot(&mut node, to, status)
            .await
    }

    /// Configure the state machine's admission controller
    pub async fn configure_admission_controller(
        &self,
        _config: crate::resource_admission::AdmissionControlConfig,
    ) {
        // This is a bit awkward since state_machine is Arc'd, but we need mutable access
        // In practice, this should be called during initialization before concurrent access
        // NOTE: This requires adding interior mutability to RaftStateMachine (planned enhancement)
        warn!(
            self.logger,
            "configure_admission_controller called - needs interior mutability implementation"
        );
        // Implementation requires refactoring RaftStateMachine to use interior mutability
    }
}
