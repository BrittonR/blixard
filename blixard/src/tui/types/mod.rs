//! Type definitions for the TUI application
//!
//! This module contains all the data structures and enums used throughout
//! the TUI application, organized by functionality.

#![allow(unused_imports)]

pub mod vm;
pub mod node;
pub mod cluster;
pub mod monitoring;
pub mod debug;
pub mod ui;
pub mod p2p;

// Re-export commonly used types
pub use vm::{VmInfo, VmFilter, PlacementStrategy};
pub use node::{NodeInfo, NodeStatus, NodeRole, NodeFilter, NodeResourceInfo};
pub use cluster::{ClusterInfo, ClusterNodeInfo, ClusterMetrics};
pub use monitoring::{SystemEvent, EventLevel, HealthAlert, AlertSeverity, ClusterResourceInfo, HealthStatus, ClusterHealthStatus};
pub use debug::{RaftDebugInfo, RaftNodeState, DebugMetrics, DebugLevel};
pub use ui::{AppTab, AppMode, InputMode, LogLevel, SearchMode, ConnectionStatus, LogSourceType, LogEntry};
pub use p2p::{P2pPeer, P2pImage, P2pTransfer, NetworkQuality, ConnectionState};