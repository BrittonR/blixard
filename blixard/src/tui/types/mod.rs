//! Type definitions for the TUI application
//!
//! This module contains all the data structures and enums used throughout
//! the TUI application, organized by functionality.

pub mod vm;
pub mod node;
pub mod cluster;
pub mod monitoring;
pub mod debug;
pub mod ui;
pub mod p2p;

// Re-export commonly used types
pub use vm::{VmInfo, VmFilter, VmTemplateType, VmTemplate, PlacementStrategy};
pub use node::{NodeInfo, NodeStatus, NodeRole, NodeFilter, NodeTemplate, NodeResourceInfo};
pub use cluster::{ClusterInfo, ClusterNodeInfo, ClusterMetrics, ClusterHealth, ClusterTemplate, DiscoveredCluster};
pub use monitoring::{SystemEvent, EventLevel, NodeHealthSnapshot, HealthAlert, AlertSeverity, ClusterResourceInfo, HealthStatus, ClusterHealthStatus};
pub use debug::{RaftDebugInfo, RaftNodeState, SnapshotMetadata, PeerDebugInfo, PeerState, DebugMetrics, DebugLogEntry, DebugLevel};
pub use ui::{AppTab, AppMode, InputMode, Theme, LogLevel, PerformanceMode, SearchMode, ConnectionStatus, LogSourceType, LogEntry, LogStreamConfig};
pub use p2p::{P2pPeer, P2pImage, P2pTransfer, NetworkQuality};