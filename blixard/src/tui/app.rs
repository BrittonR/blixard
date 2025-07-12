//! Modern TUI Application State
//!
//! Comprehensive TUI for Blixard cluster management with:
//! - Dashboard-first design with real-time metrics
//! - Enhanced VM management with scheduling
//! - Node management with daemon mode integration
//! - Monitoring and observability features
//! - Configuration management interface

// Re-export the App struct from the state module
pub use crate::tui::state::App;

// Re-export types that are commonly used outside this module
pub use crate::tui::types::{
    VmInfo, VmFilter, PlacementStrategy,
    NodeResourceInfo,
    ClusterInfo, ClusterNodeInfo, ClusterResourceInfo,
    P2pPeer, P2pTransfer, P2pImage,
};

pub use crate::tui::forms::{
    VmMigrationField,
    EditNodeField,
    SaveConfigField,
};

// Import what we need from the old app.rs for the migration
// TODO: Remove these unused imports after migration is complete

// The rest of the implementation will be migrated step by step
// For now, we'll keep the existing handle_key_event and other methods