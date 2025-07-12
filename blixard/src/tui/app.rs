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
    NodeInfo, NodeStatus, NodeRole, NodeFilter, NodeResourceInfo,
    ClusterInfo, ClusterMetrics, ClusterNodeInfo, ClusterResourceInfo,
    SystemEvent, EventLevel, HealthAlert, AlertSeverity, HealthStatus, ClusterHealthStatus,
    AppTab, AppMode, InputMode, ConnectionStatus, SearchMode, LogLevel, LogEntry, LogSourceType,
    RaftDebugInfo, DebugMetrics, RaftNodeState, DebugLevel,
    NetworkQuality,
};

pub use crate::tui::forms::{
    CreateVmForm, CreateVmField, VmMigrationForm, VmMigrationField,
    CreateNodeForm, CreateNodeField, EditNodeForm, EditNodeField,
    SaveConfigField, ExportForm, ImportForm, ExportFormField, ImportFormField,
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