//! Trait abstractions for breaking circular dependencies
//!
//! This module contains trait definitions that allow components to depend on
//! interfaces rather than concrete implementations, breaking circular dependencies
//! and improving testability.

pub mod node_events;
pub mod raft_coordinator;
pub mod state_manager;

pub use node_events::*;
pub use raft_coordinator::*;
pub use state_manager::*;