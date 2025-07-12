//! UI-related types for the TUI

use std::time::Instant;

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

#[derive(Debug, Clone, PartialEq)]
pub enum PerformanceMode {
    HighRefresh, // btop-style high-frequency updates
    Balanced,    // default mode
    PowerSaver,  // reduced updates for battery/low-power
    Debug,       // maximum detail with logging
}

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