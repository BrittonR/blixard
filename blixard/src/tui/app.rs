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
    vm::{VmInfo, VmFilter, PlacementStrategy},
    node::{NodeInfo, NodeStatus, NodeRole, NodeFilter},
    cluster::{ClusterInfo, ClusterMetrics},
    monitoring::{SystemEvent, EventLevel},
    ui::{AppTab, AppMode, InputMode, ConnectionStatus},
    debug::{RaftDebugInfo, DebugMetrics},
};

pub use crate::tui::forms::{
    vm::{CreateVmForm, CreateVmField},
    node::{CreateNodeForm, CreateNodeField},
    ConfirmDialog,
};

// Import what we need from the old app.rs for the migration
use super::p2p_view::format_bytes;
use super::{Event, VmClient};
use crate::{BlixardError, BlixardResult};
use blixard_core::types::VmStatus;
use ratatui::widgets::{ListState, TableState};
use serde_json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

// The rest of the implementation will be migrated step by step
// For now, we'll keep the existing handle_key_event and other methods