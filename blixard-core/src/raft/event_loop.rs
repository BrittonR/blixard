//! Raft event loop management and coordination
//!
//! This module provides the main event loop coordination for processing
//! Raft events including ticks, proposals, messages, and configuration changes.

use crate::error::BlixardResult;

use super::messages::{RaftConfChange, RaftProposal};

use async_trait::async_trait;
use slog::{error, info, Logger};
use tokio::sync::mpsc;
use tokio::time::{interval, Duration, Interval};
use tracing::instrument;

/// Configuration for the event loop
#[derive(Debug, Clone)]
pub struct EventLoopConfig {
    /// Tick interval in milliseconds
    pub tick_interval_ms: u64,
    /// How often to log tick progress (every N ticks)
    pub tick_log_interval: u64,
}

impl Default for EventLoopConfig {
    fn default() -> Self {
        Self {
            tick_interval_ms: 100,
            tick_log_interval: 50, // Log every 5 seconds with 100ms ticks
        }
    }
}

/// Coordinates the main Raft event processing loop
pub struct RaftEventLoop {
    config: EventLoopConfig,
    event_dispatcher: RaftEventDispatcher,
    logger: Logger,
}

impl RaftEventLoop {
    /// Create a new event loop coordinator
    pub fn new(
        config: EventLoopConfig,
        event_dispatcher: RaftEventDispatcher,
        logger: Logger,
    ) -> Self {
        Self {
            config,
            event_dispatcher,
            logger,
        }
    }

    /// Run the main event loop
    #[instrument(skip(self), fields(tick_interval_ms = self.config.tick_interval_ms))]
    pub async fn run(mut self) -> BlixardResult<()> {
        let mut tick_timer = interval(Duration::from_millis(self.config.tick_interval_ms));
        let mut tick_count = 0u64;
        
        info!(self.logger, "[EVENT-LOOP] Starting main event loop");
        
        loop {
            match self.process_next_event(&mut tick_timer, &mut tick_count).await {
                Ok(()) => continue,
                Err(e) => {
                    error!(self.logger, "[EVENT-LOOP] Event processing failed, exiting"; "error" => %e);
                    return Err(e);
                }
            }
        }
    }

    /// Process the next event from any channel
    async fn process_next_event(
        &mut self,
        tick_timer: &mut Interval,
        tick_count: &mut u64,
    ) -> BlixardResult<()> {
        tokio::select! {
            _ = tick_timer.tick() => {
                self.handle_tick(tick_count).await
            }
            Some(proposal) = self.event_dispatcher.proposal_rx.recv() => {
                self.event_dispatcher.handle_proposal(proposal).await
            }
            Some((from, msg)) = self.event_dispatcher.message_rx.recv() => {
                self.event_dispatcher.handle_raft_message(from, msg).await
            }
            Some(conf_change) = self.event_dispatcher.conf_change_rx.recv() => {
                self.event_dispatcher.handle_conf_change(conf_change).await
            }
        }
    }

    /// Handle periodic tick events
    async fn handle_tick(&self, tick_count: &mut u64) -> BlixardResult<()> {
        *tick_count += 1;
        
        if *tick_count % self.config.tick_log_interval == 0 {
            info!(self.logger, "[TICK] Tick #{}", tick_count);
        }
        
        self.event_dispatcher.handle_tick().await
    }
}

/// Dispatches Raft events to appropriate handlers
pub struct RaftEventDispatcher {
    // Channels for receiving events
    pub proposal_rx: mpsc::UnboundedReceiver<RaftProposal>,
    pub message_rx: mpsc::UnboundedReceiver<(u64, raft::prelude::Message)>,
    pub conf_change_rx: mpsc::UnboundedReceiver<RaftConfChange>,
    
    // Event handlers (function pointers or trait objects)
    tick_handler: Box<dyn TickHandler + Send + Sync>,
    proposal_handler: Box<dyn ProposalHandler + Send + Sync>,
    message_handler: Box<dyn MessageHandler + Send + Sync>,
    conf_change_handler: Box<dyn ConfChangeHandler + Send + Sync>,
    
    logger: Logger,
}

impl RaftEventDispatcher {
    /// Create a new event dispatcher
    pub fn new(
        proposal_rx: mpsc::UnboundedReceiver<RaftProposal>,
        message_rx: mpsc::UnboundedReceiver<(u64, raft::prelude::Message)>,
        conf_change_rx: mpsc::UnboundedReceiver<RaftConfChange>,
        tick_handler: Box<dyn TickHandler + Send + Sync>,
        proposal_handler: Box<dyn ProposalHandler + Send + Sync>,
        message_handler: Box<dyn MessageHandler + Send + Sync>,
        conf_change_handler: Box<dyn ConfChangeHandler + Send + Sync>,
        logger: Logger,
    ) -> Self {
        Self {
            proposal_rx,
            message_rx,
            conf_change_rx,
            tick_handler,
            proposal_handler,
            message_handler,
            conf_change_handler,
            logger,
        }
    }

    /// Handle a tick event
    async fn handle_tick(&self) -> BlixardResult<()> {
        self.tick_handler.handle_tick().await
    }

    /// Handle a proposal event
    async fn handle_proposal(&self, proposal: RaftProposal) -> BlixardResult<()> {
        info!(self.logger, "[EVENT-DISPATCHER] Handling proposal");
        self.proposal_handler.handle_proposal(proposal).await
    }

