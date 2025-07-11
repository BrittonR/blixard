//! Enhanced test helpers for integration tests
//!
//! This module provides utilities for setting up full nodes with Raft for testing,
//! including higher-level abstractions for cluster management, automatic port allocation,
//! and better wait conditions.
//!
//! ## Module Organization
//! 
//! This module has been split into focused submodules:
//! - `timing_helpers`: Timing utilities and wait conditions with CI environment support
//! - `cluster_helpers`: Cluster and node setup utilities for integration tests
//! - `vm_helpers`: VM configuration and database utilities for testing
//! - `mod`: Main coordinator with port allocation and database utilities

// Re-export everything from the modular implementation
#[allow(unused_imports)]
pub use test_helpers_modules::*;

mod test_helpers_modules;