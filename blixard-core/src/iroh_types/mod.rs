//! Native Rust types for Iroh transport, modularized for better maintainability
//!
//! This module contains all the request/response types and data structures
//! previously defined in proto files, now as native Rust types organized
//! into focused submodules.

// Re-export all types for backward compatibility
pub use common::*;
pub use cluster::*;
pub use vm::*;
pub use vm_advanced::*;
pub use task::*;
pub use health::*;
pub use ip_pool::*;
pub use p2p::*;
pub use bootstrap::*;
pub use raft::*;
pub use security::*;
pub use conversions::*;

pub mod common;
pub mod cluster;
pub mod vm;
pub mod vm_advanced;
pub mod task;
pub mod health;
pub mod ip_pool;
pub mod p2p;
pub mod bootstrap;
pub mod raft;
pub mod security;
pub mod conversions;