    /// Handle a Raft message event
    async fn handle_raft_message(&self, from: u64, msg: raft::prelude::Message) -> BlixardResult<()> {
        info!(self.logger, "[EVENT-DISPATCHER] Handling message from {}", from);
        self.message_handler.handle_message(from, msg).await
    }

    /// Handle a configuration change event
    async fn handle_conf_change(&self, conf_change: RaftConfChange) -> BlixardResult<()> {
        info!(self.logger, "[EVENT-DISPATCHER] Handling configuration change");
        self.conf_change_handler.handle_conf_change(conf_change).await
    }
}

// Handler traits for different event types

/// Handler for tick events
#[async_trait]
pub trait TickHandler {
    async fn handle_tick(&self) -> BlixardResult<()>;
}

/// Handler for proposal events  
#[async_trait]
pub trait ProposalHandler {
    async fn handle_proposal(&self, proposal: RaftProposal) -> BlixardResult<()>;
}

/// Handler for Raft message events
#[async_trait]
pub trait MessageHandler {
    async fn handle_message(&self, from: u64, msg: raft::prelude::Message) -> BlixardResult<()>;
}

/// Handler for configuration change events
#[async_trait]
pub trait ConfChangeHandler {
    async fn handle_conf_change(&self, conf_change: RaftConfChange) -> BlixardResult<()>;
}

/// Factory for creating event loop components
pub struct EventLoopFactory;

impl EventLoopFactory {
    /// Create a new event loop with standard configuration
    pub fn create_standard(
        proposal_rx: mpsc::UnboundedReceiver<RaftProposal>,
        message_rx: mpsc::UnboundedReceiver<(u64, raft::prelude::Message)>,
        conf_change_rx: mpsc::UnboundedReceiver<RaftConfChange>,
        tick_handler: Box<dyn TickHandler + Send + Sync>,
        proposal_handler: Box<dyn ProposalHandler + Send + Sync>,
        message_handler: Box<dyn MessageHandler + Send + Sync>,
        conf_change_handler: Box<dyn ConfChangeHandler + Send + Sync>,
        logger: Logger,
    ) -> RaftEventLoop {
        let config = EventLoopConfig::default();
        let dispatcher = RaftEventDispatcher::new(
            proposal_rx,
            message_rx,
            conf_change_rx,
            tick_handler,
            proposal_handler,
            message_handler,
            conf_change_handler,
            logger.clone(),
        );
        
        RaftEventLoop::new(config, dispatcher, logger)
    }

    /// Create a new event loop with custom configuration
    pub fn create_custom(
        config: EventLoopConfig,
        proposal_rx: mpsc::UnboundedReceiver<RaftProposal>,
        message_rx: mpsc::UnboundedReceiver<(u64, raft::prelude::Message)>,
        conf_change_rx: mpsc::UnboundedReceiver<RaftConfChange>,
        tick_handler: Box<dyn TickHandler + Send + Sync>,
        proposal_handler: Box<dyn ProposalHandler + Send + Sync>,
        message_handler: Box<dyn MessageHandler + Send + Sync>,
        conf_change_handler: Box<dyn ConfChangeHandler + Send + Sync>,
        logger: Logger,
    ) -> RaftEventLoop {
        let dispatcher = RaftEventDispatcher::new(
            proposal_rx,
            message_rx,
            conf_change_rx,
            tick_handler,
            proposal_handler,
            message_handler,
            conf_change_handler,
            logger.clone(),
        );
        
        RaftEventLoop::new(config, dispatcher, logger)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    // Mock implementations for testing
    struct MockTickHandler;
    struct MockProposalHandler;
    struct MockMessageHandler;
    struct MockConfChangeHandler;

    #[async_trait]
    impl TickHandler for MockTickHandler {
        async fn handle_tick(&self) -> BlixardResult<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl ProposalHandler for MockProposalHandler {
        async fn handle_proposal(&self, _proposal: RaftProposal) -> BlixardResult<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl MessageHandler for MockMessageHandler {
        async fn handle_message(&self, _from: u64, _msg: raft::prelude::Message) -> BlixardResult<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl ConfChangeHandler for MockConfChangeHandler {
        async fn handle_conf_change(&self, _conf_change: RaftConfChange) -> BlixardResult<()> {
            Ok(())
        }
    }

    #[test]
    fn test_event_loop_config_default() {
        let config = EventLoopConfig::default();
        assert_eq!(config.tick_interval_ms, 100);
        assert_eq!(config.tick_log_interval, 50);
    }

    #[tokio::test]
    async fn test_event_loop_factory() {
        let logger = super::super::utils::create_raft_logger(1);
        let (_proposal_tx, proposal_rx) = mpsc::unbounded_channel();
        let (_message_tx, message_rx) = mpsc::unbounded_channel();
        let (_conf_change_tx, conf_change_rx) = mpsc::unbounded_channel();

        let _event_loop = EventLoopFactory::create_standard(
            proposal_rx,
            message_rx,
            conf_change_rx,
            Box::new(MockTickHandler),
            Box::new(MockProposalHandler),
            Box::new(MockMessageHandler),
            Box::new(MockConfChangeHandler),
            logger,
        );
        
        // Test that factory creates event loop without panicking
    }
}