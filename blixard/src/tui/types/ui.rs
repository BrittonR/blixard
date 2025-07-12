//! UI-related types for the TUI

use std::time::Instant;

/// Main navigation tabs in the TUI application
///
/// Defines the primary sections of the application that users
/// can navigate between using tab navigation or keyboard shortcuts.
/// Each tab represents a major functional area.
///
/// # Variants
///
/// * `Dashboard` - Overview of cluster status and key metrics
/// * `VirtualMachines` - VM management and monitoring
/// * `Nodes` - Cluster node management and health
/// * `Monitoring` - Detailed monitoring and metrics
/// * `P2P` - Peer-to-peer networking and image sharing
/// * `Configuration` - Cluster and application settings
/// * `Debug` - Debug information and diagnostics
/// * `Help` - User documentation and keyboard shortcuts
#[derive(Debug, Clone, PartialEq)]
pub enum AppTab {
    Dashboard,
    VirtualMachines,
    Nodes,
    Monitoring,
    P2P,
    Configuration,
    Debug,
    Help,
}

/// Current mode or view state of the application
///
/// Defines the specific view or interaction mode within the TUI.
/// This is more granular than tabs and includes popup dialogs,
/// forms, and specialized views. Controls which widgets are
/// displayed and how input is handled.
///
/// # Main View Modes
///
/// The primary modes correspond to different main content areas:
/// - `Dashboard`, `VmList`, `NodeList` - List and overview modes
/// - `VmDetails`, `NodeDetails` - Detailed information views
/// - `Monitoring` - Metrics and performance monitoring
///
/// # Form and Dialog Modes
///
/// Interactive modes for user input and configuration:
/// - `CreateVmForm`, `CreateNodeForm` - Resource creation
/// - `SettingsForm` - Application configuration
/// - `ConfirmDialog` - User confirmation prompts
///
/// # Debug and Utility Modes
///
/// Specialized modes for troubleshooting and advanced features:
/// - `Debug`, `RaftDebug` - System diagnostics
/// - `LogViewer` - Log streaming and filtering
/// - `SearchDialog` - Search and filtering interfaces
#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    // Main tab modes
    Dashboard,
    VmList,
    VmDetails,
    VmCreate,
    VmLogs,
    NodeList,
    NodeDetails,
    NodeDaemon,
    Monitoring,
    Config,
    Help,

    // Debug modes
    Debug,
    RaftDebug,
    DebugMetrics,
    DebugLogs,

    // Popup/overlay modes
    ConfirmDialog,
    CreateVmForm,
    CreateNodeForm,
    CreateClusterForm,
    ClusterDiscovery,
    NodeTemplateSelector,
    VmTemplateSelector,
    ClusterHealthView,
    SettingsForm,
    LogViewer,
    SearchDialog,
    BatchNodeCreation,
    BatchVmCreation,
    VmMigration,
    SaveConfig,
    LoadConfig,
    EditNodeConfig,
    P2P,
    ExportCluster,
    ImportCluster,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    Editing,
    Command,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Theme {
    Default,
    Dark,
    Light,
    Cyberpunk,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LogLevel {
    Error,
    Warning,
    Info,
    Debug,
    Trace,
}

/// Performance tuning modes for the TUI application
///
/// Controls the refresh rate and update frequency to balance
/// responsiveness with resource usage. Different modes are
/// suitable for different environments and use cases.
///
/// # Variants
///
/// * `HighRefresh` - Maximum responsiveness (1s refresh, like btop)
/// * `Balanced` - Default mode balancing performance and efficiency (5s refresh)
/// * `PowerSaver` - Reduced updates for battery/low-power environments (10s refresh)
/// * `Debug` - High detail with extensive logging (2s refresh)
///
/// # Usage
///
/// Users can cycle through modes with keyboard shortcuts to adapt
/// the application behavior to their current needs and environment.
#[derive(Debug, Clone, PartialEq)]
pub enum PerformanceMode {
    HighRefresh, // btop-style high-frequency updates
    Balanced,    // default mode
    PowerSaver,  // reduced updates for battery/low-power
    Debug,       // maximum detail with logging
}

/// Search and filtering modes for data exploration
///
/// Defines different approaches to searching and filtering
/// data within the TUI. Each mode provides different search
/// capabilities and interaction patterns.
///
/// # Variants
///
/// * `None` - No active search or filtering
/// * `Filter` - Simple substring filtering
/// * `Fuzzy` - Fuzzy matching for approximate searches
/// * `VmSearch` - VM-specific search with field awareness
/// * `NodeSearch` - Node-specific search with field awareness
/// * `QuickFilter` - Fast filtering across all current data
#[derive(Debug, Clone, PartialEq)]
pub enum SearchMode {
    None,
    Filter,
    Fuzzy,
    VmSearch,
    NodeSearch,
    QuickFilter,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    Connected,
    Connecting,
    Disconnected,
    Error(String),
    PartiallyConnected { connected: usize, total: usize },
}

impl ConnectionStatus {
    pub fn is_connected(&self) -> bool {
        matches!(self, ConnectionStatus::Connected | ConnectionStatus::PartiallyConnected { .. })
    }

    pub fn color(&self) -> ratatui::style::Color {
        use ratatui::style::Color;
        match self {
            ConnectionStatus::Connected => Color::Green,
            ConnectionStatus::Connecting => Color::Yellow,
            ConnectionStatus::Disconnected => Color::Red,
            ConnectionStatus::Error(_) => Color::Red,
            ConnectionStatus::PartiallyConnected { .. } => Color::Yellow,
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            ConnectionStatus::Connected => "●",
            ConnectionStatus::Connecting => "◐",
            ConnectionStatus::Disconnected => "○",
            ConnectionStatus::Error(_) => "✖",
            ConnectionStatus::PartiallyConnected { .. } => "◐",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LogSourceType {
    All,
    System,
    Vm(String),
    Node(u64),
    Cluster,
    Debug,
    Raft,
    GrpcServer,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: Instant,
    pub source: LogSourceType,
    pub level: LogLevel,
    pub message: String,
    pub context: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LogStreamConfig {
    pub enabled: bool,
    pub sources: Vec<LogSourceType>,
    pub level_filter: LogLevel,
    pub max_buffer_size: usize,
    pub selected_source: LogSourceType,
    pub filters: LogFilters,
    pub follow_mode: bool,
}

#[derive(Debug, Clone)]
pub struct LogFilters {
    pub log_level: LogLevel,
    pub search_text: String,
    pub show_timestamps: bool,
    pub highlight_errors: bool,
}