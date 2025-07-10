//! Trait abstractions for breaking circular dependencies
//!
//! This module contains trait definitions that allow components to depend on
//! interfaces rather than concrete implementations, breaking circular dependencies
//! and improving testability.

pub mod command;
pub mod config;
pub mod container;
pub mod filesystem;
pub mod node_events;
pub mod raft_coordinator;
pub mod state_manager;
pub mod storage;
pub mod time;

pub use command::*;
pub use config::*;
pub use container::*;
pub use filesystem::*;
pub use node_events::*;
pub use raft_coordinator::*;
pub use state_manager::*;
pub use storage::*;
pub use time::*;