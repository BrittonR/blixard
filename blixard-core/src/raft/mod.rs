//! Raft consensus implementation
//!
//! This module provides the distributed consensus layer for Blixard using the Raft algorithm.
//! It's organized into focused submodules for better maintainability:
//!
//! - `core` - Core RaftManager and event loop
//! - `state_machine` - State machine implementation for applying proposals
//! - `proposals` - Proposal types and handling logic
//! - `config_manager` - Configuration changes and membership management
//! - `snapshot` - Snapshot creation and restoration
//! - `messages` - Message types and routing

pub mod core;
pub mod state_machine;
pub mod proposals;
pub mod config_manager;
pub mod snapshot;
pub mod messages;
pub mod bootstrap;
pub mod event_loop;
pub mod handlers;

// Re-export commonly used types
pub use self::core::RaftManager;
pub use self::messages::{RaftMessage, RaftConfChange, ConfChangeType, RaftProposal};
pub use self::proposals::ProposalData;

// Internal utilities shared across modules
pub(crate) mod utils {
    use slog::{o, Logger, Drain};
    
    /// Create a Raft logger with consistent formatting
    pub fn create_raft_logger(node_id: u64) -> Logger {
        let decorator = slog_term::TermDecorator::new().build();
        let drain = slog_term::FullFormat::new(decorator).build().fuse();
        let drain = slog_async::Async::new(drain).build().fuse();
        slog::Logger::root(drain, o!("node_id" => node_id, "module" => "raft"))
    }
    
    /// Generate a unique proposal ID
    pub fn generate_proposal_id() -> Vec<u8> {
        use std::time::{SystemTime, UNIX_EPOCH};
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_nanos()
            .hash(&mut hasher);
        std::process::id().hash(&mut hasher);
        std::thread::current().id().hash(&mut hasher);
        
        hasher.finish().to_le_bytes().to_vec()
    }
}