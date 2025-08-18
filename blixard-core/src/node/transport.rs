use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::error::{BlixardError, BlixardResult};
use crate::raft::messages::RaftProposal;
use crate::raft_manager::{RaftConfChange, RaftManager};

use super::core::Node;

impl Node {
    /// Set up transport infrastructure (Raft transport and peer connector)
    pub(super) async fn setup_transport(
        &mut self,
        message_tx: mpsc::UnboundedSender<(u64, raft::prelude::Message)>,
        _conf_change_tx: mpsc::UnboundedSender<RaftConfChange>,
        proposal_tx: mpsc::UnboundedSender<RaftProposal>,
    ) -> BlixardResult<Arc<crate::transport::raft_transport_adapter::RaftTransport>> {
        // Get transport configuration - using default TransportConfig (Iroh-only now)
        let transport_config = crate::transport::config::TransportConfig::default();

        // Create transport
        let raft_transport = Arc::new(
            crate::transport::raft_transport_adapter::RaftTransport::new(
                self.shared.clone(),
                message_tx.clone(),
                &transport_config,
            )
            .await?,
        );

        // Store transport for peer management
        // Note: Transport stored implicitly through peer connector and endpoints

        // Get the endpoint from the transport and create peer connector
        let endpoint = raft_transport.endpoint().as_ref().clone();
        let p2p_monitor = Arc::new(crate::p2p_monitor::NoOpMonitor);

        let peer_connector = Arc::new(
            crate::transport::iroh_peer_connector::IrohPeerConnector::new(
                endpoint,
                self.shared.clone(),
                p2p_monitor,
            ),
        );

        let connector_clone = peer_connector.clone();
        tokio::spawn(async move {
            if let Err(e) = connector_clone.start().await {
                tracing::error!("Peer connector error: {}", e);
            }
        });

        self.shared.set_peer_connector(peer_connector);

        // Set individual Raft channels
        self.shared.set_raft_proposal_tx(proposal_tx);
        self.shared.set_raft_message_tx(message_tx.clone());

        // Initialize discovery if configured
        self.setup_discovery_if_configured().await?;

        Ok(raft_transport)
    }

    /// Set up discovery manager if configured
    pub(super) async fn setup_discovery_if_configured(&mut self) -> BlixardResult<()> {
        // Discovery is now handled by Iroh's built-in discovery
        // No need for separate discovery manager
        Ok(())
    }

    /// Spawn Raft message handler for outgoing messages
    pub(super) fn spawn_raft_message_handler(
        &self,
        transport: Arc<crate::transport::raft_transport_adapter::RaftTransport>,
        mut outgoing_rx: mpsc::UnboundedReceiver<(u64, raft::prelude::Message)>,
    ) {
        let shared_weak = Arc::downgrade(&self.shared);

        tracing::info!("Starting Raft message handler for outgoing messages");
        
        tokio::spawn(async move {
            tracing::info!("Raft message handler task started, waiting for messages...");
            loop {
                match outgoing_rx.recv().await {
                    Some((target, msg)) => {
                        tracing::info!("Raft message handler received message for target {}", target);
                        if let Some(_shared) = shared_weak.upgrade() {
                            let result = transport.send_message(target, msg).await;

                            if let Err(e) = result {
                                match &e {
                                    BlixardError::NodeNotFound { .. } => {
                                        // Node removed from cluster, this is expected
                                        tracing::debug!(
                                            "Cannot send message to removed node {}: {}",
                                            target,
                                            e
                                        );
                                    }
                                    _ => {
                                        tracing::warn!("Failed to send Raft message to {}: {}", target, e);
                                        // Could implement retry logic here
                                    }
                                }
                            }
                        } else {
                            // Node is shutting down
                            tracing::info!("Node shutting down, stopping message handler");
                            break;
                        }
                    }
                    None => {
                        tracing::warn!("Raft message channel closed, handler exiting");
                        break;
                    }
                }
            }
            tracing::info!("Raft message handler stopped");
        });
    }

    /// Spawn Raft manager with automatic recovery
    pub(super) fn spawn_raft_manager_with_recovery(
        &self,
        raft_manager: RaftManager,
    ) -> tokio::task::JoinHandle<BlixardResult<()>> {
        let shared_weak = Arc::downgrade(&self.shared);
        let node_id = self.shared.get_id();
        let shared_for_db = self.shared.clone();

        tokio::spawn(async move {
            let config = match crate::config_global::get() {
                Ok(cfg) => cfg,
                Err(e) => {
                    tracing::error!("Failed to get config for Raft restart: {}", e);
                    return Err(e);
                }
            };
            let max_restarts = config.cluster.raft.max_restarts;
            let restart_delay_ms = config.cluster.raft.restart_delay.as_millis() as u64;

            // Run the initial Raft manager
            let mut restart_count = match raft_manager.run().await {
                Ok(_) => {
                    tracing::info!("Raft manager exited normally");
                    return Ok(());
                }
                Err(e) => {
                    tracing::error!("Raft manager crashed: {}", e);
                    1
                }
            };

            // Recovery loop
            while restart_count <= max_restarts {
                if let Some(shared) = shared_weak.upgrade() {
                    if !shared.is_running() {
                        tracing::info!("Node is stopping, not restarting Raft manager");
                        return Ok(());
                    }

                    // Exponential backoff: 2^(restart_count - 1) * base delay
                    let delay = restart_delay_ms * (1 << (restart_count - 1).min(5));
                    tracing::warn!(
                        "Restarting Raft manager (attempt {}/{}) after {} ms",
                        restart_count,
                        max_restarts,
                        delay
                    );
                    tokio::time::sleep(Duration::from_millis(delay)).await;

                    // Recreate storage and Raft manager
                    let db = shared_for_db.get_database().await;
                    match Self::recreate_raft_manager(node_id, db.clone(), shared.clone()).await {
                        Ok(new_manager) => {
                            tracing::info!("Successfully recreated Raft manager");
                            match new_manager.run().await {
                                Ok(_) => {
                                    tracing::info!("Restarted Raft manager exited normally");
                                    return Ok(());
                                }
                                Err(e) => {
                                    tracing::error!("Restarted Raft manager crashed: {}", e);
                                    restart_count += 1;
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to recreate Raft manager: {}", e);
                            restart_count += 1;
                        }
                    }
                } else {
                    // Node has been dropped
                    break;
                }
            }

            let error = BlixardError::Internal {
                message: format!(
                    "Raft manager failed after {} restart attempts",
                    max_restarts
                ),
            };
            tracing::error!("{}", error);
            Err(error)
        })
    }
}