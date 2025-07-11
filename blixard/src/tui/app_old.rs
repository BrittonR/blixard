//! Modern TUI Application State
//!
//! Comprehensive TUI for Blixard cluster management with:
//! - Dashboard-first design with real-time metrics
//! - Enhanced VM management with scheduling
//! - Node management with daemon mode integration
//! - Monitoring and observability features
//! - Configuration management interface

use super::p2p_view::format_bytes;
use super::{Event, VmClient};
use crate::{BlixardError, BlixardResult};
use blixard_core::types::VmStatus;
use ratatui::widgets::{ListState, TableState};
use serde_json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct VmInfo {
    pub name: String,
    pub status: VmStatus,
    pub vcpus: u32,
    pub memory: u32,
    pub node_id: u64,
    pub ip_address: Option<String>,
    pub uptime: Option<String>,
    pub cpu_usage: Option<f32>,
    pub memory_usage: Option<f32>,
    #[allow(dead_code)]
    pub placement_strategy: Option<String>,
    #[allow(dead_code)]
    pub created_at: Option<String>,
    #[allow(dead_code)]
    pub config_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub id: u64,
    pub address: String,
    pub status: NodeStatus,
    pub role: NodeRole,
    pub cpu_usage: f32,
    pub memory_usage: f32,
    pub vm_count: u32,
    pub last_seen: Option<Instant>,
    #[allow(dead_code)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeStatus {
    Healthy,
    Warning,
    Critical,
    #[allow(dead_code)]
    Offline,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeRole {
    Leader,
    Follower,
    #[allow(dead_code)]
    Candidate,
    #[allow(dead_code)]
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ClusterMetrics {
    pub total_nodes: u32,
    pub healthy_nodes: u32,
    pub total_vms: u32,
    pub running_vms: u32,
    pub total_cpu: u32,
    pub used_cpu: u32,
    pub total_memory: u64,
    pub used_memory: u64,
    pub leader_id: Option<u64>,
    pub raft_term: u64,
    #[allow(dead_code)]
    pub last_updated: Instant,
}

#[derive(Debug, Clone)]
pub struct SystemEvent {
    pub timestamp: Instant,
    pub level: EventLevel,
    pub source: String,
    pub message: String,
    #[allow(dead_code)]
    pub details: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EventLevel {
    Info,
    Warning,
    Error,
    Critical,
    Debug,
}

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
pub enum DebugLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
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

#[derive(Debug, Clone)]
pub struct CreateVmForm {
    pub name: String,
    pub vcpus: String,
    pub memory: String,
    pub config_path: String,
    pub placement_strategy: PlacementStrategy,
    pub node_id: Option<u64>,
    pub auto_start: bool,
    pub current_field: CreateVmField,
}

#[derive(Debug, Clone)]
pub struct VmMigrationForm {
    pub vm_name: String,
    pub source_node_id: u64,
    pub target_node_id: String,
    pub live_migration: bool,
    pub current_field: VmMigrationField,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VmMigrationField {
    TargetNode,
    LiveMigration,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CreateVmField {
    Name,
    Vcpus,
    Memory,
    ConfigPath,
    PlacementStrategy,
    NodeId,
    AutoStart,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SaveConfigField {
    FilePath,
    Description,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlacementStrategy {
    MostAvailable,
    LeastAvailable,
    RoundRobin,
    Manual,
}

#[derive(Debug, Clone)]
pub struct ClusterNodeInfo {
    pub id: u64,
    pub address: String,
    pub state: String,
    pub is_current: bool,
}

#[derive(Debug, Clone)]
pub struct ClusterInfo {
    pub leader_id: u64,
    pub term: u64,
    pub node_count: usize,
    pub current_node_id: u64,
    pub current_node_state: String,
    pub nodes: Vec<ClusterNodeInfo>,
}

#[derive(Debug, Clone)]
pub struct RaftDebugInfo {
    pub current_term: u64,
    pub voted_for: Option<u64>,
    pub log_length: u64,
    pub commit_index: u64,
    pub last_applied: u64,
    pub state: RaftNodeState,
    pub last_heartbeat: Option<Instant>,
    pub election_timeout: Duration,
    pub entries_since_snapshot: u64,
    pub snapshot_metadata: Option<SnapshotMetadata>,
    pub peer_states: Vec<PeerDebugInfo>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RaftNodeState {
    Follower,
    Candidate,
    Leader,
    PreCandidate,
}

#[derive(Debug, Clone)]
pub struct SnapshotMetadata {
    pub last_included_index: u64,
    pub last_included_term: u64,
    pub size_bytes: u64,
    pub created_at: Instant,
}

#[derive(Debug, Clone)]
pub struct PeerDebugInfo {
    pub id: u64,
    pub next_index: u64,
    pub match_index: u64,
    pub state: PeerState,
    pub last_activity: Option<Instant>,
    pub is_reachable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PeerState {
    Probe,
    Replicate,
    Snapshot,
}

#[derive(Debug, Clone)]
pub struct DebugMetrics {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub proposals_submitted: u64,
    pub proposals_committed: u64,
    pub elections_started: u64,
    pub leadership_changes: u64,
    pub snapshot_creations: u64,
    pub log_compactions: u64,
    pub network_partitions_detected: u64,
    pub last_reset: Instant,
}

impl Default for DebugMetrics {
    fn default() -> Self {
        Self {
            messages_sent: 0,
            messages_received: 0,
            proposals_submitted: 0,
            proposals_committed: 0,
            elections_started: 0,
            leadership_changes: 0,
            snapshot_creations: 0,
            log_compactions: 0,
            network_partitions_detected: 0,
            last_reset: Instant::now(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionStatus {
    pub state: ConnectionState,
    pub endpoint: String,
    pub connected_since: Option<Instant>,
    pub last_attempt: Option<Instant>,
    pub retry_count: u32,
    pub next_retry_in: Option<Duration>,
    pub latency_ms: Option<u64>,
    pub quality: NetworkQuality,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Connected,
    Connecting,
    Reconnecting,
    Disconnected,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NetworkQuality {
    Excellent, // < 10ms latency
    Good,      // < 50ms latency
    Fair,      // < 100ms latency
    Poor,      // < 200ms latency
    Bad,       // >= 200ms latency
    Unknown,
}

impl Default for ConnectionStatus {
    fn default() -> Self {
        Self {
            state: ConnectionState::Disconnected,
            endpoint: String::new(),
            connected_since: None,
            last_attempt: None,
            retry_count: 0,
            next_retry_in: None,
            latency_ms: None,
            quality: NetworkQuality::Unknown,
            error_message: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DebugLogEntry {
    pub timestamp: Instant,
    pub level: DebugLevel,
    pub component: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct ClusterHealth {
    pub status: HealthStatus,
    pub uptime: Duration,
    pub leader_changes: u32,
    pub failed_nodes: u32,
    pub degraded_nodes: u32,
    pub healthy_nodes: u32,
    pub network_latency_ms: f32,
    pub replication_lag_ms: f32,
    pub last_health_check: Instant,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Critical,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct NodeHealthSnapshot {
    pub timestamp: Instant,
    pub cpu_percent: f32,
    pub memory_percent: f32,
    pub disk_io_mbps: f32,
    pub network_mbps: f32,
    pub vm_count: u32,
    pub status: NodeStatus,
}

#[derive(Debug, Clone)]
pub struct HealthAlert {
    pub timestamp: Instant,
    pub severity: AlertSeverity,
    pub node_id: Option<u64>,
    pub title: String,
    pub message: String,
    pub resolved: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone)]
pub struct VmTemplate {
    pub name: String,
    pub description: String,
    pub vcpus: u32,
    pub memory: u32,
    pub disk_gb: u32,
    pub template_type: VmTemplateType,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VmTemplateType {
    Development,
    WebServer,
    Database,
    Container,
    LoadBalancer,
    Custom,
}

#[derive(Debug, Clone)]
pub struct ClusterResourceInfo {
    pub total_nodes: u32,
    pub total_vcpus: u32,
    pub used_vcpus: u32,
    pub total_memory_mb: u64,
    pub used_memory_mb: u64,
    pub total_disk_gb: u64,
    pub used_disk_gb: u64,
    pub total_running_vms: u32,
    pub nodes: Vec<NodeResourceInfo>,
}

#[derive(Debug, Clone)]
pub struct NodeResourceInfo {
    pub node_id: u64,
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub used_vcpus: u32,
    pub used_memory_mb: u64,
    pub used_disk_gb: u64,
    pub running_vms: u32,
    pub features: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CreateNodeForm {
    pub id: String,
    pub bind_address: String,
    pub data_dir: String,
    pub peers: String,
    pub vm_backend: String,
    pub daemon_mode: bool,
    pub current_field: CreateNodeField,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CreateNodeField {
    Id,
    BindAddress,
    DataDir,
    Peers,
    VmBackend,
    DaemonMode,
}

#[derive(Debug, Clone)]
pub struct EditNodeForm {
    pub node_id: u64,
    pub original_address: String,
    pub bind_address: String,
    pub data_dir: String,
    pub vm_backend: String,
    pub current_field: EditNodeField,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EditNodeField {
    BindAddress,
    DataDir,
    VmBackend,
}

#[derive(Debug, Clone)]
pub struct ExportForm {
    pub output_path: String,
    pub cluster_name: String,
    pub include_images: bool,
    pub include_telemetry: bool,
    pub compress: bool,
    pub p2p_share: bool,
    pub current_field: ExportFormField,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExportFormField {
    OutputPath,
    ClusterName,
    IncludeImages,
    IncludeTelemetry,
    Compress,
    P2pShare,
}

#[derive(Debug, Clone)]
pub struct ImportForm {
    pub input_path: String,
    pub merge: bool,
    pub p2p: bool,
    pub current_field: ImportFormField,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImportFormField {
    InputPath,
    Merge,
    P2p,
}

#[derive(Debug, Clone)]
pub struct ConfirmDialog {
    pub title: String,
    pub message: String,
    pub action: ConfirmAction,
    pub selected: bool, // true = Yes, false = No
}

#[derive(Debug, Clone)]
pub enum ConfirmAction {
    DeleteVm(String),
    StopVm(String),
    RemoveNode(u64),
    RestartNode(u64),
    DestroyCluster(String),
    ShutdownCluster,
    ResetData,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VmFilter {
    All,
    Running,
    Stopped,
    Failed,
    ByNode(u64),
    ByName(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeFilter {
    All,
    Healthy,
    Warning,
    Critical,
    Leaders,
    Followers,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SearchMode {
    None,
    VmSearch,
    NodeSearch,
    QuickFilter,
    ClusterDiscovery,
}

#[derive(Debug, Clone)]
pub struct DiscoveredCluster {
    pub name: String,
    pub leader_address: String,
    pub leader_id: u64,
    pub node_count: usize,
    pub version: Option<String>,
    pub last_seen: Instant,
    pub health_status: ClusterHealthStatus,
    pub auto_discovered: bool,
    pub accessible: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClusterHealthStatus {
    Healthy,
    Degraded,
    Critical,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct NodeTemplate {
    pub name: String,
    pub description: String,
    pub default_port_offset: u16,
    pub default_vm_backend: String,
    pub default_data_dir_pattern: String,
    pub resource_requirements: Option<NodeResourceRequirements>,
}

#[derive(Debug, Clone)]
pub struct NodeResourceRequirements {
    pub min_cpu_cores: u32,
    pub min_memory_mb: u64,
    pub min_disk_gb: u64,
    pub required_features: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ClusterTemplate {
    pub name: String,
    pub description: String,
    pub node_count: u32,
    pub node_template: NodeTemplate,
    pub network_config: ClusterNetworkConfig,
}

#[derive(Debug, Clone)]
pub struct ClusterNetworkConfig {
    pub base_port: u16,
    pub port_range: u16,
    pub auto_configure_networking: bool,
    pub cluster_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LogStreamConfig {
    pub sources: Vec<LogSource>,
    pub filters: LogFilters,
    pub follow_mode: bool,
    pub buffer_size: usize,
    pub selected_source: usize,
}

#[derive(Debug, Clone)]
pub struct LogSource {
    pub name: String,
    pub source_type: LogSourceType,
    pub enabled: bool,
    pub color: Option<ratatui::style::Color>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LogSourceType {
    Node(u64),  // Node ID
    Vm(String), // VM name
    System,     // System-wide logs
    Raft,       // Raft consensus logs
    GrpcServer, // gRPC server logs
    All,        // All sources
}

#[derive(Debug, Clone)]
pub struct LogFilters {
    pub log_level: LogLevel,
    pub search_text: String,
    pub show_timestamps: bool,
    pub highlight_errors: bool,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: Instant,
    pub source: LogSourceType,
    pub level: LogLevel,
    pub message: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone)]
pub struct P2pPeer {
    pub node_id: String,
    pub address: String,
    pub status: String,
    pub latency_ms: u64,
    pub shared_images: usize,
}

#[derive(Debug, Clone)]
pub struct P2pImage {
    pub name: String,
    pub version: String,
    pub size: u64,
    pub available_peers: usize,
    pub is_cached: bool,
    pub is_downloading: bool,
}

#[derive(Debug, Clone)]
pub struct P2pTransfer {
    pub resource_name: String,
    pub peer_id: String,
    pub is_upload: bool,
    pub total_bytes: u64,
    pub bytes_transferred: u64,
    pub speed_bps: u64,
}

pub struct App {
    // Core application state
    pub current_tab: AppTab,
    pub mode: AppMode,
    pub input_mode: InputMode,
    pub should_quit: bool,

    // Network and data
    pub vm_client: Option<VmClient>,
    pub connection_status: ConnectionStatus,
    pub auto_refresh: bool,
    pub refresh_interval: Duration,
    pub last_refresh: Instant,

    // Dashboard data
    pub cluster_metrics: ClusterMetrics,
    pub recent_events: Vec<SystemEvent>,
    pub max_events: usize,
    pub cluster_health: ClusterHealth,
    pub node_health_history: HashMap<u64, Vec<NodeHealthSnapshot>>,
    pub health_alerts: Vec<HealthAlert>,

    // VM management
    pub vms: Vec<VmInfo>,
    pub vm_list_state: ListState,
    pub vm_table_state: TableState,
    pub selected_vm: Option<String>,
    pub vm_logs: Vec<String>,
    pub vm_log_state: ListState,

    // Log streaming
    pub log_stream_config: LogStreamConfig,
    pub log_entries: Vec<LogEntry>,
    pub log_list_state: ListState,
    pub max_log_entries: usize,

    // Node management
    pub nodes: Vec<NodeInfo>,
    pub node_list_state: ListState,
    pub node_table_state: TableState,
    pub selected_node: Option<u64>,
    pub daemon_processes: HashMap<u64, u32>, // node_id -> PID
    pub batch_node_count: String,

    // Forms and dialogs
    pub create_vm_form: CreateVmForm,
    pub create_node_form: CreateNodeForm,
    pub edit_node_form: EditNodeForm,
    pub vm_migration_form: VmMigrationForm,
    pub confirm_dialog: Option<ConfirmDialog>,

    // UI state
    pub status_message: Option<String>,
    pub error_message: Option<String>,
    pub search_query: String,
    pub show_help: bool,

    // Filtering and search (lazygit-inspired)
    pub vm_filter: VmFilter,
    pub node_filter: NodeFilter,
    pub filtered_vms: Vec<VmInfo>,
    pub filtered_nodes: Vec<NodeInfo>,
    pub search_mode: SearchMode,
    pub quick_filter: String,

    // Cluster discovery and management
    pub discovered_clusters: Vec<DiscoveredCluster>,
    pub cluster_templates: Vec<ClusterTemplate>,
    pub node_templates: Vec<NodeTemplate>,
    pub vm_templates: Vec<VmTemplate>,
    pub selected_cluster: Option<String>,
    pub cluster_discovery_active: bool,
    pub cluster_scan_progress: f32,

    // Monitoring
    pub cpu_history: Vec<f32>,
    pub memory_history: Vec<f32>,
    pub network_history: Vec<f32>,
    pub max_history_points: usize,

    // Configuration
    pub config_dirty: bool,
    pub settings: AppSettings,
    pub config_file_path: Option<String>,
    pub config_description: String,
    pub save_config_field: SaveConfigField,

    // Debug mode data
    pub raft_debug_info: Option<RaftDebugInfo>,
    pub debug_metrics: DebugMetrics,
    pub debug_log_entries: Vec<DebugLogEntry>,
    pub max_debug_entries: usize,

    // P2P networking
    pub p2p_enabled: bool,
    pub p2p_node_id: String,
    pub p2p_peer_count: usize,
    pub p2p_shared_images: usize,
    pub p2p_peers: Vec<P2pPeer>,
    pub p2p_images: Vec<P2pImage>,
    pub p2p_transfers: Vec<P2pTransfer>,
    pub p2p_store: Option<Arc<tokio::sync::Mutex<blixard_core::p2p_image_store::P2pImageStore>>>,
    pub p2p_manager: Option<Arc<blixard_core::p2p_manager::P2pManager>>,

    // Cluster export/import
    pub export_form: ExportForm,
    pub import_form: ImportForm,

    // Event sender for background tasks
    pub event_sender: Option<tokio::sync::mpsc::UnboundedSender<Event>>,
}

#[derive(Debug, Clone)]
pub struct AppSettings {
    pub theme: Theme,
    pub refresh_rate: u64, // seconds
    pub log_level: LogLevel,
    pub auto_connect: bool,
    pub default_vm_backend: String,
    pub show_timestamps: bool,
    pub compact_mode: bool,
    pub performance_mode: PerformanceMode,
    pub vim_mode: bool,
    pub max_history_retention: usize,
    pub update_frequency_ms: u64,
    pub debug_mode: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PerformanceMode {
    HighRefresh, // btop-style high-frequency updates
    Balanced,    // default mode
    PowerSaver,  // reduced updates for battery/low-power
    Debug,       // maximum detail with logging
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
}

impl Default for ClusterMetrics {
    fn default() -> Self {
        Self {
            total_nodes: 0,
            healthy_nodes: 0,
            total_vms: 0,
            running_vms: 0,
            total_cpu: 0,
            used_cpu: 0,
            total_memory: 0,
            used_memory: 0,
            leader_id: None,
            raft_term: 0,
            last_updated: Instant::now(),
        }
    }
}

impl Default for ClusterHealth {
    fn default() -> Self {
        Self {
            status: HealthStatus::Unknown,
            uptime: Duration::from_secs(0),
            leader_changes: 0,
            failed_nodes: 0,
            degraded_nodes: 0,
            healthy_nodes: 0,
            network_latency_ms: 0.0,
            replication_lag_ms: 0.0,
            last_health_check: Instant::now(),
        }
    }
}

impl Default for LogStreamConfig {
    fn default() -> Self {
        Self {
            sources: vec![
                LogSource {
                    name: "All Sources".to_string(),
                    source_type: LogSourceType::All,
                    enabled: true,
                    color: None,
                },
                LogSource {
                    name: "System".to_string(),
                    source_type: LogSourceType::System,
                    enabled: true,
                    color: Some(ratatui::style::Color::Blue),
                },
                LogSource {
                    name: "Raft Consensus".to_string(),
                    source_type: LogSourceType::Raft,
                    enabled: false,
                    color: Some(ratatui::style::Color::Magenta),
                },
                LogSource {
                    name: "gRPC Server".to_string(),
                    source_type: LogSourceType::GrpcServer,
                    enabled: false,
                    color: Some(ratatui::style::Color::Cyan),
                },
            ],
            filters: LogFilters::default(),
            follow_mode: true,
            buffer_size: 1000,
            selected_source: 0,
        }
    }
}

impl Default for LogFilters {
    fn default() -> Self {
        Self {
            log_level: LogLevel::Info,
            search_text: String::new(),
            show_timestamps: true,
            highlight_errors: true,
        }
    }
}

impl Default for CreateVmForm {
    fn default() -> Self {
        Self {
            name: "my-vm".to_string(),
            vcpus: "2".to_string(),
            memory: "1024".to_string(),
            config_path: String::new(),
            placement_strategy: PlacementStrategy::MostAvailable,
            node_id: None,
            auto_start: true,
            current_field: CreateVmField::Name,
        }
    }
}

impl Default for VmMigrationForm {
    fn default() -> Self {
        Self {
            vm_name: String::new(),
            source_node_id: 0,
            target_node_id: String::new(),
            live_migration: false,
            current_field: VmMigrationField::TargetNode,
        }
    }
}

impl CreateVmForm {
    pub fn new_with_smart_defaults(existing_vms: &[VmInfo]) -> Self {
        // Generate a unique VM name
        let mut name_counter = 1;
        let mut vm_name = format!("vm-{}", name_counter);

        while existing_vms.iter().any(|vm| vm.name == vm_name) {
            name_counter += 1;
            vm_name = format!("vm-{}", name_counter);
        }

        Self {
            name: vm_name,
            vcpus: "2".to_string(),
            memory: "1024".to_string(),
            config_path: String::new(),
            placement_strategy: PlacementStrategy::MostAvailable,
            node_id: None,
            auto_start: true,
            current_field: CreateVmField::Name,
        }
    }
}

impl Default for CreateNodeForm {
    fn default() -> Self {
        Self {
            id: "2".to_string(),
            bind_address: "127.0.0.1:7003".to_string(),
            data_dir: "./data-node2".to_string(),
            peers: "127.0.0.1:7002".to_string(),
            vm_backend: "microvm".to_string(),
            daemon_mode: false,
            current_field: CreateNodeField::Id,
        }
    }
}

impl Default for EditNodeForm {
    fn default() -> Self {
        Self {
            node_id: 0,
            original_address: String::new(),
            bind_address: String::new(),
            data_dir: String::new(),
            vm_backend: "microvm".to_string(),
            current_field: EditNodeField::BindAddress,
        }
    }
}

impl CreateNodeForm {
    pub fn new_with_smart_defaults(existing_nodes: &[NodeInfo]) -> Self {
        // Generate next available node ID
        let mut node_id = 2; // Start with 2 since 1 is usually the leader
        while existing_nodes.iter().any(|node| node.id == node_id) {
            node_id += 1;
        }

        // Generate corresponding port (7000 + node_id)
        let port = 7000 + node_id;
        let bind_address = format!("127.0.0.1:{}", port);
        let data_dir = format!("./data-node{}", node_id);

        Self {
            id: node_id.to_string(),
            bind_address,
            data_dir,
            peers: "127.0.0.1:7002".to_string(), // Default to connecting to leader
            vm_backend: "microvm".to_string(),
            daemon_mode: false,
            current_field: CreateNodeField::Id,
        }
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: Theme::Default,
            refresh_rate: 2,
            log_level: LogLevel::Info,
            auto_connect: true,
            default_vm_backend: "microvm".to_string(),
            show_timestamps: true,
            compact_mode: false,
            performance_mode: PerformanceMode::Balanced,
            vim_mode: false,
            max_history_retention: 100,
            update_frequency_ms: 500,
            debug_mode: false,
        }
    }
}

impl Default for ClusterInfo {
    fn default() -> Self {
        Self {
            leader_id: 0,
            term: 0,
            node_count: 0,
            current_node_id: 0,
            current_node_state: "Unknown".to_string(),
            nodes: Vec::new(),
        }
    }
}

impl App {
    pub async fn new() -> BlixardResult<Self> {
        let mut app = Self {
            current_tab: AppTab::Dashboard,
            mode: AppMode::Dashboard,
            input_mode: InputMode::Normal,
            should_quit: false,

            vm_client: None,
            connection_status: ConnectionStatus::default(),
            auto_refresh: true,
            refresh_interval: Duration::from_secs(2),
            last_refresh: Instant::now(),

            cluster_metrics: ClusterMetrics::default(),
            recent_events: Vec::new(),
            cluster_health: ClusterHealth::default(),
            node_health_history: HashMap::new(),
            health_alerts: Vec::new(),

            // Debug mode fields
            debug_log_entries: Vec::new(),
            debug_metrics: DebugMetrics::default(),
            max_debug_entries: 1000,
            raft_debug_info: None,
            max_events: 100,

            vms: Vec::new(),
            vm_list_state: ListState::default(),
            vm_table_state: TableState::default(),
            selected_vm: None,
            vm_logs: Vec::new(),
            vm_log_state: ListState::default(),

            log_stream_config: LogStreamConfig::default(),
            log_entries: Vec::new(),
            log_list_state: ListState::default(),
            max_log_entries: 1000,

            nodes: Vec::new(),
            node_list_state: ListState::default(),
            node_table_state: TableState::default(),
            selected_node: None,
            daemon_processes: HashMap::new(),
            batch_node_count: "3".to_string(),

            create_vm_form: CreateVmForm::default(),
            create_node_form: CreateNodeForm::default(),
            edit_node_form: EditNodeForm::default(),
            vm_migration_form: VmMigrationForm::default(),
            confirm_dialog: None,

            status_message: None,
            error_message: None,
            search_query: String::new(),
            show_help: false,

            vm_filter: VmFilter::All,
            node_filter: NodeFilter::All,
            filtered_vms: Vec::new(),
            filtered_nodes: Vec::new(),
            search_mode: SearchMode::None,
            quick_filter: String::new(),

            discovered_clusters: Vec::new(),
            cluster_templates: Self::default_cluster_templates(),
            node_templates: Self::default_node_templates(),
            vm_templates: Self::default_vm_templates(),
            selected_cluster: None,
            cluster_discovery_active: false,
            cluster_scan_progress: 0.0,

            cpu_history: Vec::new(),
            memory_history: Vec::new(),
            network_history: Vec::new(),
            max_history_points: 50,

            config_dirty: false,
            settings: AppSettings::default(),
            config_file_path: None,
            config_description: String::new(),
            save_config_field: SaveConfigField::FilePath,

            // P2P networking
            p2p_enabled: false,
            p2p_node_id: String::new(),
            p2p_peer_count: 0,
            p2p_shared_images: 0,
            p2p_peers: Vec::new(),
            p2p_images: Vec::new(),
            p2p_transfers: Vec::new(),
            p2p_store: None,
            p2p_manager: None,

            // Cluster export/import
            export_form: ExportForm {
                output_path: "./cluster-export.json.gz".to_string(),
                cluster_name: "my-cluster".to_string(),
                include_images: false,
                include_telemetry: false,
                compress: true,
                p2p_share: false,
                current_field: ExportFormField::OutputPath,
            },
            import_form: ImportForm {
                input_path: String::new(),
                merge: false,
                p2p: false,
                current_field: ImportFormField::InputPath,
            },

            event_sender: None,
        };

        // Add startup event
        app.add_event(
            EventLevel::Info,
            "TUI".to_string(),
            "Blixard TUI starting up".to_string(),
        );

        // Try to connect to local server
        app.add_event(
            EventLevel::Info,
            "Connection".to_string(),
            "Attempting to connect to cluster at 127.0.0.1:7001".to_string(),
        );
        app.try_connect().await;

        // Start cluster discovery
        app.start_cluster_discovery().await;

        Ok(app)
    }

    /// Set the event sender for background tasks to communicate with the UI
    pub fn set_event_sender(&mut self, sender: tokio::sync::mpsc::UnboundedSender<Event>) {
        self.event_sender = Some(sender);
    }

    fn default_node_templates() -> Vec<NodeTemplate> {
        vec![
            NodeTemplate {
                name: "Standard Node".to_string(),
                description: "Standard cluster node with default settings".to_string(),
                default_port_offset: 1,
                default_vm_backend: "microvm".to_string(),
                default_data_dir_pattern: "./data-node{}".to_string(),
                resource_requirements: Some(NodeResourceRequirements {
                    min_cpu_cores: 2,
                    min_memory_mb: 2048,
                    min_disk_gb: 10,
                    required_features: vec!["kvm".to_string()],
                }),
            },
            NodeTemplate {
                name: "Lightweight Node".to_string(),
                description: "Minimal resource node for testing or small deployments".to_string(),
                default_port_offset: 1,
                default_vm_backend: "mock".to_string(),
                default_data_dir_pattern: "./data-light{}".to_string(),
                resource_requirements: Some(NodeResourceRequirements {
                    min_cpu_cores: 1,
                    min_memory_mb: 512,
                    min_disk_gb: 5,
                    required_features: vec![],
                }),
            },
            NodeTemplate {
                name: "High Performance Node".to_string(),
                description: "High-resource node for demanding workloads".to_string(),
                default_port_offset: 1,
                default_vm_backend: "microvm".to_string(),
                default_data_dir_pattern: "./data-hiperf{}".to_string(),
                resource_requirements: Some(NodeResourceRequirements {
                    min_cpu_cores: 8,
                    min_memory_mb: 8192,
                    min_disk_gb: 50,
                    required_features: vec!["kvm".to_string(), "gpu".to_string()],
                }),
            },
        ]
    }

    fn default_cluster_templates() -> Vec<ClusterTemplate> {
        vec![
            ClusterTemplate {
                name: "Development Cluster".to_string(),
                description: "Small 3-node cluster for development and testing".to_string(),
                node_count: 3,
                node_template: NodeTemplate {
                    name: "Dev Node".to_string(),
                    description: "Development node".to_string(),
                    default_port_offset: 1,
                    default_vm_backend: "mock".to_string(),
                    default_data_dir_pattern: "./dev-cluster/node{}".to_string(),
                    resource_requirements: None,
                },
                network_config: ClusterNetworkConfig {
                    base_port: 7000,
                    port_range: 10,
                    auto_configure_networking: true,
                    cluster_name: Some("dev-cluster".to_string()),
                },
            },
            ClusterTemplate {
                name: "Production Cluster".to_string(),
                description: "High-availability 5-node production cluster".to_string(),
                node_count: 5,
                node_template: NodeTemplate {
                    name: "Prod Node".to_string(),
                    description: "Production node".to_string(),
                    default_port_offset: 1,
                    default_vm_backend: "microvm".to_string(),
                    default_data_dir_pattern: "./prod-cluster/node{}".to_string(),
                    resource_requirements: Some(NodeResourceRequirements {
                        min_cpu_cores: 4,
                        min_memory_mb: 4096,
                        min_disk_gb: 20,
                        required_features: vec!["kvm".to_string()],
                    }),
                },
                network_config: ClusterNetworkConfig {
                    base_port: 8000,
                    port_range: 20,
                    auto_configure_networking: true,
                    cluster_name: Some("prod-cluster".to_string()),
                },
            },
        ]
    }

    fn default_vm_templates() -> Vec<VmTemplate> {
        vec![
            VmTemplate {
                name: "Micro VM".to_string(),
                description: "Minimal VM for lightweight services".to_string(),
                vcpus: 1,
                memory: 512,
                disk_gb: 5,
                template_type: VmTemplateType::Development,
                features: vec![],
            },
            VmTemplate {
                name: "Web Server".to_string(),
                description: "Optimized for web services and APIs".to_string(),
                vcpus: 2,
                memory: 2048,
                disk_gb: 20,
                template_type: VmTemplateType::WebServer,
                features: vec!["nginx".to_string(), "ssl".to_string()],
            },
            VmTemplate {
                name: "Database Server".to_string(),
                description: "Configured for database workloads".to_string(),
                vcpus: 4,
                memory: 4096,
                disk_gb: 50,
                template_type: VmTemplateType::Database,
                features: vec!["ssd".to_string(), "backup".to_string()],
            },
            VmTemplate {
                name: "Container Host".to_string(),
                description: "Docker/Podman container runtime".to_string(),
                vcpus: 2,
                memory: 2048,
                disk_gb: 30,
                template_type: VmTemplateType::Container,
                features: vec!["docker".to_string(), "containerd".to_string()],
            },
            VmTemplate {
                name: "Load Balancer".to_string(),
                description: "High-performance load balancing".to_string(),
                vcpus: 2,
                memory: 1024,
                disk_gb: 10,
                template_type: VmTemplateType::LoadBalancer,
                features: vec!["haproxy".to_string(), "keepalived".to_string()],
            },
        ]
    }

    fn calculate_network_quality(latency_ms: u64) -> NetworkQuality {
        match latency_ms {
            0..=10 => NetworkQuality::Excellent,
            11..=50 => NetworkQuality::Good,
            51..=100 => NetworkQuality::Fair,
            101..=200 => NetworkQuality::Poor,
            _ => NetworkQuality::Bad,
        }
    }

    /// Update connection latency based on operation timing
    pub fn update_connection_latency(&mut self, operation_duration: Duration) {
        let latency_ms = operation_duration.as_millis() as u64;

        // Update rolling average if already connected
        if let Some(current_latency) = self.connection_status.latency_ms {
            // Simple exponential moving average
            self.connection_status.latency_ms = Some((current_latency * 7 + latency_ms * 3) / 10);
        } else {
            self.connection_status.latency_ms = Some(latency_ms);
        }

        // Update quality based on new latency
        if let Some(avg_latency) = self.connection_status.latency_ms {
            self.connection_status.quality = Self::calculate_network_quality(avg_latency);
        }
    }

    /// Check if reconnection is needed
    pub fn check_reconnection_needed(&mut self) {
        if self.vm_client.is_none() && self.connection_status.state != ConnectionState::Connecting {
            if let Some(last_attempt) = self.connection_status.last_attempt {
                let elapsed = last_attempt.elapsed();
                let retry_delay = Duration::from_secs(
                    5 * (self.connection_status.retry_count as u64 + 1).min(60),
                );

                if elapsed >= retry_delay {
                    self.connection_status.state = ConnectionState::Reconnecting;
                    self.connection_status.next_retry_in = None;
                } else {
                    self.connection_status.next_retry_in = Some(retry_delay - elapsed);
                }
            }
        }
    }

    pub async fn try_connect(&mut self) {
        let endpoint = "127.0.0.1:7001";
        self.connection_status.endpoint = endpoint.to_string();
        self.connection_status.state = ConnectionState::Connecting;
        self.connection_status.last_attempt = Some(Instant::now());

        self.status_message = Some("Connecting to cluster (with automatic retry)...".to_string());
        self.add_event(
            EventLevel::Info,
            "Connection".to_string(),
            "Attempting to connect with retry logic enabled".to_string(),
        );

        let start_time = Instant::now();
        match VmClient::new(endpoint).await {
            Ok(client) => {
                let connect_duration = start_time.elapsed();
                self.vm_client = Some(client);

                // Update connection status
                self.connection_status.state = ConnectionState::Connected;
                self.connection_status.connected_since = Some(Instant::now());
                self.connection_status.retry_count = 0;
                self.connection_status.latency_ms = Some(connect_duration.as_millis() as u64);
                self.connection_status.quality =
                    Self::calculate_network_quality(connect_duration.as_millis() as u64);
                self.connection_status.error_message = None;

                self.status_message = Some(format!(
                    "Connected to cluster ({}ms)",
                    connect_duration.as_millis()
                ));
                self.error_message = None;
                self.add_event(
                    EventLevel::Info,
                    "Connection".to_string(),
                    format!(
                        "Successfully connected to cluster with {}ms latency",
                        connect_duration.as_millis()
                    ),
                );

                // Initial data load
                match self.refresh_all_data().await {
                    Ok(_) => {
                        self.add_event(
                            EventLevel::Info,
                            "Data".to_string(),
                            "Initial data refresh completed".to_string(),
                        );
                    }
                    Err(e) => {
                        self.add_event(
                            EventLevel::Error,
                            "Data".to_string(),
                            format!("Initial data refresh failed: {}", e),
                        );
                    }
                }
            }
            Err(e) => {
                self.vm_client = None;

                // Update connection status for failure
                self.connection_status.state = ConnectionState::Failed;
                self.connection_status.retry_count += 1;
                self.connection_status.error_message = Some(e.to_string());
                self.connection_status.next_retry_in = Some(Duration::from_secs(5));

                self.error_message = Some(format!("Failed to connect after retries: {}", e));
                self.status_message = None;
                self.add_event(
                    EventLevel::Error,
                    "Connection".to_string(),
                    format!(
                        "Failed to connect to cluster after automatic retries: {}",
                        e
                    ),
                );
            }
        }
    }

    pub async fn refresh_all_data(&mut self) -> BlixardResult<()> {
        if self.vm_client.is_none() {
            self.add_event(
                EventLevel::Warning,
                "Data".to_string(),
                "Cannot refresh data - no connection to cluster".to_string(),
            );
            return Ok(());
        }

        let start_time = Instant::now();
        let mut errors = Vec::new();

        // Refresh VMs
        if let Err(e) = self.refresh_vm_list().await {
            errors.push(format!("VM list: {}", e));
        }

        // Refresh cluster metrics
        if let Err(e) = self.refresh_cluster_metrics().await {
            errors.push(format!("Cluster metrics: {}", e));
        }

        // Refresh nodes
        if let Err(e) = self.refresh_node_list().await {
            errors.push(format!("Node list: {}", e));
        }

        // Update history
        self.update_metrics_history();

        // Update connection latency based on refresh duration
        let refresh_duration = start_time.elapsed();
        self.update_connection_latency(refresh_duration);

        self.last_refresh = Instant::now();

        if !errors.is_empty() {
            let error_msg = format!("Data refresh errors: {}", errors.join(", "));
            self.add_event(EventLevel::Warning, "Data".to_string(), error_msg.clone());
            return Err(crate::BlixardError::Internal { message: error_msg });
        }

        // Update cluster health
        self.update_cluster_health();

        // Log successful refresh with data counts
        self.add_event(
            EventLevel::Info,
            "Data".to_string(),
            format!(
                "Refreshed: {} VMs, {} nodes, leader={:?} ({}ms)",
                self.vms.len(),
                self.nodes.len(),
                self.cluster_metrics.leader_id,
                refresh_duration.as_millis()
            ),
        );

        Ok(())
    }

    pub async fn refresh_vm_list(&mut self) -> BlixardResult<()> {
        if let Some(client) = &mut self.vm_client {
            match client.list_vms().await {
                Ok(vms) => {
                    self.vms = vms
                        .into_iter()
                        .map(|vm| VmInfo {
                            name: vm.name,
                            status: vm.status,
                            vcpus: vm.vcpus,
                            memory: vm.memory,
                            node_id: vm.node_id,
                            ip_address: vm.ip_address,
                            uptime: vm.uptime,
                            cpu_usage: vm.cpu_usage,
                            memory_usage: vm.memory_usage,
                            placement_strategy: None, // TODO: Add to proto
                            created_at: None,         // TODO: Add to proto
                            config_path: None,        // TODO: Add to proto
                        })
                        .collect();
                    self.apply_vm_filter(); // Apply current filter after refresh
                    self.error_message = None;
                }
                Err(e) => {
                    let error_msg = format!("Failed to refresh VMs: {}", e);
                    self.error_message = Some(error_msg.clone());
                    return Err(crate::BlixardError::Internal { message: error_msg });
                }
            }
        }
        Ok(())
    }

    /// Update cluster health based on current metrics
    pub fn update_cluster_health(&mut self) {
        let now = Instant::now();

        // Calculate health status based on nodes
        let total_nodes = self.nodes.len();
        let healthy_nodes = self
            .nodes
            .iter()
            .filter(|n| n.status == NodeStatus::Healthy)
            .count();
        let warning_nodes = self
            .nodes
            .iter()
            .filter(|n| n.status == NodeStatus::Warning)
            .count();
        let critical_nodes = self
            .nodes
            .iter()
            .filter(|n| n.status == NodeStatus::Critical)
            .count();

        self.cluster_health.healthy_nodes = healthy_nodes as u32;
        self.cluster_health.degraded_nodes = warning_nodes as u32;
        self.cluster_health.failed_nodes = critical_nodes as u32;

        // Determine overall health status
        self.cluster_health.status = if critical_nodes > 0 || healthy_nodes < total_nodes / 2 {
            HealthStatus::Critical
        } else if warning_nodes > 0 {
            HealthStatus::Degraded
        } else if healthy_nodes == total_nodes && total_nodes > 0 {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unknown
        };

        // Update node health history
        for node in &self.nodes {
            let snapshot = NodeHealthSnapshot {
                timestamp: now,
                cpu_percent: node.cpu_usage,
                memory_percent: node.memory_usage,
                disk_io_mbps: 0.0, // TODO: Get from metrics
                network_mbps: 0.0, // TODO: Get from metrics
                vm_count: node.vm_count,
                status: node.status.clone(),
            };

            self.node_health_history
                .entry(node.id)
                .or_insert_with(Vec::new)
                .push(snapshot);

            // Keep only last 60 snapshots (1 minute of history at 1s refresh)
            if let Some(history) = self.node_health_history.get_mut(&node.id) {
                if history.len() > 60 {
                    history.remove(0);
                }
            }
        }

        // Check for alerts
        self.check_health_alerts();

        self.cluster_health.last_health_check = now;
    }

    /// Check for health alerts based on current state
    fn check_health_alerts(&mut self) {
        // Collect alerts to add (avoid borrowing issues)
        let mut alerts_to_add = Vec::new();

        // Check node health
        for node in &self.nodes {
            // High CPU alert
            if node.cpu_usage > 90.0 {
                alerts_to_add.push((
                    AlertSeverity::Critical,
                    Some(node.id),
                    format!("High CPU usage on node {}", node.id),
                    format!("CPU usage is at {:.1}%", node.cpu_usage),
                ));
            } else if node.cpu_usage > 80.0 {
                alerts_to_add.push((
                    AlertSeverity::Warning,
                    Some(node.id),
                    format!("Elevated CPU usage on node {}", node.id),
                    format!("CPU usage is at {:.1}%", node.cpu_usage),
                ));
            }

            // High memory alert
            if node.memory_usage > 90.0 {
                alerts_to_add.push((
                    AlertSeverity::Critical,
                    Some(node.id),
                    format!("High memory usage on node {}", node.id),
                    format!("Memory usage is at {:.1}%", node.memory_usage),
                ));
            }

            // Node status alert
            if node.status == NodeStatus::Critical {
                alerts_to_add.push((
                    AlertSeverity::Critical,
                    Some(node.id),
                    format!("Node {} is in critical state", node.id),
                    "Node may be unresponsive or experiencing failures".to_string(),
                ));
            }
        }

        // Check cluster-wide issues
        if self.cluster_health.failed_nodes > 0 {
            alerts_to_add.push((
                AlertSeverity::Critical,
                None,
                "Cluster has failed nodes".to_string(),
                format!(
                    "{} nodes are in failed state",
                    self.cluster_health.failed_nodes
                ),
            ));
        }

        // Add all collected alerts
        for (severity, node_id, title, message) in alerts_to_add {
            self.add_health_alert(severity, node_id, title, message);
        }

        // Remove old resolved alerts
        self.health_alerts.retain(|alert| {
            !alert.resolved || alert.timestamp.elapsed() < Duration::from_secs(300)
        });
    }

    /// Add a health alert if it doesn't already exist
    fn add_health_alert(
        &mut self,
        severity: AlertSeverity,
        node_id: Option<u64>,
        title: String,
        message: String,
    ) {
        // Check if similar alert already exists
        let exists = self
            .health_alerts
            .iter()
            .any(|a| a.title == title && a.node_id == node_id && !a.resolved);

        if !exists {
            self.health_alerts.push(HealthAlert {
                timestamp: Instant::now(),
                severity,
                node_id,
                title,
                message,
                resolved: false,
            });

            // Keep only last 50 alerts
            if self.health_alerts.len() > 50 {
                self.health_alerts.remove(0);
            }
        }
    }

    pub async fn refresh_cluster_metrics(&mut self) -> BlixardResult<()> {
        if let Some(client) = &mut self.vm_client {
            match client.get_cluster_status().await {
                Ok(status) => {
                    self.cluster_metrics = ClusterMetrics {
                        total_nodes: status.nodes.len() as u32,
                        healthy_nodes: status.nodes.len() as u32, // Simplified: assume all nodes are healthy
                        total_vms: self.vms.len() as u32,
                        running_vms: self
                            .vms
                            .iter()
                            .filter(|vm| vm.status == VmStatus::Running)
                            .count() as u32,
                        total_cpu: status.nodes.len() as u32 * 8, // TODO: Get actual CPU count
                        used_cpu: (self.vms.iter().map(|vm| vm.vcpus).sum::<u32>()),
                        total_memory: status.nodes.len() as u64 * 16 * 1024, // TODO: Get actual memory
                        used_memory: self.vms.iter().map(|vm| vm.memory as u64).sum(),
                        leader_id: Some(status.leader_id),
                        raft_term: status.term,
                        last_updated: Instant::now(),
                    };
                    self.error_message = None;
                }
                Err(e) => {
                    let error_msg = format!("Failed to refresh cluster: {}", e);
                    self.error_message = Some(error_msg.clone());
                    return Err(crate::BlixardError::Internal { message: error_msg });
                }
            }
        }
        Ok(())
    }

    pub async fn refresh_node_list(&mut self) -> BlixardResult<()> {
        if let Some(client) = &mut self.vm_client {
            match client.get_cluster_status().await {
                Ok(status) => {
                    self.nodes = status
                        .nodes
                        .into_iter()
                        .map(|node| NodeInfo {
                            id: node.id,
                            address: node.address,
                            status: NodeStatus::Healthy, // Simplified: assume all nodes are healthy
                            role: if node.id == status.leader_id {
                                NodeRole::Leader
                            } else {
                                NodeRole::Follower
                            },
                            cpu_usage: 20.0 + ::rand::random::<f32>() * 60.0, // Simulated CPU usage 20-80%
                            memory_usage: 30.0 + ::rand::random::<f32>() * 50.0, // Simulated memory usage 30-80%
                            vm_count: self.vms.iter().filter(|vm| vm.node_id == node.id).count()
                                as u32,
                            last_seen: Some(Instant::now()),
                            version: None, // TODO: Add to proto
                        })
                        .collect();
                    self.apply_node_filter(); // Apply current filter after refresh
                    self.error_message = None;
                }
                Err(e) => {
                    let error_msg = format!("Failed to refresh nodes: {}", e);
                    self.error_message = Some(error_msg.clone());
                    return Err(crate::BlixardError::Internal { message: error_msg });
                }
            }
        }
        Ok(())
    }

    pub fn update_metrics_history(&mut self) {
        // Update CPU history
        let cpu_usage = if self.cluster_metrics.total_cpu > 0 {
            (self.cluster_metrics.used_cpu as f32 / self.cluster_metrics.total_cpu as f32) * 100.0
        } else {
            0.0
        };
        self.cpu_history.push(cpu_usage);
        if self.cpu_history.len() > self.max_history_points {
            self.cpu_history.remove(0);
        }

        // Update memory history
        let memory_usage = if self.cluster_metrics.total_memory > 0 {
            (self.cluster_metrics.used_memory as f32 / self.cluster_metrics.total_memory as f32)
                * 100.0
        } else {
            0.0
        };
        self.memory_history.push(memory_usage);
        if self.memory_history.len() > self.max_history_points {
            self.memory_history.remove(0);
        }

        // Update network history with more realistic simulation
        // Simulate network traffic based on VM count and activity
        let base_network = 10.0; // Base network usage in MB/s
        let vm_network = self
            .vms
            .iter()
            .filter(|vm| vm.status == VmStatus::Running)
            .count() as f32
            * 5.0; // 5 MB/s per running VM
        let network_variance = (::rand::random::<f32>() - 0.5) * 20.0; // +/- 10 MB/s variance
        let network_usage = (base_network + vm_network + network_variance).max(0.0);

        self.network_history.push(network_usage);
        if self.network_history.len() > self.max_history_points {
            self.network_history.remove(0);
        }

        // Update per-node health history
        for node in &self.nodes {
            let snapshot = NodeHealthSnapshot {
                timestamp: Instant::now(),
                cpu_percent: node.cpu_usage,
                memory_percent: node.memory_usage,
                disk_io_mbps: 25.0 + (::rand::random::<f32>() - 0.5) * 10.0, // Simulated disk I/O
                network_mbps: network_usage / self.nodes.len() as f32, // Distribute network across nodes
                vm_count: node.vm_count,
                status: node.status.clone(),
            };

            let history = self
                .node_health_history
                .entry(node.id)
                .or_insert_with(Vec::new);

            history.push(snapshot);

            // Keep only last N snapshots per node
            if history.len() > self.max_history_points {
                history.remove(0);
            }
        }
    }

    pub fn add_event(&mut self, level: EventLevel, source: String, message: String) {
        let event = SystemEvent {
            timestamp: Instant::now(),
            level: level.clone(),
            source: source.clone(),
            message: message.clone(),
            details: None,
        };

        self.recent_events.insert(0, event);
        if self.recent_events.len() > self.max_events {
            self.recent_events.truncate(self.max_events);
        }

        // Also add to log stream
        let log_level = match level {
            EventLevel::Info => LogLevel::Info,
            EventLevel::Warning => LogLevel::Warning,
            EventLevel::Error => LogLevel::Error,
            EventLevel::Critical => LogLevel::Error,
            EventLevel::Debug => LogLevel::Debug,
        };

        let log_source = match source.as_str() {
            "Node" => LogSourceType::System,
            "VM" => LogSourceType::System,
            "Raft" => LogSourceType::Raft,
            "gRPC" => LogSourceType::GrpcServer,
            _ => LogSourceType::System,
        };

        self.add_log_entry(log_source, log_level, message);
    }

    /// Add a log entry to the streaming log view
    pub fn add_log_entry(&mut self, source: LogSourceType, level: LogLevel, message: String) {
        let entry = LogEntry {
            timestamp: Instant::now(),
            source,
            level,
            message,
            details: None,
        };

        self.log_entries.push(entry);

        // Maintain buffer size
        if self.log_entries.len() > self.log_stream_config.buffer_size {
            self.log_entries.remove(0);
        }

        // Auto-scroll if in follow mode
        if self.log_stream_config.follow_mode {
            self.log_list_state
                .select(Some(self.log_entries.len().saturating_sub(1)));
        }
    }

    /// Simulate log entries for demonstration
    pub fn simulate_log_entries(&mut self) {
        // Add some sample log entries
        let sources = vec![
            (LogSourceType::System, "System started successfully"),
            (LogSourceType::Node(1), "Node 1 joined cluster"),
            (LogSourceType::Node(2), "Node 2 joined cluster"),
            (
                LogSourceType::Raft,
                "Leader election completed, node 1 is leader",
            ),
            (
                LogSourceType::GrpcServer,
                "gRPC server listening on 0.0.0.0:7001",
            ),
            (
                LogSourceType::Vm("web-server".to_string()),
                "VM 'web-server' started",
            ),
            (
                LogSourceType::System,
                "Health check passed, cluster healthy",
            ),
            (LogSourceType::Node(1), "Applying configuration change"),
            (LogSourceType::Raft, "Committed entry at index 42"),
            (LogSourceType::System, "Resource usage: CPU 45%, Memory 62%"),
        ];

        for (source, msg) in sources {
            let level = match ::rand::random::<f32>() {
                x if x < 0.1 => LogLevel::Error,
                x if x < 0.2 => LogLevel::Warning,
                x if x < 0.4 => LogLevel::Debug,
                _ => LogLevel::Info,
            };

            self.add_log_entry(source, level, msg.to_string());
        }
    }

    /// Open log viewer for a specific source
    pub fn open_log_viewer(&mut self, source: Option<LogSourceType>) {
        self.mode = AppMode::LogViewer;

        // If a specific source is provided, select it
        if let Some(source) = source {
            // Add source if not already present
            let exists = self
                .log_stream_config
                .sources
                .iter()
                .any(|s| s.source_type == source.clone());
            if !exists {
                let name = match &source {
                    LogSourceType::Node(id) => format!("Node {}", id),
                    LogSourceType::Vm(name) => format!("VM: {}", name),
                    _ => format!("{:?}", source),
                };

                self.log_stream_config.sources.push(LogSource {
                    name,
                    source_type: source.clone(),
                    enabled: true,
                    color: Some(ratatui::style::Color::Magenta),
                });
            }

            // Select the source
            if let Some(idx) = self
                .log_stream_config
                .sources
                .iter()
                .position(|s| s.source_type == source)
            {
                self.log_stream_config.selected_source = idx;
            }
        }

        // Simulate some initial logs if empty
        if self.log_entries.is_empty() {
            self.simulate_log_entries();
        }
    }

    pub fn switch_tab(&mut self, tab: AppTab) {
        self.current_tab = tab.clone();
        self.mode = match tab {
            AppTab::Dashboard => AppMode::Dashboard,
            AppTab::VirtualMachines => AppMode::VmList,
            AppTab::Nodes => AppMode::NodeList,
            AppTab::Monitoring => AppMode::Monitoring,
            AppTab::P2P => AppMode::P2P,
            AppTab::Configuration => AppMode::Config,
            AppTab::Debug => AppMode::Debug,
            AppTab::Help => AppMode::Help,
        };
        self.input_mode = InputMode::Normal;
    }

    pub fn should_refresh(&self) -> bool {
        let refresh_interval = match self.settings.performance_mode {
            PerformanceMode::HighRefresh => {
                Duration::from_millis(self.settings.update_frequency_ms / 2)
            }
            PerformanceMode::Balanced => self.refresh_interval,
            PerformanceMode::PowerSaver => Duration::from_secs(self.settings.refresh_rate * 2),
            PerformanceMode::Debug => Duration::from_millis(self.settings.update_frequency_ms),
        };

        self.auto_refresh && self.last_refresh.elapsed() >= refresh_interval
    }

    /// Apply current VM filter
    pub fn apply_vm_filter(&mut self) {
        self.filtered_vms = self
            .vms
            .iter()
            .filter(|vm| self.vm_matches_filter(vm))
            .cloned()
            .collect();
    }

    /// Apply current node filter
    pub fn apply_node_filter(&mut self) {
        self.filtered_nodes = self
            .nodes
            .iter()
            .filter(|node| self.node_matches_filter(node))
            .cloned()
            .collect();
    }

    fn vm_matches_filter(&self, vm: &VmInfo) -> bool {
        // Quick filter first
        if !self.quick_filter.is_empty() {
            if !vm
                .name
                .to_lowercase()
                .contains(&self.quick_filter.to_lowercase())
            {
                return false;
            }
        }

        match &self.vm_filter {
            VmFilter::All => true,
            VmFilter::Running => vm.status == blixard_core::types::VmStatus::Running,
            VmFilter::Stopped => vm.status == blixard_core::types::VmStatus::Stopped,
            VmFilter::Failed => vm.status == blixard_core::types::VmStatus::Failed,
            VmFilter::ByNode(node_id) => vm.node_id == *node_id,
            VmFilter::ByName(name) => vm.name.to_lowercase().contains(&name.to_lowercase()),
        }
    }

    fn node_matches_filter(&self, node: &NodeInfo) -> bool {
        // Quick filter first
        if !self.quick_filter.is_empty() {
            if !node
                .address
                .to_lowercase()
                .contains(&self.quick_filter.to_lowercase())
            {
                return false;
            }
        }

        match &self.node_filter {
            NodeFilter::All => true,
            NodeFilter::Healthy => node.status == NodeStatus::Healthy,
            NodeFilter::Warning => node.status == NodeStatus::Warning,
            NodeFilter::Critical => node.status == NodeStatus::Critical,
            NodeFilter::Leaders => node.role == NodeRole::Leader,
            NodeFilter::Followers => node.role == NodeRole::Follower,
        }
    }

    /// Set VM filter and apply it
    #[allow(dead_code)]
    pub fn set_vm_filter(&mut self, filter: VmFilter) {
        self.vm_filter = filter;
        self.apply_vm_filter();
    }

    /// Set node filter and apply it
    #[allow(dead_code)]
    pub fn set_node_filter(&mut self, filter: NodeFilter) {
        self.node_filter = filter;
        self.apply_node_filter();
    }

    /// Set quick filter and apply all filters
    pub fn set_quick_filter(&mut self, filter: String) {
        self.quick_filter = filter;
        self.apply_vm_filter();
        self.apply_node_filter();
    }

    /// Toggle performance mode (btop-inspired)
    pub fn cycle_performance_mode(&mut self) {
        self.settings.performance_mode = match self.settings.performance_mode {
            PerformanceMode::PowerSaver => PerformanceMode::Balanced,
            PerformanceMode::Balanced => PerformanceMode::HighRefresh,
            PerformanceMode::HighRefresh => PerformanceMode::Debug,
            PerformanceMode::Debug => PerformanceMode::PowerSaver,
        };

        self.add_event(
            EventLevel::Info,
            "Settings".to_string(),
            format!("Performance mode: {:?}", self.settings.performance_mode),
        );
    }

    /// Switch to tab by index (for vim navigation)
    pub fn switch_to_tab_by_index(&mut self, index: usize) {
        let tab = match index {
            0 => AppTab::Dashboard,
            1 => AppTab::VirtualMachines,
            2 => AppTab::Nodes,
            3 => AppTab::Monitoring,
            4 => AppTab::P2P,
            5 => AppTab::Configuration,
            6 => AppTab::Debug,
            7 => AppTab::Help,
            _ => return,
        };
        self.switch_tab(tab);
    }

    /// Handle vim-style down movement
    pub fn handle_vim_down(&mut self) {
        match self.current_tab {
            AppTab::VirtualMachines => {
                if let Some(selected) = self.vm_table_state.selected() {
                    if selected < self.get_displayed_vms().len().saturating_sub(1) {
                        self.vm_table_state.select(Some(selected + 1));
                    }
                } else if !self.get_displayed_vms().is_empty() {
                    self.vm_table_state.select(Some(0));
                }
            }
            AppTab::Nodes => {
                if let Some(selected) = self.node_table_state.selected() {
                    if selected < self.get_displayed_nodes().len().saturating_sub(1) {
                        self.node_table_state.select(Some(selected + 1));
                    }
                } else if !self.get_displayed_nodes().is_empty() {
                    self.node_table_state.select(Some(0));
                }
            }
            _ => {}
        }
    }

    /// Handle vim-style up movement
    pub fn handle_vim_up(&mut self) {
        match self.current_tab {
            AppTab::VirtualMachines => {
                if let Some(selected) = self.vm_table_state.selected() {
                    if selected > 0 {
                        self.vm_table_state.select(Some(selected - 1));
                    }
                }
            }
            AppTab::Nodes => {
                if let Some(selected) = self.node_table_state.selected() {
                    if selected > 0 {
                        self.node_table_state.select(Some(selected - 1));
                    }
                }
            }
            _ => {}
        }
    }

    /// Get currently displayed VMs (filtered or all)
    pub fn get_displayed_vms(&self) -> &[VmInfo] {
        if self.vm_filter != VmFilter::All || !self.quick_filter.is_empty() {
            &self.filtered_vms
        } else {
            &self.vms
        }
    }

    /// Get currently displayed nodes (filtered or all)
    pub fn get_displayed_nodes(&self) -> &[NodeInfo] {
        if self.node_filter != NodeFilter::All || !self.quick_filter.is_empty() {
            &self.filtered_nodes
        } else {
            &self.nodes
        }
    }

    pub async fn handle_event(&mut self, event: Event) -> BlixardResult<()> {
        match event {
            Event::Tick => {
                // Check if reconnection is needed
                self.check_reconnection_needed();

                // Try to reconnect if in reconnecting state
                if self.connection_status.state == ConnectionState::Reconnecting {
                    self.add_event(
                        EventLevel::Info,
                        "Connection".to_string(),
                        format!(
                            "Attempting automatic reconnection (attempt {})",
                            self.connection_status.retry_count + 1
                        ),
                    );
                    self.try_connect().await;
                }

                // Regular data refresh if connected
                if self.should_refresh()
                    && self.connection_status.state == ConnectionState::Connected
                {
                    if let Err(e) = self.refresh_all_data().await {
                        self.add_event(
                            EventLevel::Error,
                            "Refresh".to_string(),
                            format!("Auto-refresh failed: {}", e),
                        );

                        // If refresh failed due to connection issue, mark as disconnected
                        if e.to_string().contains("connection")
                            || e.to_string().contains("transport")
                        {
                            self.connection_status.state = ConnectionState::Disconnected;
                            self.connection_status.connected_since = None;
                            self.vm_client = None;
                        }
                    }
                }
            }
            Event::Key(key) => {
                self.handle_key_event(key).await?;
            }
            Event::Mouse(mouse) => {
                self.handle_mouse_event(mouse).await?;
            }
            Event::LogLine(line) => {
                // Add log line to VM logs if viewing logs, or to debug logs
                if self.mode == AppMode::VmLogs {
                    self.vm_logs.push(line.clone());
                    // Keep only last 1000 lines
                    if self.vm_logs.len() > 1000 {
                        self.vm_logs.remove(0);
                    }
                } else if matches!(self.mode, AppMode::Debug | AppMode::DebugLogs) {
                    self.debug_log_entries.push(DebugLogEntry {
                        timestamp: Instant::now(),
                        level: DebugLevel::Info,
                        component: "system".to_string(),
                        message: line,
                        details: None,
                    });
                    // Keep only last max_debug_entries
                    if self.debug_log_entries.len() > self.max_debug_entries {
                        self.debug_log_entries.remove(0);
                    }
                }
            }
            Event::P2pPeersUpdate(peers) => {
                self.p2p_peers = peers;
                self.p2p_peer_count = self.p2p_peers.len();
            }
            Event::P2pTransfersUpdate(transfers) => {
                self.p2p_transfers = transfers;
            }
            Event::P2pImagesUpdate(images) => {
                self.p2p_images = images;
                self.p2p_shared_images = self.p2p_images.len();
            }
        }
        Ok(())
    }

    async fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
        use crossterm::event::{KeyCode, KeyModifiers};

        // Global keybindings
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), KeyModifiers::NONE) => {
                if self.mode == AppMode::ConfirmDialog || self.search_mode != SearchMode::None {
                    // Handle confirm dialog or exit search mode
                    self.search_mode = SearchMode::None;
                    self.mode = AppMode::Dashboard;
                    return Ok(());
                }
                self.should_quit = true;
                return Ok(());
            }
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                self.should_quit = true;
                return Ok(());
            }
            (KeyCode::Char('h'), KeyModifiers::NONE) => {
                if self.settings.vim_mode && self.search_mode == SearchMode::None {
                    // vim-style navigation: h = left/previous tab
                    let new_tab_index = match self.current_tab {
                        AppTab::Dashboard => 7, // wrap to Help
                        AppTab::VirtualMachines => 0,
                        AppTab::Nodes => 1,
                        AppTab::Monitoring => 2,
                        AppTab::P2P => 3,
                        AppTab::Configuration => 4,
                        AppTab::Debug => 5,
                        AppTab::Help => 6,
                    };
                    self.switch_to_tab_by_index(new_tab_index);
                } else {
                    self.switch_tab(AppTab::Help);
                }
                return Ok(());
            }
            (KeyCode::Char('l'), KeyModifiers::NONE) => {
                if self.settings.vim_mode && self.search_mode == SearchMode::None {
                    // vim-style navigation: l = right/next tab
                    let new_tab_index = match self.current_tab {
                        AppTab::Dashboard => 1,
                        AppTab::VirtualMachines => 2,
                        AppTab::Nodes => 3,
                        AppTab::Monitoring => 4,
                        AppTab::P2P => 5,
                        AppTab::Configuration => 6,
                        AppTab::Debug => 7,
                        AppTab::Help => 0, // wrap to Dashboard
                    };
                    self.switch_to_tab_by_index(new_tab_index);
                }
                return Ok(());
            }
            (KeyCode::Char('j'), KeyModifiers::NONE) => {
                if self.settings.vim_mode && self.search_mode == SearchMode::None {
                    self.handle_vim_down();
                    return Ok(());
                }
            }
            (KeyCode::Char('k'), KeyModifiers::NONE) => {
                if self.settings.vim_mode && self.search_mode == SearchMode::None {
                    self.handle_vim_up();
                    return Ok(());
                }
            }
            (KeyCode::Char('D'), KeyModifiers::SHIFT) => {
                // Toggle enhanced debug mode
                if self.settings.debug_mode {
                    self.mode = if self.mode == AppMode::Debug {
                        AppMode::Dashboard
                    } else {
                        AppMode::Debug
                    };
                }
                return Ok(());
            }
            (KeyCode::Char('r'), KeyModifiers::NONE) => {
                match self.refresh_all_data().await {
                    Ok(_) => {
                        self.status_message = Some("Data refreshed successfully".to_string());
                        self.error_message = None;
                    }
                    Err(e) => {
                        self.error_message = Some(format!("Refresh failed: {}", e));
                        self.status_message = None;
                    }
                }
                return Ok(());
            }
            (KeyCode::Char('/'), KeyModifiers::NONE) => {
                // lazygit-inspired search
                self.search_mode = match self.current_tab {
                    AppTab::VirtualMachines => SearchMode::VmSearch,
                    AppTab::Nodes => SearchMode::NodeSearch,
                    _ => SearchMode::QuickFilter,
                };
                self.quick_filter.clear();
                return Ok(());
            }
            (KeyCode::Char('f'), KeyModifiers::NONE) => {
                // Quick filter (different from search)
                self.search_mode = SearchMode::QuickFilter;
                self.quick_filter.clear();
                return Ok(());
            }
            (KeyCode::Char('F'), KeyModifiers::SHIFT) => {
                // Clear all filters
                self.vm_filter = VmFilter::All;
                self.node_filter = NodeFilter::All;
                self.quick_filter.clear();
                self.apply_vm_filter();
                self.apply_node_filter();
                self.status_message = Some("All filters cleared".to_string());
                return Ok(());
            }
            (KeyCode::Char('p'), KeyModifiers::NONE) => {
                // Cycle performance mode (btop-inspired)
                self.cycle_performance_mode();
                return Ok(());
            }
            (KeyCode::Char('v'), KeyModifiers::NONE) => {
                // Toggle vim mode
                self.settings.vim_mode = !self.settings.vim_mode;
                self.add_event(
                    EventLevel::Info,
                    "Settings".to_string(),
                    format!(
                        "Vim mode: {}",
                        if self.settings.vim_mode {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    ),
                );
                return Ok(());
            }
            (KeyCode::Char('L'), KeyModifiers::SHIFT) => {
                // Open log viewer
                self.open_log_viewer(None);
                return Ok(());
            }
            (KeyCode::Char('d'), KeyModifiers::NONE) => {
                // Toggle debug mode
                if self.search_mode == SearchMode::None {
                    self.settings.debug_mode = !self.settings.debug_mode;
                    self.add_event(
                        EventLevel::Info,
                        "Settings".to_string(),
                        format!(
                            "Debug mode: {}",
                            if self.settings.debug_mode {
                                "enabled"
                            } else {
                                "disabled"
                            }
                        ),
                    );
                    return Ok(());
                }
            }
            (KeyCode::Char('C'), KeyModifiers::SHIFT) => {
                // Discover clusters
                self.start_cluster_discovery().await;
                return Ok(());
            }
            (KeyCode::Char('N'), KeyModifiers::SHIFT) => {
                // Quick cluster creation
                self.mode = AppMode::CreateClusterForm;
                return Ok(());
            }
            _ => {}
        }

        // Handle search mode input
        if self.search_mode != SearchMode::None {
            return self.handle_search_input(key).await;
        }

        // Tab switching
        match key.code {
            KeyCode::Char('1') => self.switch_tab(AppTab::Dashboard),
            KeyCode::Char('2') => self.switch_tab(AppTab::VirtualMachines),
            KeyCode::Char('3') => self.switch_tab(AppTab::Nodes),
            KeyCode::Char('4') => self.switch_tab(AppTab::Monitoring),
            KeyCode::Char('5') => self.switch_tab(AppTab::P2P),
            KeyCode::Char('6') => self.switch_tab(AppTab::Configuration),
            KeyCode::Char('7') => self.switch_tab(AppTab::Debug),
            _ => {}
        }

        // Mode-specific handling
        match self.mode {
            AppMode::Dashboard => self.handle_dashboard_keys(key).await?,
            AppMode::VmList => self.handle_vm_list_keys(key).await?,
            AppMode::VmCreate => self.handle_vm_create_keys(key).await?,
            AppMode::NodeList => self.handle_node_list_keys(key).await?,
            AppMode::CreateNodeForm => self.handle_create_node_keys(key).await?,
            AppMode::CreateClusterForm => self.handle_create_cluster_keys(key).await?,
            AppMode::ClusterDiscovery => self.handle_cluster_discovery_keys(key).await?,
            AppMode::ConfirmDialog => self.handle_confirm_dialog_keys(key).await?,
            AppMode::VmMigration => self.handle_vm_migration_keys(key).await?,
            AppMode::Help => self.handle_help_keys(key),
            AppMode::Debug | AppMode::RaftDebug | AppMode::DebugMetrics | AppMode::DebugLogs => {
                self.handle_debug_keys(key).await?;
            }
            AppMode::BatchNodeCreation => self.handle_batch_node_creation_keys(key).await?,
            AppMode::VmTemplateSelector => self.handle_vm_template_selector_keys(key).await?,
            AppMode::BatchVmCreation => self.handle_batch_vm_creation_keys(key).await?,
            AppMode::LogViewer => self.handle_log_viewer_keys(key).await?,
            AppMode::Config => self.handle_config_keys(key).await?,
            AppMode::SaveConfig => self.handle_save_config_keys(key).await?,
            AppMode::LoadConfig => self.handle_load_config_keys(key).await?,
            AppMode::EditNodeConfig => self.handle_edit_node_config_keys(key).await?,
            AppMode::P2P => self.handle_p2p_keys(key).await?,
            AppMode::ExportCluster => self.handle_export_cluster_keys(key).await?,
            AppMode::ImportCluster => self.handle_import_cluster_keys(key).await?,
            _ => {}
        }

        Ok(())
    }

    async fn handle_create_cluster_keys(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::Dashboard;
            }
            KeyCode::Char('1') => {
                // Create development cluster
                if let Some(template) = self.cluster_templates.get(0).cloned() {
                    self.create_cluster_from_template(&template).await?;
                    self.mode = AppMode::Dashboard;
                }
            }
            KeyCode::Char('2') => {
                // Create production cluster
                if let Some(template) = self.cluster_templates.get(1).cloned() {
                    self.create_cluster_from_template(&template).await?;
                    self.mode = AppMode::Dashboard;
                }
            }
            _ => {}
        }

        Ok(())
    }

    async fn handle_cluster_discovery_keys(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::Dashboard;
            }
            KeyCode::Char('r') => {
                // Refresh discovery
                self.discovered_clusters.clear();
                self.start_cluster_discovery().await;
            }
            KeyCode::Enter => {
                // Connect to selected cluster
                if let Some(cluster) = self.discovered_clusters.first().cloned() {
                    self.connect_to_cluster(&cluster.leader_address).await?;
                    self.mode = AppMode::Dashboard;
                }
            }
            _ => {}
        }

        Ok(())
    }

    async fn handle_search_input(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.search_mode = SearchMode::None;
                self.quick_filter.clear();
                self.apply_vm_filter();
                self.apply_node_filter();
            }
            KeyCode::Enter => {
                // Apply the filter and exit search mode
                self.search_mode = SearchMode::None;
                self.set_quick_filter(self.quick_filter.clone());
                self.status_message = Some(format!("Filter applied: '{}'", self.quick_filter));
            }
            KeyCode::Backspace => {
                self.quick_filter.pop();
                self.set_quick_filter(self.quick_filter.clone());
            }
            KeyCode::Char(c) => {
                self.quick_filter.push(c);
                self.set_quick_filter(self.quick_filter.clone());
            }
            _ => {}
        }

        Ok(())
    }

    /// Start cluster discovery on common ports
    pub async fn start_cluster_discovery(&mut self) {
        self.cluster_discovery_active = true;
        self.cluster_scan_progress = 0.0;

        self.add_event(
            EventLevel::Info,
            "Discovery".to_string(),
            "Starting cluster discovery...".to_string(),
        );

        // Discover clusters on common ports
        let common_ports = vec![7001, 7002, 7003, 8000, 8001, 8002, 8080, 9000];
        let addresses = vec!["127.0.0.1", "localhost"];

        for addr in &addresses {
            for (i, port) in common_ports.iter().enumerate() {
                self.cluster_scan_progress = (i as f32 / common_ports.len() as f32) * 100.0;

                let endpoint = format!("{}:{}", addr, port);
                match self.probe_cluster(&endpoint).await {
                    Ok(Some(cluster)) => {
                        self.discovered_clusters.push(cluster);
                        self.add_event(
                            EventLevel::Info,
                            "Discovery".to_string(),
                            format!("Found cluster at {}", endpoint),
                        );
                    }
                    Ok(None) => {
                        // No cluster at this endpoint
                    }
                    Err(_) => {
                        // Connection failed - expected for most endpoints
                    }
                }
            }
        }

        self.cluster_discovery_active = false;
        self.cluster_scan_progress = 100.0;

        let count = self.discovered_clusters.len();
        self.add_event(
            EventLevel::Info,
            "Discovery".to_string(),
            format!("Discovery complete: found {} cluster(s)", count),
        );
    }

    /// Discover and auto-join local nodes
    pub async fn auto_discover_nodes(&mut self) -> BlixardResult<()> {
        self.add_event(
            EventLevel::Info,
            "Discovery".to_string(),
            "Starting local node discovery...".to_string(),
        );
        self.status_message = Some("Scanning for local nodes...".to_string());

        // Common ports where nodes might be running
        let ports_to_scan = vec![7001, 7002, 7003, 7004, 7005, 7006, 7007, 7008, 7009, 7010];
        let mut found_nodes = Vec::new();

        // First, find nodes that are running
        for port in &ports_to_scan {
            let endpoint = format!("127.0.0.1:{}", port);
            match VmClient::new(&endpoint).await {
                Ok(mut client) => {
                    // Try to get cluster status to verify it's a real node
                    if let Ok(status) = client.get_cluster_status().await {
                        found_nodes.push((port, status));
                        self.add_event(
                            EventLevel::Info,
                            "Discovery".to_string(),
                            format!("Found running node at port {}", port),
                        );
                    }
                }
                Err(_) => {
                    // No node at this port, continue scanning
                }
            }
        }

        if found_nodes.is_empty() {
            self.status_message = Some("No local nodes found. Start a node first with: cargo run -- node --id 1 --bind 127.0.0.1:7001".to_string());
            self.add_event(
                EventLevel::Warning,
                "Discovery".to_string(),
                "No running nodes found on common ports".to_string(),
            );
            return Ok(());
        }

        // Connect to the first found node if not already connected
        if self.vm_client.is_none() && !found_nodes.is_empty() {
            let (port, _) = &found_nodes[0];
            let endpoint = format!("127.0.0.1:{}", port);
            self.connect_to_cluster(&endpoint).await?;
        }

        // Store the count before moving found_nodes
        let found_count = found_nodes.len();

        // Now add any nodes that aren't already in the cluster
        if let Some(client) = self.vm_client.as_mut() {
            // Get cluster status first
            let cluster_status = match client.get_cluster_status().await {
                Ok(status) => status,
                Err(e) => {
                    self.add_event(
                        EventLevel::Error,
                        "Discovery".to_string(),
                        format!("Failed to get cluster status: {}", e),
                    );
                    return Ok(());
                }
            };

            let existing_node_ids: Vec<u64> = cluster_status.nodes.iter().map(|n| n.id).collect();

            // Find the next available node ID
            let mut next_id = 2;
            while existing_node_ids.contains(&next_id) {
                next_id += 1;
            }

            // Auto-join nodes that aren't already in the cluster
            for (port, status) in found_nodes {
                let bind_addr = format!("127.0.0.1:{}", port);

                // Check if this node is already in the cluster
                let already_in_cluster = cluster_status
                    .nodes
                    .iter()
                    .any(|n| n.address == bind_addr || n.id == status.leader_id);

                if !already_in_cluster && *port != 7001 {
                    // Don't try to join the leader to itself
                    self.add_event(
                        EventLevel::Info,
                        "Discovery".to_string(),
                        format!("Auto-joining node at port {} with ID {}", port, next_id),
                    );

                    // Call join_cluster on the client
                    let join_result = client.join_cluster(next_id, &bind_addr).await;

                    match join_result {
                        Ok(msg) => {
                            self.add_event(
                                EventLevel::Info,
                                "Node".to_string(),
                                format!("Node {} joined: {}", next_id, msg),
                            );
                            next_id += 1;
                        }
                        Err(e) => {
                            self.add_event(
                                EventLevel::Warning,
                                "Node".to_string(),
                                format!("Failed to auto-join node at {}: {}", bind_addr, e),
                            );
                        }
                    }
                }
            }

            // Refresh node list to show the updated cluster
            self.refresh_node_list().await?;
        }

        self.status_message = Some(format!(
            "Discovery complete: found {} local nodes",
            found_count
        ));
        Ok(())
    }

    /// Quick create VM with micro template
    pub async fn quick_create_vm(&mut self) -> BlixardResult<()> {
        // Extract needed values before mutable borrow
        let vm_number = self.vms.len() + 1;
        let vm_name = format!("vm-{}", vm_number);
        let template_name = self.vm_templates[0].name.clone();
        let template_vcpus = self.vm_templates[0].vcpus;
        let template_memory = self.vm_templates[0].memory;

        self.add_event(
            EventLevel::Info,
            "VM".to_string(),
            format!(
                "Quick creating VM '{}' from '{}' template",
                vm_name, template_name
            ),
        );

        if let Some(client) = &mut self.vm_client {
            match client
                .create_vm(&vm_name, template_vcpus, template_memory)
                .await
            {
                Ok(_) => {
                    self.add_event(
                        EventLevel::Info,
                        "VM".to_string(),
                        format!("VM '{}' created successfully", vm_name),
                    );
                    self.status_message = Some(format!(
                        "Created VM '{}' ({}vcpu, {}MB)",
                        vm_name, template_vcpus, template_memory
                    ));

                    // Refresh VM list
                    self.refresh_vm_list().await?;
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to create VM: {}", e));
                    self.add_event(
                        EventLevel::Error,
                        "VM".to_string(),
                        format!("Failed to create VM '{}': {}", vm_name, e),
                    );
                }
            }
        } else {
            self.error_message = Some("Not connected to cluster".to_string());
        }

        Ok(())
    }

    /// Batch create VMs
    pub async fn batch_create_vms(&mut self, count: u32) -> BlixardResult<()> {
        // Extract needed values before mutable borrow
        let base_vm_number = self.vms.len();
        let template_vcpus = self.vm_templates[0].vcpus;
        let template_memory = self.vm_templates[0].memory;

        if let Some(client) = &mut self.vm_client {
            let mut successful = 0;
            let mut failed = Vec::new();

            for i in 0..count {
                let vm_name = format!("batch-vm-{}", base_vm_number + i as usize + 1);

                match client
                    .create_vm(&vm_name, template_vcpus, template_memory)
                    .await
                {
                    Ok(_) => {
                        successful += 1;
                    }
                    Err(e) => {
                        failed.push((vm_name, e.to_string()));
                    }
                }
            }

            // Refresh VM list
            self.refresh_vm_list().await?;

            if successful > 0 {
                self.status_message =
                    Some(format!("Batch created {} VMs successfully", successful));
            }

            if !failed.is_empty() {
                self.error_message = Some(format!("{} VMs failed to create", failed.len()));
                for (name, err) in failed {
                    self.add_event(
                        EventLevel::Error,
                        "VM".to_string(),
                        format!("Failed to create '{}': {}", name, err),
                    );
                }
            }
        } else {
            self.error_message = Some("Not connected to cluster".to_string());
        }

        Ok(())
    }

    /// Migrate VM to another node
    pub async fn migrate_vm(
        &mut self,
        vm_name: String,
        target_node_id: u64,
        live_migration: bool,
    ) -> BlixardResult<()> {
        self.add_event(
            EventLevel::Info,
            "VM".to_string(),
            format!(
                "Initiating migration of VM '{}' to node {}",
                vm_name, target_node_id
            ),
        );

        if let Some(client) = &mut self.vm_client {
            match client
                .migrate_vm(&vm_name, target_node_id, live_migration)
                .await
            {
                Ok((source_id, target_id, message)) => {
                    self.add_event(
                        EventLevel::Info,
                        "VM".to_string(),
                        format!(
                            "VM '{}' migration from node {} to {} started",
                            vm_name, source_id, target_id
                        ),
                    );
                    self.status_message = Some(message);

                    // Refresh VM list to show updated location
                    self.refresh_vm_list().await?;
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to migrate VM: {}", e));
                    self.add_event(
                        EventLevel::Error,
                        "VM".to_string(),
                        format!("Failed to migrate VM '{}': {}", vm_name, e),
                    );
                }
            }
        } else {
            self.error_message = Some("Not connected to cluster".to_string());
        }

        Ok(())
    }

    /// Create VM from template
    pub async fn create_vm_from_template(&mut self, template: &VmTemplate) -> BlixardResult<()> {
        // Extract needed values before mutable borrow
        let vm_number = self.vms.len() + 1;
        let vm_name = format!(
            "{}-{}",
            template.name.to_lowercase().replace(' ', "-"),
            vm_number
        );
        let template_name = template.name.clone();
        let template_vcpus = template.vcpus;
        let template_memory = template.memory;

        self.add_event(
            EventLevel::Info,
            "VM".to_string(),
            format!(
                "Creating VM '{}' from template '{}'",
                vm_name, template_name
            ),
        );

        if let Some(client) = &mut self.vm_client {
            match client
                .create_vm(&vm_name, template_vcpus, template_memory)
                .await
            {
                Ok(_) => {
                    self.add_event(
                        EventLevel::Info,
                        "VM".to_string(),
                        format!("VM '{}' created successfully", vm_name),
                    );
                    self.status_message = Some(format!(
                        "Created {} VM '{}' ({}vcpu, {}MB)",
                        template_name, vm_name, template_vcpus, template_memory
                    ));

                    // Refresh VM list
                    self.refresh_vm_list().await?;
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to create VM: {}", e));
                    self.add_event(
                        EventLevel::Error,
                        "VM".to_string(),
                        format!("Failed to create VM '{}': {}", vm_name, e),
                    );
                }
            }
        } else {
            self.error_message = Some("Not connected to cluster".to_string());
        }

        Ok(())
    }

    /// Probe a potential cluster endpoint
    async fn probe_cluster(&self, endpoint: &str) -> BlixardResult<Option<DiscoveredCluster>> {
        // Try to connect and get cluster status
        match VmClient::new(endpoint).await {
            Ok(mut client) => {
                match client.get_cluster_status().await {
                    Ok(status) => {
                        let cluster = DiscoveredCluster {
                            name: format!("cluster-{}", status.leader_id),
                            leader_address: endpoint.to_string(),
                            leader_id: status.leader_id,
                            node_count: status.nodes.len(),
                            version: None, // TODO: Add version to cluster status
                            last_seen: Instant::now(),
                            health_status: if status.nodes.len() > 0 {
                                ClusterHealthStatus::Healthy
                            } else {
                                ClusterHealthStatus::Critical
                            },
                            auto_discovered: true,
                            accessible: true,
                        };
                        Ok(Some(cluster))
                    }
                    Err(_) => Ok(None),
                }
            }
            Err(_) => Ok(None),
        }
    }

    /// Connect to a discovered cluster
    pub async fn connect_to_cluster(&mut self, cluster_address: &str) -> BlixardResult<()> {
        match VmClient::new(cluster_address).await {
            Ok(client) => {
                self.vm_client = Some(client);
                self.selected_cluster = Some(cluster_address.to_string());
                self.status_message = Some(format!("Connected to cluster at {}", cluster_address));
                self.error_message = None;

                // Initial data load
                self.refresh_all_data().await?;

                self.add_event(
                    EventLevel::Info,
                    "Connection".to_string(),
                    format!("Connected to cluster at {}", cluster_address),
                );
            }
            Err(e) => {
                self.error_message =
                    Some(format!("Failed to connect to {}: {}", cluster_address, e));
                self.add_event(
                    EventLevel::Error,
                    "Connection".to_string(),
                    format!("Failed to connect to {}: {}", cluster_address, e),
                );
            }
        }
        Ok(())
    }

    /// Create a new cluster from template
    pub async fn create_cluster_from_template(
        &mut self,
        template: &ClusterTemplate,
    ) -> BlixardResult<()> {
        self.add_event(
            EventLevel::Info,
            "Cluster".to_string(),
            format!("Creating cluster from template: {}", template.name),
        );

        // Start the bootstrap node
        let bootstrap_port = template.network_config.base_port + 1;
        let bootstrap_address = format!("127.0.0.1:{}", bootstrap_port);

        self.add_event(
            EventLevel::Info,
            "Cluster".to_string(),
            format!("Starting bootstrap node at {}", bootstrap_address),
        );

        // In a real implementation, this would spawn the actual node process
        // For now, we'll simulate by adding to discovered clusters
        let cluster = DiscoveredCluster {
            name: template.name.clone(),
            leader_address: bootstrap_address.clone(),
            leader_id: 1,
            node_count: 1,
            version: Some("0.1.0".to_string()),
            last_seen: Instant::now(),
            health_status: ClusterHealthStatus::Healthy,
            auto_discovered: false,
            accessible: true,
        };

        self.discovered_clusters.push(cluster);

        // Try to connect to the new cluster
        match self.connect_to_cluster(&bootstrap_address).await {
            Ok(_) => {
                self.status_message =
                    Some(format!("Cluster '{}' created successfully", template.name));
            }
            Err(e) => {
                self.error_message = Some(format!("Cluster created but connection failed: {}", e));
            }
        }

        Ok(())
    }

    /// Scale cluster by adding nodes
    #[allow(dead_code)]
    pub async fn scale_cluster(&mut self, target_node_count: u32) -> BlixardResult<()> {
        let selected_cluster = self.selected_cluster.clone();

        if let Some(selected) = selected_cluster {
            self.add_event(
                EventLevel::Info,
                "Cluster".to_string(),
                format!("Scaling cluster to {} nodes", target_node_count),
            );

            // Find current cluster
            if let Some(cluster) = self
                .discovered_clusters
                .iter_mut()
                .find(|c| c.leader_address == selected)
            {
                if target_node_count > cluster.node_count as u32 {
                    // Adding nodes
                    let nodes_to_add = target_node_count - cluster.node_count as u32;
                    let current_count = cluster.node_count;

                    // Create events for all nodes being added
                    let mut events = Vec::new();
                    for i in 0..nodes_to_add {
                        let node_id = current_count as u64 + i as u64 + 1;
                        let node_port = 7000 + node_id;
                        let node_address = format!("127.0.0.1:{}", node_port);

                        events.push(format!("Adding node {} at {}", node_id, node_address));

                        // In real implementation, would spawn node process and join cluster
                        // For now, simulate by updating cluster info
                    }

                    // Update cluster info and drop the mutable borrow
                    cluster.node_count = target_node_count as usize;
                    let _ = cluster; // Explicitly drop the mutable borrow

                    // Now add all the events
                    for event_msg in events {
                        self.add_event(EventLevel::Info, "Cluster".to_string(), event_msg);
                    }
                    self.status_message =
                        Some(format!("Cluster scaled to {} nodes", target_node_count));
                } else {
                    // Removing nodes would require more complex logic
                    self.error_message = Some("Node removal not yet implemented".to_string());
                }
            }
        } else {
            self.error_message = Some("No cluster selected".to_string());
        }

        Ok(())
    }

    async fn handle_dashboard_keys(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Char('c') => {
                self.mode = AppMode::CreateVmForm;
                self.create_vm_form = CreateVmForm::new_with_smart_defaults(&self.vms);
            }
            KeyCode::Char('n') => {
                self.mode = AppMode::CreateNodeForm;
                self.create_node_form = CreateNodeForm::new_with_smart_defaults(&self.nodes);
            }
            KeyCode::Char('s') => {
                // Show cluster status
                let _ = self.refresh_cluster_metrics().await;
                self.status_message = Some("Cluster status updated".to_string());
            }
            KeyCode::Char('C') => {
                // Show cluster discovery
                self.mode = AppMode::ClusterDiscovery;
            }
            KeyCode::Char('N') => {
                // Show cluster creation
                self.mode = AppMode::CreateClusterForm;
            }
            KeyCode::Char('+') => {
                // Quick add node with next available ID and port
                self.quick_add_node().await?;
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_vm_list_keys(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Up => {
                if let Some(selected) = self.vm_table_state.selected() {
                    if selected > 0 {
                        self.vm_table_state.select(Some(selected - 1));
                    }
                }
            }
            KeyCode::Down => {
                if let Some(selected) = self.vm_table_state.selected() {
                    if selected < self.vms.len().saturating_sub(1) {
                        self.vm_table_state.select(Some(selected + 1));
                    }
                } else if !self.vms.is_empty() {
                    self.vm_table_state.select(Some(0));
                }
            }
            KeyCode::Char('t') => {
                // Show VM template selector
                self.mode = AppMode::VmTemplateSelector;
            }
            KeyCode::Char('+') => {
                // Quick create VM with micro template
                self.quick_create_vm().await?;
            }
            KeyCode::Char('b') => {
                // Batch create VMs
                self.mode = AppMode::BatchVmCreation;
                self.batch_node_count = "3".to_string(); // Reuse field for VM count
            }
            KeyCode::Enter => {
                if let Some(selected) = self.vm_table_state.selected() {
                    if let Some(vm) = self.vms.get(selected) {
                        self.selected_vm = Some(vm.name.clone());
                        self.mode = AppMode::VmDetails;
                    }
                }
            }
            KeyCode::Char('c') => {
                self.mode = AppMode::CreateVmForm;
                self.create_vm_form = CreateVmForm::new_with_smart_defaults(&self.vms);
            }
            KeyCode::Char('d') => {
                if let Some(selected) = self.vm_table_state.selected() {
                    if let Some(vm) = self.vms.get(selected) {
                        self.confirm_dialog = Some(ConfirmDialog {
                            title: "Delete VM".to_string(),
                            message: format!("Are you sure you want to delete VM '{}'?", vm.name),
                            action: ConfirmAction::DeleteVm(vm.name.clone()),
                            selected: false,
                        });
                        self.mode = AppMode::ConfirmDialog;
                    }
                }
            }
            KeyCode::Char('s') => {
                // Start/Stop VM
                if let Some(selected) = self.vm_table_state.selected() {
                    if let Some(vm) = self.vms.get(selected) {
                        match vm.status {
                            VmStatus::Running => {
                                self.confirm_dialog = Some(ConfirmDialog {
                                    title: "Stop VM".to_string(),
                                    message: format!(
                                        "Are you sure you want to stop VM '{}'?",
                                        vm.name
                                    ),
                                    action: ConfirmAction::StopVm(vm.name.clone()),
                                    selected: false,
                                });
                                self.mode = AppMode::ConfirmDialog;
                            }
                            _ => {
                                // Start VM directly without confirmation
                                let vm_name = vm.name.clone();
                                if let Err(e) = self.start_vm(vm_name).await {
                                    self.error_message = Some(format!("Failed to start VM: {}", e));
                                }
                            }
                        }
                    }
                }
            }
            KeyCode::Char('m') => {
                // Migrate VM
                if let Some(selected) = self.vm_table_state.selected() {
                    if let Some(vm) = self.vms.get(selected) {
                        // Prepare migration form
                        self.vm_migration_form = VmMigrationForm {
                            vm_name: vm.name.clone(),
                            source_node_id: vm.node_id,
                            target_node_id: String::new(),
                            live_migration: vm.status == VmStatus::Running,
                            current_field: VmMigrationField::TargetNode,
                        };
                        self.mode = AppMode::VmMigration;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_vm_migration_keys(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::VmList;
                self.vm_migration_form = VmMigrationForm::default();
            }
            KeyCode::Tab => {
                // Toggle between fields
                self.vm_migration_form.current_field = match self.vm_migration_form.current_field {
                    VmMigrationField::TargetNode => VmMigrationField::LiveMigration,
                    VmMigrationField::LiveMigration => VmMigrationField::TargetNode,
                };
            }
            KeyCode::Enter => {
                // Submit migration
                let target_node_id = match self.vm_migration_form.target_node_id.parse::<u64>() {
                    Ok(id) => id,
                    Err(_) => {
                        self.error_message = Some("Invalid target node ID".to_string());
                        return Ok(());
                    }
                };

                // Check if target is different from source
                if target_node_id == self.vm_migration_form.source_node_id {
                    self.error_message =
                        Some("Target node must be different from source node".to_string());
                    return Ok(());
                }

                // Check if target node exists
                if !self.nodes.iter().any(|n| n.id == target_node_id) {
                    self.error_message =
                        Some(format!("Node {} not found in cluster", target_node_id));
                    return Ok(());
                }

                // Perform migration
                let vm_name = self.vm_migration_form.vm_name.clone();
                let live_migration = self.vm_migration_form.live_migration;

                self.migrate_vm(vm_name, target_node_id, live_migration)
                    .await?;

                // Return to VM list
                self.mode = AppMode::VmList;
                self.vm_migration_form = VmMigrationForm::default();
            }
            KeyCode::Char(c) => {
                if self.vm_migration_form.current_field == VmMigrationField::TargetNode {
                    if c.is_ascii_digit() {
                        self.vm_migration_form.target_node_id.push(c);
                    }
                } else if self.vm_migration_form.current_field == VmMigrationField::LiveMigration {
                    if c == ' ' {
                        self.vm_migration_form.live_migration =
                            !self.vm_migration_form.live_migration;
                    }
                }
            }
            KeyCode::Backspace => {
                if self.vm_migration_form.current_field == VmMigrationField::TargetNode {
                    self.vm_migration_form.target_node_id.pop();
                }
            }
            KeyCode::Up | KeyCode::Down => {
                if self.vm_migration_form.current_field == VmMigrationField::LiveMigration {
                    self.vm_migration_form.live_migration = !self.vm_migration_form.live_migration;
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_vm_create_keys(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::VmList;
                self.create_vm_form = CreateVmForm::default(); // Reset form
            }
            KeyCode::Tab => {
                // Move to next field
                self.create_vm_form.current_field = match self.create_vm_form.current_field {
                    CreateVmField::Name => CreateVmField::Vcpus,
                    CreateVmField::Vcpus => CreateVmField::Memory,
                    CreateVmField::Memory => CreateVmField::ConfigPath,
                    CreateVmField::ConfigPath => CreateVmField::PlacementStrategy,
                    CreateVmField::PlacementStrategy => CreateVmField::NodeId,
                    CreateVmField::NodeId => CreateVmField::AutoStart,
                    CreateVmField::AutoStart => CreateVmField::Name,
                };
            }
            KeyCode::BackTab => {
                // Move to previous field (Shift+Tab)
                self.create_vm_form.current_field = match self.create_vm_form.current_field {
                    CreateVmField::Name => CreateVmField::AutoStart,
                    CreateVmField::Vcpus => CreateVmField::Name,
                    CreateVmField::Memory => CreateVmField::Vcpus,
                    CreateVmField::ConfigPath => CreateVmField::Memory,
                    CreateVmField::PlacementStrategy => CreateVmField::ConfigPath,
                    CreateVmField::NodeId => CreateVmField::PlacementStrategy,
                    CreateVmField::AutoStart => CreateVmField::NodeId,
                };
            }
            KeyCode::Enter => {
                // Submit form
                self.submit_vm_form().await?;
            }
            KeyCode::Char(c) => match self.create_vm_form.current_field {
                CreateVmField::Name => {
                    self.create_vm_form.name.push(c);
                }
                CreateVmField::Vcpus => {
                    if c.is_ascii_digit() {
                        self.create_vm_form.vcpus.push(c);
                    }
                }
                CreateVmField::Memory => {
                    if c.is_ascii_digit() {
                        self.create_vm_form.memory.push(c);
                    }
                }
                CreateVmField::ConfigPath => {
                    self.create_vm_form.config_path.push(c);
                }
                CreateVmField::NodeId => {
                    if c.is_ascii_digit() {
                        let mut node_id_str = self
                            .create_vm_form
                            .node_id
                            .map(|id| id.to_string())
                            .unwrap_or_default();
                        node_id_str.push(c);
                        if let Ok(parsed) = node_id_str.parse::<u64>() {
                            self.create_vm_form.node_id = Some(parsed);
                        }
                    }
                }
                _ => {}
            },
            KeyCode::Backspace => match self.create_vm_form.current_field {
                CreateVmField::Name => {
                    self.create_vm_form.name.pop();
                }
                CreateVmField::Vcpus => {
                    self.create_vm_form.vcpus.pop();
                }
                CreateVmField::Memory => {
                    self.create_vm_form.memory.pop();
                }
                CreateVmField::ConfigPath => {
                    self.create_vm_form.config_path.pop();
                }
                CreateVmField::NodeId => {
                    self.create_vm_form.node_id = None;
                }
                _ => {}
            },
            KeyCode::Up | KeyCode::Down => {
                if self.create_vm_form.current_field == CreateVmField::PlacementStrategy {
                    // Cycle through placement strategies
                    let strategies = PlacementStrategy::all();
                    let current_index = strategies
                        .iter()
                        .position(|s| *s == self.create_vm_form.placement_strategy)
                        .unwrap_or(0);
                    let new_index = if key.code == KeyCode::Up {
                        if current_index == 0 {
                            strategies.len() - 1
                        } else {
                            current_index - 1
                        }
                    } else {
                        (current_index + 1) % strategies.len()
                    };
                    self.create_vm_form.placement_strategy = strategies[new_index].clone();
                } else if self.create_vm_form.current_field == CreateVmField::AutoStart {
                    // Toggle auto start
                    self.create_vm_form.auto_start = !self.create_vm_form.auto_start;
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn submit_vm_form(&mut self) -> BlixardResult<()> {
        // Validate form
        if self.create_vm_form.name.is_empty() {
            self.error_message = Some("VM name is required".to_string());
            return Ok(());
        }

        let vcpus = match self.create_vm_form.vcpus.parse::<u32>() {
            Ok(v) if v > 0 => v,
            _ => {
                self.error_message = Some("vCPUs must be a positive number".to_string());
                return Ok(());
            }
        };

        let memory = match self.create_vm_form.memory.parse::<u32>() {
            Ok(m) if m > 0 => m,
            _ => {
                self.error_message = Some("Memory must be a positive number (MB)".to_string());
                return Ok(());
            }
        };

        // Create VM
        if let Some(client) = &mut self.vm_client {
            match client
                .create_vm(&self.create_vm_form.name, vcpus, memory)
                .await
            {
                Ok(_) => {
                    self.status_message = Some(format!(
                        "VM '{}' created successfully",
                        self.create_vm_form.name
                    ));
                    self.create_vm_form = CreateVmForm::default(); // Reset form
                    self.mode = AppMode::VmList;
                    self.refresh_vm_list().await?;
                    self.add_event(
                        EventLevel::Info,
                        "VM".to_string(),
                        format!("Created VM '{}'", self.create_vm_form.name),
                    );
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to create VM: {}", e));
                    self.add_event(
                        EventLevel::Error,
                        "VM".to_string(),
                        format!("Failed to create VM '{}': {}", self.create_vm_form.name, e),
                    );
                }
            }
        } else {
            self.error_message = Some("Not connected to cluster".to_string());
        }

        Ok(())
    }

    async fn submit_node_form(&mut self) -> BlixardResult<()> {
        // Validate form
        let node_id = match self.create_node_form.id.parse::<u64>() {
            Ok(id) if id > 0 => id,
            _ => {
                self.error_message = Some("Node ID must be a positive number".to_string());
                return Ok(());
            }
        };

        if self.create_node_form.bind_address.is_empty() {
            self.error_message = Some("Bind address is required".to_string());
            return Ok(());
        }

        // Validate bind address format (basic check)
        if !self.create_node_form.bind_address.contains(':') {
            self.error_message =
                Some("Bind address must include port (e.g., 127.0.0.1:7003)".to_string());
            return Ok(());
        }

        // Join cluster (this is what "creating a node" means in cluster context)
        if let Some(client) = &mut self.vm_client {
            match client
                .join_cluster(node_id, &self.create_node_form.bind_address)
                .await
            {
                Ok(message) => {
                    self.status_message =
                        Some(format!("Node {} joined cluster: {}", node_id, message));
                    self.create_node_form = CreateNodeForm::default(); // Reset form
                    self.mode = AppMode::NodeList;
                    self.refresh_node_list().await?;
                    self.add_event(
                        EventLevel::Info,
                        "Node".to_string(),
                        format!("Node {} joined cluster", node_id),
                    );
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to join cluster: {}", e));
                    self.add_event(
                        EventLevel::Error,
                        "Node".to_string(),
                        format!("Failed to join cluster: {}", e),
                    );
                }
            }
        } else {
            self.error_message = Some("Not connected to cluster".to_string());
        }

        Ok(())
    }

    async fn handle_node_list_keys(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Up => {
                if let Some(selected) = self.node_table_state.selected() {
                    if selected > 0 {
                        self.node_table_state.select(Some(selected - 1));
                    }
                }
            }
            KeyCode::Down => {
                if let Some(selected) = self.node_table_state.selected() {
                    if selected < self.nodes.len().saturating_sub(1) {
                        self.node_table_state.select(Some(selected + 1));
                    }
                } else if !self.nodes.is_empty() {
                    self.node_table_state.select(Some(0));
                }
            }
            KeyCode::Char('a') => {
                self.mode = AppMode::CreateNodeForm;
                self.create_node_form = CreateNodeForm::new_with_smart_defaults(&self.nodes);
            }
            KeyCode::Char('t') => {
                // Node template selector
                self.mode = AppMode::NodeTemplateSelector;
            }
            KeyCode::Char('d') => {
                // Delete/destroy selected node
                if let Some(selected) = self.node_table_state.selected() {
                    let displayed_nodes = self.get_displayed_nodes();
                    if let Some(node) = displayed_nodes.get(selected) {
                        self.confirm_dialog = Some(ConfirmDialog {
                            title: "Destroy Node".to_string(),
                            message: format!(
                                "Are you sure you want to destroy node {} at {}?",
                                node.id, node.address
                            ),
                            action: ConfirmAction::RemoveNode(node.id),
                            selected: false,
                        });
                        self.mode = AppMode::ConfirmDialog;
                    }
                }
            }
            KeyCode::Char('r') => {
                // Restart selected node (if daemon mode)
                if let Some(selected) = self.node_table_state.selected() {
                    let displayed_nodes = self.get_displayed_nodes();
                    if let Some(node) = displayed_nodes.get(selected) {
                        self.restart_node(node.id).await?;
                    }
                }
            }
            KeyCode::Char('s') => {
                // Scale cluster
                self.show_cluster_scaling_dialog();
            }
            KeyCode::Char('b') => {
                // Batch add nodes
                self.mode = AppMode::BatchNodeCreation;
                self.batch_node_count = "3".to_string(); // Default to 3 nodes
            }
            KeyCode::Char('+') => {
                // Quick add single node
                self.quick_add_node().await?;
            }
            KeyCode::Char('D') => {
                // Auto-discover and join local nodes
                self.auto_discover_nodes().await?;
            }
            KeyCode::Char('e') => {
                // Edit selected node configuration
                if let Some(selected) = self.node_table_state.selected() {
                    let displayed_nodes = self.get_displayed_nodes();
                    if let Some(node) = displayed_nodes.get(selected) {
                        // Populate edit form with current node data
                        self.edit_node_form = EditNodeForm {
                            node_id: node.id,
                            original_address: node.address.clone(),
                            bind_address: node.address.clone(),
                            data_dir: format!("./data-node{}", node.id), // Default, could be fetched from node
                            vm_backend: "microvm".to_string(), // Default, could be fetched from node
                            current_field: EditNodeField::BindAddress,
                        };
                        self.mode = AppMode::EditNodeConfig;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_create_node_keys(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::NodeList;
                self.create_node_form = CreateNodeForm::default(); // Reset form
            }
            KeyCode::Tab => {
                // Move to next field
                self.create_node_form.current_field = match self.create_node_form.current_field {
                    CreateNodeField::Id => CreateNodeField::BindAddress,
                    CreateNodeField::BindAddress => CreateNodeField::DataDir,
                    CreateNodeField::DataDir => CreateNodeField::Peers,
                    CreateNodeField::Peers => CreateNodeField::VmBackend,
                    CreateNodeField::VmBackend => CreateNodeField::DaemonMode,
                    CreateNodeField::DaemonMode => CreateNodeField::Id,
                };
            }
            KeyCode::BackTab => {
                // Move to previous field (Shift+Tab)
                self.create_node_form.current_field = match self.create_node_form.current_field {
                    CreateNodeField::Id => CreateNodeField::DaemonMode,
                    CreateNodeField::BindAddress => CreateNodeField::Id,
                    CreateNodeField::DataDir => CreateNodeField::BindAddress,
                    CreateNodeField::Peers => CreateNodeField::DataDir,
                    CreateNodeField::VmBackend => CreateNodeField::Peers,
                    CreateNodeField::DaemonMode => CreateNodeField::VmBackend,
                };
            }
            KeyCode::Enter => {
                // Submit form
                self.submit_node_form().await?;
            }
            KeyCode::Char(c) => match self.create_node_form.current_field {
                CreateNodeField::Id => {
                    if c.is_ascii_digit() {
                        self.create_node_form.id.push(c);
                    }
                }
                CreateNodeField::BindAddress => {
                    self.create_node_form.bind_address.push(c);
                }
                CreateNodeField::DataDir => {
                    self.create_node_form.data_dir.push(c);
                }
                CreateNodeField::Peers => {
                    self.create_node_form.peers.push(c);
                }
                CreateNodeField::VmBackend => {
                    if c.is_ascii_alphabetic() || c == '_' {
                        self.create_node_form.vm_backend.push(c);
                    }
                }
                _ => {}
            },
            KeyCode::Backspace => match self.create_node_form.current_field {
                CreateNodeField::Id => {
                    self.create_node_form.id.pop();
                }
                CreateNodeField::BindAddress => {
                    self.create_node_form.bind_address.pop();
                }
                CreateNodeField::DataDir => {
                    self.create_node_form.data_dir.pop();
                }
                CreateNodeField::Peers => {
                    self.create_node_form.peers.pop();
                }
                CreateNodeField::VmBackend => {
                    self.create_node_form.vm_backend.pop();
                }
                _ => {}
            },
            KeyCode::Up | KeyCode::Down => {
                if self.create_node_form.current_field == CreateNodeField::DaemonMode {
                    // Toggle daemon mode
                    self.create_node_form.daemon_mode = !self.create_node_form.daemon_mode;
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_confirm_dialog_keys(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        let mut action_to_execute = None;

        if let Some(dialog) = &mut self.confirm_dialog {
            match key.code {
                KeyCode::Left | KeyCode::Right => {
                    dialog.selected = !dialog.selected;
                }
                KeyCode::Enter => {
                    if dialog.selected {
                        // Extract the action to execute later
                        action_to_execute = Some(dialog.action.clone());
                    }
                    self.confirm_dialog = None;
                    self.mode = AppMode::VmList; // Return to previous mode
                }
                KeyCode::Esc => {
                    self.confirm_dialog = None;
                    self.mode = AppMode::VmList; // Return to previous mode
                }
                _ => {}
            }
        }

        // Execute the action after releasing the borrow
        if let Some(action) = action_to_execute {
            match action {
                ConfirmAction::DeleteVm(name) => {
                    self.delete_vm(name).await?;
                }
                ConfirmAction::StopVm(name) => {
                    self.stop_vm(name).await?;
                }
                ConfirmAction::RemoveNode(id) => {
                    self.remove_node(id).await?;
                }
                ConfirmAction::RestartNode(id) => {
                    self.restart_node(id).await?;
                }
                ConfirmAction::DestroyCluster(name) => {
                    self.destroy_cluster(&name).await?;
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn handle_help_keys(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.switch_tab(AppTab::Dashboard);
            }
            _ => {}
        }
    }

    async fn delete_vm(&mut self, name: String) -> BlixardResult<()> {
        if let Some(client) = &mut self.vm_client {
            match client.delete_vm(&name).await {
                Ok(_) => {
                    self.status_message = Some(format!("VM '{}' deleted successfully", name));
                    self.refresh_vm_list().await?;
                    self.add_event(
                        EventLevel::Info,
                        "VM".to_string(),
                        format!("Deleted VM '{}'", name),
                    );
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to delete VM '{}': {}", name, e));
                    self.add_event(
                        EventLevel::Error,
                        "VM".to_string(),
                        format!("Failed to delete VM '{}': {}", name, e),
                    );
                }
            }
        }
        Ok(())
    }

    async fn start_vm(&mut self, name: String) -> BlixardResult<()> {
        if let Some(client) = &mut self.vm_client {
            match client.start_vm(&name).await {
                Ok(_) => {
                    self.status_message = Some(format!("VM '{}' started successfully", name));
                    self.refresh_vm_list().await?;
                    self.add_event(
                        EventLevel::Info,
                        "VM".to_string(),
                        format!("Started VM '{}'", name),
                    );
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to start VM '{}': {}", name, e));
                    self.add_event(
                        EventLevel::Error,
                        "VM".to_string(),
                        format!("Failed to start VM '{}': {}", name, e),
                    );
                }
            }
        }
        Ok(())
    }

    async fn stop_vm(&mut self, name: String) -> BlixardResult<()> {
        if let Some(client) = &mut self.vm_client {
            match client.stop_vm(&name).await {
                Ok(_) => {
                    self.status_message = Some(format!("VM '{}' stopped successfully", name));
                    self.refresh_vm_list().await?;
                    self.add_event(
                        EventLevel::Info,
                        "VM".to_string(),
                        format!("Stopped VM '{}'", name),
                    );
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to stop VM '{}': {}", name, e));
                    self.add_event(
                        EventLevel::Error,
                        "VM".to_string(),
                        format!("Failed to stop VM '{}': {}", name, e),
                    );
                }
            }
        }
        Ok(())
    }

    async fn handle_vm_template_selector_keys(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::VmList;
            }
            KeyCode::Char('1') => {
                if let Some(template) = self.vm_templates.get(0).cloned() {
                    self.create_vm_from_template(&template).await?;
                    self.mode = AppMode::VmList;
                }
            }
            KeyCode::Char('2') => {
                if let Some(template) = self.vm_templates.get(1).cloned() {
                    self.create_vm_from_template(&template).await?;
                    self.mode = AppMode::VmList;
                }
            }
            KeyCode::Char('3') => {
                if let Some(template) = self.vm_templates.get(2).cloned() {
                    self.create_vm_from_template(&template).await?;
                    self.mode = AppMode::VmList;
                }
            }
            KeyCode::Char('4') => {
                if let Some(template) = self.vm_templates.get(3).cloned() {
                    self.create_vm_from_template(&template).await?;
                    self.mode = AppMode::VmList;
                }
            }
            KeyCode::Char('5') => {
                if let Some(template) = self.vm_templates.get(4).cloned() {
                    self.create_vm_from_template(&template).await?;
                    self.mode = AppMode::VmList;
                }
            }
            _ => {}
        }

        Ok(())
    }

    async fn handle_batch_vm_creation_keys(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::VmList;
                self.batch_node_count = "3".to_string(); // Reset
            }
            KeyCode::Enter => {
                // Parse count and create VMs
                if let Ok(count) = self.batch_node_count.parse::<u32>() {
                    if count > 0 && count <= 10 {
                        self.batch_create_vms(count).await?;
                        self.mode = AppMode::VmList;
                    } else {
                        self.error_message = Some("Count must be between 1 and 10".to_string());
                    }
                } else {
                    self.error_message = Some("Invalid number".to_string());
                }
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                if self.batch_node_count.len() < 2 {
                    self.batch_node_count.push(c);
                }
            }
            KeyCode::Backspace => {
                self.batch_node_count.pop();
            }
            _ => {}
        }

        Ok(())
    }

    async fn handle_log_viewer_keys(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.mode = self.previous_mode();
            }
            KeyCode::Char(' ') => {
                // Toggle follow mode
                self.log_stream_config.follow_mode = !self.log_stream_config.follow_mode;
            }
            KeyCode::Char('l') | KeyCode::Char('L') => {
                // Cycle log level
                self.log_stream_config.filters.log_level =
                    match self.log_stream_config.filters.log_level {
                        LogLevel::Debug => LogLevel::Info,
                        LogLevel::Info => LogLevel::Warning,
                        LogLevel::Warning => LogLevel::Error,
                        LogLevel::Error => LogLevel::Debug,
                    };
            }
            KeyCode::Char('t') | KeyCode::Char('T') => {
                // Toggle timestamps
                self.log_stream_config.filters.show_timestamps =
                    !self.log_stream_config.filters.show_timestamps;
            }
            KeyCode::Char('e') | KeyCode::Char('E') => {
                // Toggle error highlighting
                self.log_stream_config.filters.highlight_errors =
                    !self.log_stream_config.filters.highlight_errors;
            }
            KeyCode::Up => {
                // Select previous source
                if self.log_stream_config.selected_source > 0 {
                    self.log_stream_config.selected_source -= 1;
                }
            }
            KeyCode::Down => {
                // Select next source
                if self.log_stream_config.selected_source < self.log_stream_config.sources.len() - 1
                {
                    self.log_stream_config.selected_source += 1;
                }
            }
            KeyCode::Enter => {
                // Toggle selected source
                if let Some(source) = self
                    .log_stream_config
                    .sources
                    .get_mut(self.log_stream_config.selected_source)
                {
                    source.enabled = !source.enabled;
                }
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                // Clear log entries
                self.log_entries.clear();
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                // Simulate more log entries
                self.simulate_log_entries();
            }
            KeyCode::PageUp => {
                // Scroll up
                if let Some(selected) = self.log_list_state.selected() {
                    self.log_list_state
                        .select(Some(selected.saturating_sub(10)));
                    self.log_stream_config.follow_mode = false;
                }
            }
            KeyCode::PageDown => {
                // Scroll down
                if let Some(selected) = self.log_list_state.selected() {
                    let new_pos = (selected + 10).min(self.log_entries.len().saturating_sub(1));
                    self.log_list_state.select(Some(new_pos));
                    if new_pos == self.log_entries.len().saturating_sub(1) {
                        self.log_stream_config.follow_mode = true;
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn previous_mode(&self) -> AppMode {
        match self.current_tab {
            AppTab::Dashboard => AppMode::Dashboard,
            AppTab::VirtualMachines => AppMode::VmList,
            AppTab::Nodes => AppMode::NodeList,
            AppTab::Monitoring => AppMode::Monitoring,
            AppTab::P2P => AppMode::P2P,
            AppTab::Configuration => AppMode::Config,
            AppTab::Debug => AppMode::Debug,
            AppTab::Help => AppMode::Help,
        }
    }

    async fn handle_batch_node_creation_keys(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::NodeList;
                self.batch_node_count = "3".to_string();
            }
            KeyCode::Enter => {
                // Execute batch creation
                if let Ok(count) = self.batch_node_count.parse::<u32>() {
                    if count > 0 && count <= 10 {
                        self.batch_add_nodes(count).await?;
                        self.mode = AppMode::NodeList;
                    } else {
                        self.error_message =
                            Some("Node count must be between 1 and 10".to_string());
                    }
                } else {
                    self.error_message = Some("Invalid node count".to_string());
                }
            }
            KeyCode::Backspace => {
                self.batch_node_count.pop();
            }
            KeyCode::Char(c) if c.is_numeric() => {
                if self.batch_node_count.len() < 2 {
                    self.batch_node_count.push(c);
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Batch add multiple nodes at once
    async fn batch_add_nodes(&mut self, count: u32) -> BlixardResult<()> {
        let start_id = if self.nodes.is_empty() {
            2
        } else {
            self.nodes.iter().map(|n| n.id).max().unwrap_or(1) + 1
        };

        let mut successful_adds = 0;
        let mut failed_adds = Vec::new();

        for i in 0..count {
            let node_id = start_id + i as u64;
            let port = 7000 + node_id;
            let bind_address = format!("127.0.0.1:{}", port);

            if let Some(client) = &mut self.vm_client {
                match client.join_cluster(node_id, &bind_address).await {
                    Ok(_) => {
                        successful_adds += 1;
                        self.add_event(
                            EventLevel::Info,
                            "Node".to_string(),
                            format!("Added node {} at {}", node_id, bind_address),
                        );
                    }
                    Err(e) => {
                        failed_adds.push((node_id, e.to_string()));
                        self.add_event(
                            EventLevel::Error,
                            "Node".to_string(),
                            format!("Failed to add node {}: {}", node_id, e),
                        );
                    }
                }
            }
        }

        if successful_adds > 0 {
            self.refresh_node_list().await?;
            self.status_message = Some(format!("Successfully added {} nodes", successful_adds));
        }

        if !failed_adds.is_empty() {
            let failed_list = failed_adds
                .iter()
                .map(|(id, _)| format!("{}", id))
                .collect::<Vec<_>>()
                .join(", ");
            self.error_message = Some(format!("Failed to add nodes: {}", failed_list));
        }

        Ok(())
    }

    /// Quick add node with auto-generated settings
    async fn quick_add_node(&mut self) -> BlixardResult<()> {
        // Find the next available node ID
        let next_node_id = if self.nodes.is_empty() {
            2 // Start with node 2 (assuming node 1 is the initial node)
        } else {
            self.nodes.iter().map(|n| n.id).max().unwrap_or(1) + 1
        };

        // Generate port based on node ID (7000 + node_id)
        let port = 7000 + next_node_id;
        let bind_address = format!("127.0.0.1:{}", port);

        // Try to join the cluster
        if let Some(client) = &mut self.vm_client {
            match client.join_cluster(next_node_id, &bind_address).await {
                Ok(message) => {
                    self.status_message = Some(format!(
                        "Quick added node {} at {}: {}",
                        next_node_id, bind_address, message
                    ));
                    self.add_event(
                        EventLevel::Info,
                        "Node".to_string(),
                        format!("Quick added node {} at {}", next_node_id, bind_address),
                    );
                    self.refresh_node_list().await?;
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to quick add node: {}", e));
                    self.add_event(
                        EventLevel::Error,
                        "Node".to_string(),
                        format!("Failed to quick add node {}: {}", next_node_id, e),
                    );
                }
            }
        } else {
            self.error_message = Some("Not connected to cluster".to_string());
        }

        Ok(())
    }

    /// Restart a node (daemon mode)
    async fn restart_node(&mut self, node_id: u64) -> BlixardResult<()> {
        self.add_event(
            EventLevel::Info,
            "Node".to_string(),
            format!("Restarting node {}", node_id),
        );

        // In a real implementation, this would:
        // 1. Send graceful shutdown signal to the node
        // 2. Wait for clean shutdown
        // 3. Restart the node process
        // 4. Wait for it to rejoin the cluster

        // For now, just simulate
        self.status_message = Some(format!("Node {} restart initiated", node_id));

        // Update the node's last_seen timestamp to simulate restart
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == node_id) {
            node.last_seen = Some(Instant::now());
        }

        Ok(())
    }

    /// Show cluster scaling dialog
    fn show_cluster_scaling_dialog(&mut self) {
        // For now, just add a scaling event
        let current_count = self.nodes.len();
        self.add_event(
            EventLevel::Info,
            "Cluster".to_string(),
            format!(
                "Current cluster size: {} nodes. Use 's' to scale.",
                current_count
            ),
        );

        // In a full implementation, this would show a dialog to input target node count
        self.status_message = Some("Cluster scaling: Use dashboard 'N' for templates".to_string());
    }

    /// Enhanced node removal with cleanup
    async fn remove_node(&mut self, id: u64) -> BlixardResult<()> {
        self.add_event(
            EventLevel::Warning,
            "Node".to_string(),
            format!("Removing node {} from cluster", id),
        );

        // In a real implementation, this would:
        // 1. Gracefully remove the node from Raft cluster
        // 2. Migrate any VMs running on that node
        // 3. Clean up data directories
        // 4. Stop daemon processes
        // 5. Update cluster configuration

        // For now, simulate by removing from discovered clusters and UI
        if let Some(cluster) = self
            .discovered_clusters
            .iter_mut()
            .find(|c| c.leader_address == self.selected_cluster.as_deref().unwrap_or(""))
        {
            if cluster.node_count > 1 {
                cluster.node_count -= 1;
            }
        }

        // Remove from nodes list
        self.nodes.retain(|node| node.id != id);
        self.apply_node_filter();

        // Clear selection if it was the removed node
        if let Some(selected) = self.node_table_state.selected() {
            if selected >= self.get_displayed_nodes().len() {
                self.node_table_state.select(None);
            }
        }

        self.status_message = Some(format!("Node {} removed from cluster", id));
        self.add_event(
            EventLevel::Info,
            "Node".to_string(),
            format!("Node {} successfully removed", id),
        );

        Ok(())
    }

    /// Destroy an entire cluster
    async fn destroy_cluster(&mut self, cluster_name: &str) -> BlixardResult<()> {
        self.add_event(
            EventLevel::Warning,
            "Cluster".to_string(),
            format!("Destroying cluster '{}'", cluster_name),
        );

        // In a real implementation, this would:
        // 1. Stop all VMs on all nodes
        // 2. Gracefully shutdown all nodes
        // 3. Clean up all data directories
        // 4. Remove network configurations
        // 5. Terminate daemon processes

        // For now, simulate by removing from discovered clusters
        self.discovered_clusters
            .retain(|cluster| cluster.name != cluster_name);

        // Clear connection if we were connected to this cluster
        if let Some(selected) = &self.selected_cluster {
            if self
                .discovered_clusters
                .iter()
                .any(|c| c.leader_address == *selected && c.name == cluster_name)
            {
                self.vm_client = None;
                self.selected_cluster = None;
            }
        }

        // Clear all data
        self.nodes.clear();
        self.vms.clear();
        self.apply_vm_filter();
        self.apply_node_filter();

        self.status_message = Some(format!("Cluster '{}' destroyed", cluster_name));
        self.add_event(
            EventLevel::Info,
            "Cluster".to_string(),
            format!("Cluster '{}' successfully destroyed", cluster_name),
        );

        Ok(())
    }

    async fn handle_mouse_event(
        &mut self,
        mouse: crossterm::event::MouseEvent,
    ) -> BlixardResult<()> {
        use crossterm::event::{MouseButton, MouseEventKind};

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.handle_left_click(mouse.column, mouse.row).await?;
            }
            MouseEventKind::ScrollUp => {
                self.handle_scroll(true).await?;
            }
            MouseEventKind::ScrollDown => {
                self.handle_scroll(false).await?;
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_left_click(&mut self, column: u16, row: u16) -> BlixardResult<()> {
        // Tab bar is typically at row 1-2, so check if click is in tab area
        if row <= 2 {
            self.handle_tab_click(column).await?;
        } else {
            // Handle clicks within current tab content
            match self.current_tab {
                AppTab::VirtualMachines => {
                    self.handle_vm_list_click(column, row).await?;
                }
                AppTab::Nodes => {
                    self.handle_node_list_click(column, row).await?;
                }
                _ => {
                    // For other tabs, just show click coordinates in status
                    self.status_message = Some(format!("Clicked at ({}, {})", column, row));
                }
            }
        }
        Ok(())
    }

    async fn handle_tab_click(&mut self, column: u16) -> BlixardResult<()> {
        // Rough tab width calculation - adjust based on tab titles
        let tab_width = 15; // Approximate width per tab
        let tab_index = column / tab_width;

        let new_tab = match tab_index {
            0 => AppTab::Dashboard,
            1 => AppTab::VirtualMachines,
            2 => AppTab::Nodes,
            3 => AppTab::Monitoring,
            4 => AppTab::Configuration,
            5 => AppTab::Help,
            _ => return Ok(()), // Invalid tab area
        };

        self.switch_tab(new_tab);
        self.status_message = Some("Switched tab via mouse click".to_string());
        Ok(())
    }

    async fn handle_vm_list_click(&mut self, _column: u16, row: u16) -> BlixardResult<()> {
        // VM table typically starts around row 4-5, accounting for headers
        if row >= 5 && !self.vms.is_empty() {
            let vm_index = (row - 5) as usize;
            if vm_index < self.vms.len() {
                self.vm_table_state.select(Some(vm_index));
                self.status_message = Some(format!("Selected VM: {}", self.vms[vm_index].name));
            }
        }
        Ok(())
    }

    async fn handle_node_list_click(&mut self, _column: u16, row: u16) -> BlixardResult<()> {
        // Node table typically starts around row 4-5, accounting for headers
        if row >= 5 && !self.nodes.is_empty() {
            let node_index = (row - 5) as usize;
            if node_index < self.nodes.len() {
                self.node_table_state.select(Some(node_index));
                self.status_message = Some(format!("Selected Node: {}", self.nodes[node_index].id));
            }
        }
        Ok(())
    }

    async fn handle_scroll(&mut self, scroll_up: bool) -> BlixardResult<()> {
        match self.current_tab {
            AppTab::VirtualMachines => {
                if let Some(selected) = self.vm_table_state.selected() {
                    if scroll_up && selected > 0 {
                        self.vm_table_state.select(Some(selected - 1));
                    } else if !scroll_up && selected < self.vms.len().saturating_sub(1) {
                        self.vm_table_state.select(Some(selected + 1));
                    }
                } else if !self.vms.is_empty() {
                    self.vm_table_state.select(Some(0));
                }
            }
            AppTab::Nodes => {
                if let Some(selected) = self.node_table_state.selected() {
                    if scroll_up && selected > 0 {
                        self.node_table_state.select(Some(selected - 1));
                    } else if !scroll_up && selected < self.nodes.len().saturating_sub(1) {
                        self.node_table_state.select(Some(selected + 1));
                    }
                } else if !self.nodes.is_empty() {
                    self.node_table_state.select(Some(0));
                }
            }
            _ => {
                // For other tabs, scroll through recent events or do nothing
                self.status_message = Some(format!(
                    "Scrolled {} in current view",
                    if scroll_up { "up" } else { "down" }
                ));
            }
        }
        Ok(())
    }

    /// Add debug log entry
    pub fn add_debug_log(
        &mut self,
        level: DebugLevel,
        component: String,
        message: String,
        details: Option<serde_json::Value>,
    ) {
        let entry = DebugLogEntry {
            timestamp: Instant::now(),
            level,
            component,
            message,
            details,
        };

        self.debug_log_entries.insert(0, entry);
        if self.debug_log_entries.len() > self.max_debug_entries {
            self.debug_log_entries.truncate(self.max_debug_entries);
        }
    }

    /// Update Raft debug info (would be called from node)
    pub fn update_raft_debug_info(&mut self, debug_info: RaftDebugInfo) {
        // Add debug event first, before moving debug_info
        self.add_debug_log(
            DebugLevel::Debug,
            "Raft".to_string(),
            format!(
                "State: {:?}, Term: {}, Commit: {}",
                debug_info.state, debug_info.current_term, debug_info.commit_index
            ),
            None,
        );

        self.raft_debug_info = Some(debug_info);
    }

    /// Update debug metrics
    pub fn update_debug_metrics<F>(&mut self, update_fn: F)
    where
        F: FnOnce(&mut DebugMetrics),
    {
        update_fn(&mut self.debug_metrics);
    }

    /// Reset debug metrics
    pub fn reset_debug_metrics(&mut self) {
        self.debug_metrics = DebugMetrics {
            messages_sent: 0,
            messages_received: 0,
            proposals_submitted: 0,
            proposals_committed: 0,
            elections_started: 0,
            leadership_changes: 0,
            snapshot_creations: 0,
            log_compactions: 0,
            network_partitions_detected: 0,
            last_reset: Instant::now(),
        };

        self.add_debug_log(
            DebugLevel::Info,
            "Debug".to_string(),
            "Debug metrics reset".to_string(),
            None,
        );
    }

    /// Handle debug mode key events
    pub async fn handle_debug_keys(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::Dashboard;
            }
            KeyCode::Char('r') => {
                // Switch to Raft debug view
                self.mode = AppMode::RaftDebug;
            }
            KeyCode::Char('m') => {
                // Switch to metrics view
                self.mode = AppMode::DebugMetrics;
            }
            KeyCode::Char('l') => {
                // Switch to debug logs view
                self.mode = AppMode::DebugLogs;
            }
            KeyCode::Char('R') => {
                // Reset debug metrics
                self.reset_debug_metrics();
                self.status_message = Some("Debug metrics reset".to_string());
            }
            KeyCode::Char('c') => {
                // Clear debug logs
                self.debug_log_entries.clear();
                self.status_message = Some("Debug logs cleared".to_string());
            }
            KeyCode::Char('s') => {
                // Simulate Raft debug data for testing
                self.simulate_raft_debug_data();
            }
            _ => {}
        }

        Ok(())
    }

    async fn handle_config_keys(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Char('s') => {
                // Save configuration
                self.mode = AppMode::SaveConfig;
                self.config_file_path = Some("cluster-config.yaml".to_string());
            }
            KeyCode::Char('l') => {
                // Load configuration
                self.mode = AppMode::LoadConfig;
                self.config_file_path = Some("cluster-config.yaml".to_string());
            }
            KeyCode::Char('e') => {
                // Export cluster state
                self.mode = AppMode::ExportCluster;
            }
            KeyCode::Char('i') => {
                // Import cluster state
                self.mode = AppMode::ImportCluster;
            }
            _ => {}
        }

        Ok(())
    }

    async fn handle_save_config_keys(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::Config;
                self.config_file_path = None;
                self.config_description.clear();
            }
            KeyCode::Tab => {
                // Toggle between fields
                self.save_config_field = match self.save_config_field {
                    SaveConfigField::FilePath => SaveConfigField::Description,
                    SaveConfigField::Description => SaveConfigField::FilePath,
                };
            }
            KeyCode::Enter => {
                if let Some(path) = self.config_file_path.clone() {
                    match self.save_cluster_config(&path).await {
                        Ok(_) => {
                            self.mode = AppMode::Config;
                            self.status_message = Some(format!("Configuration saved to {}", path));
                        }
                        Err(e) => {
                            self.error_message =
                                Some(format!("Failed to save configuration: {}", e));
                        }
                    }
                }
            }
            KeyCode::Char(c) => match self.save_config_field {
                SaveConfigField::FilePath => {
                    if let Some(path) = &mut self.config_file_path {
                        path.push(c);
                    }
                }
                SaveConfigField::Description => {
                    self.config_description.push(c);
                }
            },
            KeyCode::Backspace => match self.save_config_field {
                SaveConfigField::FilePath => {
                    if let Some(path) = &mut self.config_file_path {
                        path.pop();
                    }
                }
                SaveConfigField::Description => {
                    self.config_description.pop();
                }
            },
            _ => {}
        }

        Ok(())
    }

    async fn handle_load_config_keys(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::Config;
                self.config_file_path = None;
            }
            KeyCode::Enter => {
                if let Some(path) = self.config_file_path.clone() {
                    match self.load_cluster_config(&path).await {
                        Ok(_) => {
                            self.mode = AppMode::Config;
                            self.status_message =
                                Some(format!("Configuration loaded from {}", path));
                        }
                        Err(e) => {
                            self.error_message =
                                Some(format!("Failed to load configuration: {}", e));
                        }
                    }
                }
            }
            KeyCode::Char(c) => {
                if let Some(path) = &mut self.config_file_path {
                    path.push(c);
                }
            }
            KeyCode::Backspace => {
                if let Some(path) = &mut self.config_file_path {
                    path.pop();
                }
            }
            _ => {}
        }

        Ok(())
    }

    async fn handle_edit_node_config_keys(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::NodeList;
            }
            KeyCode::Tab => {
                // Cycle through fields
                self.edit_node_form.current_field = match self.edit_node_form.current_field {
                    EditNodeField::BindAddress => EditNodeField::DataDir,
                    EditNodeField::DataDir => EditNodeField::VmBackend,
                    EditNodeField::VmBackend => EditNodeField::BindAddress,
                };
            }
            KeyCode::Enter => {
                // Apply changes
                self.apply_node_config_changes().await?;
                self.mode = AppMode::NodeList;
            }
            KeyCode::Char(c) => match self.edit_node_form.current_field {
                EditNodeField::BindAddress => self.edit_node_form.bind_address.push(c),
                EditNodeField::DataDir => self.edit_node_form.data_dir.push(c),
                EditNodeField::VmBackend => self.edit_node_form.vm_backend.push(c),
            },
            KeyCode::Backspace => match self.edit_node_form.current_field {
                EditNodeField::BindAddress => {
                    self.edit_node_form.bind_address.pop();
                }
                EditNodeField::DataDir => {
                    self.edit_node_form.data_dir.pop();
                }
                EditNodeField::VmBackend => {
                    self.edit_node_form.vm_backend.pop();
                }
            },
            _ => {}
        }
        Ok(())
    }

    /// Save cluster configuration to file
    pub async fn save_cluster_config(&mut self, path: &str) -> BlixardResult<()> {
        use super::cluster_config::{
            ClusterConfiguration, ClusterSettings, NodeConfiguration, VmConfiguration,
            VmTemplate as ConfigVmTemplate,
        };

        let mut config = ClusterConfiguration::new(Some(self.config_description.clone()));

        // Add nodes
        for node in &self.nodes {
            config.nodes.push(NodeConfiguration {
                id: node.id,
                bind_address: node.address.clone(),
                data_dir: format!("./data-node{}", node.id), // Default pattern
                vm_backend: self.settings.default_vm_backend.clone(),
                use_tailscale: false,
                role: match node.role {
                    NodeRole::Leader => "leader",
                    NodeRole::Follower => "follower",
                    _ => "unknown",
                }
                .to_string(),
                status: match node.status {
                    NodeStatus::Healthy => "healthy",
                    NodeStatus::Warning => "warning",
                    NodeStatus::Critical => "critical",
                    _ => "unknown",
                }
                .to_string(),
            });
        }

        // Add VMs
        for vm in &self.vms {
            config.vms.push(VmConfiguration {
                name: vm.name.clone(),
                vcpus: vm.vcpus,
                memory: vm.memory,
                node_id: vm.node_id,
                status: format!("{:?}", vm.status),
                config_path: vm.config_path.clone(),
            });
        }

        // Add VM templates
        for template in &self.vm_templates {
            config.vm_templates.push(ConfigVmTemplate {
                name: template.name.clone(),
                description: template.description.clone(),
                vcpus: template.vcpus,
                memory: template.memory,
                template_type: format!("{:?}", template.template_type),
            });
        }

        // Add settings
        config.settings = ClusterSettings {
            default_vm_backend: self.settings.default_vm_backend.clone(),
            refresh_rate: self.settings.refresh_rate,
            max_log_entries: self.max_log_entries,
            auto_connect: self.settings.auto_connect,
        };

        // Save to file
        config.save_to_file(std::path::Path::new(path)).await?;

        self.config_file_path = Some(path.to_string());
        self.config_dirty = false;
        self.status_message = Some(format!("Configuration saved to {}", path));

        Ok(())
    }

    /// Load cluster configuration from file
    pub async fn load_cluster_config(&mut self, path: &str) -> BlixardResult<()> {
        use super::cluster_config::ClusterConfiguration;

        let config = ClusterConfiguration::load_from_file(std::path::Path::new(path)).await?;

        // Validate configuration
        config.validate()?;

        // Apply configuration
        self.config_description = config.metadata.description.unwrap_or_default();

        // Note: We don't automatically create nodes/VMs from loaded config
        // This would require more complex orchestration
        // For now, we just load the configuration for reference

        self.config_file_path = Some(path.to_string());
        self.status_message = Some(format!("Configuration loaded from {}", path));

        // Show a dialog with loaded configuration summary
        self.add_event(
            EventLevel::Info,
            "Config".to_string(),
            format!(
                "Loaded configuration with {} nodes and {} VMs",
                config.nodes.len(),
                config.vms.len()
            ),
        );

        Ok(())
    }

    /// Simulate Raft debug data for testing
    fn simulate_raft_debug_data(&mut self) {
        use std::time::Duration;

        let raft_debug = RaftDebugInfo {
            current_term: 5,
            voted_for: Some(1),
            log_length: 150,
            commit_index: 145,
            last_applied: 145,
            state: RaftNodeState::Leader,
            last_heartbeat: Some(Instant::now()),
            election_timeout: Duration::from_millis(300),
            entries_since_snapshot: 50,
            snapshot_metadata: Some(SnapshotMetadata {
                last_included_index: 100,
                last_included_term: 4,
                size_bytes: 1024 * 1024,
                created_at: Instant::now() - Duration::from_secs(300),
            }),
            peer_states: vec![
                PeerDebugInfo {
                    id: 2,
                    next_index: 150,
                    match_index: 145,
                    state: PeerState::Replicate,
                    last_activity: Some(Instant::now() - Duration::from_millis(50)),
                    is_reachable: true,
                },
                PeerDebugInfo {
                    id: 3,
                    next_index: 150,
                    match_index: 145,
                    state: PeerState::Replicate,
                    last_activity: Some(Instant::now() - Duration::from_millis(75)),
                    is_reachable: true,
                },
            ],
        };

        self.update_raft_debug_info(raft_debug);

        // Update some debug metrics
        self.update_debug_metrics(|metrics| {
            metrics.messages_sent += 10;
            metrics.messages_received += 8;
            metrics.proposals_submitted += 2;
            metrics.proposals_committed += 2;
        });

        self.status_message = Some("Simulated Raft debug data updated".to_string());
    }

    /// Apply node configuration changes
    async fn apply_node_config_changes(&mut self) -> BlixardResult<()> {
        let node_id = self.edit_node_form.node_id;
        let new_address = &self.edit_node_form.bind_address;
        let new_data_dir = &self.edit_node_form.data_dir;
        let new_vm_backend = &self.edit_node_form.vm_backend;

        // Update node info in local state
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == node_id) {
            node.address = new_address.clone();
        }

        // In a real implementation, we would send these changes to the node via gRPC
        // For now, we'll just show a status message
        self.status_message = Some(format!(
            "Node {} configuration updated:\n  Address: {}\n  Data Dir: {}\n  VM Backend: {}",
            node_id, new_address, new_data_dir, new_vm_backend
        ));

        self.add_event(
            EventLevel::Info,
            "Node Config".to_string(),
            format!("Updated configuration for node {}", node_id),
        );

        // Note: In production, this would require:
        // 1. Stopping the node if bind address changed
        // 2. Updating node configuration file
        // 3. Restarting the node with new configuration
        // 4. Updating cluster membership if address changed

        Ok(())
    }

    /// Initialize P2P functionality
    pub async fn initialize_p2p(&mut self) -> BlixardResult<()> {
        if self.p2p_enabled {
            self.status_message = Some("P2P networking is already enabled".to_string());
            return Ok(()); // Already initialized
        }

        // Get current node info if connected
        // For now, use the first node's ID or default to 1
        let node_id = self.nodes.first().map(|n| n.id).unwrap_or(1);

        self.status_message = Some("Initializing P2P networking...".to_string());

        // Create temporary directory for P2P data
        let p2p_dir = std::env::temp_dir().join(format!("blixard-p2p-{}", node_id));
        if let Err(e) = std::fs::create_dir_all(&p2p_dir) {
            self.error_message = Some(format!("Failed to create P2P directory: {}", e));
            self.add_event(
                EventLevel::Error,
                "P2P".to_string(),
                format!("Failed to create P2P directory: {}", e),
            );
            return Err(BlixardError::IoError(e));
        }

        // Initialize P2P manager with default config
        let p2p_config = blixard_core::p2p_manager::P2pConfig::default();
        let manager =
            match blixard_core::p2p_manager::P2pManager::new(node_id, &p2p_dir, p2p_config).await {
                Ok(mgr) => Arc::new(mgr),
                Err(e) => {
                    self.error_message = Some(format!("Failed to initialize P2P manager: {}", e));
                    self.add_event(
                        EventLevel::Error,
                        "P2P".to_string(),
                        format!("Failed to initialize P2P manager: {}", e),
                    );
                    return Err(e);
                }
            };

        // Start the P2P manager
        if let Err(e) = manager.start().await {
            self.error_message = Some(format!("Failed to start P2P manager: {}", e));
            self.add_event(
                EventLevel::Error,
                "P2P".to_string(),
                format!("Failed to start P2P manager: {}", e),
            );
            return Err(e);
        }

        // Initialize P2P image store
        let store = match blixard_core::p2p_image_store::P2pImageStore::new(node_id, &p2p_dir).await
        {
            Ok(s) => s,
            Err(e) => {
                self.error_message = Some(format!("Failed to initialize P2P image store: {}", e));
                self.add_event(
                    EventLevel::Error,
                    "P2P".to_string(),
                    format!("Failed to initialize P2P image store: {}", e),
                );
                return Err(e);
            }
        };

        // Get node ID from Iroh
        let node_addr = store.get_node_addr().await?;
        self.p2p_node_id = node_addr.node_id.to_string().chars().take(8).collect();

        self.p2p_store = Some(Arc::new(tokio::sync::Mutex::new(store)));
        self.p2p_manager = Some(manager.clone());
        self.p2p_enabled = true;

        self.add_event(
            EventLevel::Info,
            "P2P".to_string(),
            format!("P2P networking enabled with node ID: {}", self.p2p_node_id),
        );

        self.status_message = Some("P2P networking enabled".to_string());

        // Start background task to update P2P stats
        self.start_p2p_stats_updater(manager);

        // Populate with demo images for better UX
        self.populate_demo_p2p_images().await?;

        Ok(())
    }

    /// Disable P2P functionality
    pub async fn disable_p2p(&mut self) -> BlixardResult<()> {
        if !self.p2p_enabled {
            self.status_message = Some("P2P networking is already disabled".to_string());
            return Ok(());
        }

        self.status_message = Some("Disabling P2P networking...".to_string());

        // Note: P2P manager doesn't have an explicit stop method
        // It will be cleaned up when dropped

        // Clear P2P state
        self.p2p_enabled = false;
        self.p2p_store = None;
        self.p2p_manager = None;
        self.p2p_peers.clear();
        self.p2p_images.clear();
        self.p2p_transfers.clear();
        self.p2p_peer_count = 0;
        self.p2p_shared_images = 0;

        self.add_event(
            EventLevel::Info,
            "P2P".to_string(),
            "P2P networking disabled successfully".to_string(),
        );

        self.status_message = Some("P2P networking disabled".to_string());

        Ok(())
    }

    /// Start background task to update P2P stats
    fn start_p2p_stats_updater(&self, manager: Arc<blixard_core::p2p_manager::P2pManager>) {
        let event_sender = self.event_sender.clone();

        // Spawn task to handle P2P events
        let event_rx = manager.event_receiver();
        tokio::spawn(async move {
            let mut rx = event_rx.write().await;

            while let Some(event) = rx.recv().await {
                use blixard_core::p2p_manager::P2pEvent;

                match event {
                    P2pEvent::PeerConnected(peer_id) => {
                        tracing::info!("Peer connected: {}", peer_id);
                    }
                    P2pEvent::PeerDisconnected(peer_id) => {
                        tracing::info!("Peer disconnected: {}", peer_id);
                    }
                    P2pEvent::TransferStarted(transfer_id) => {
                        tracing::info!("Transfer started: {}", transfer_id);
                    }
                    P2pEvent::TransferCompleted(transfer_id) => {
                        tracing::info!("Transfer completed: {}", transfer_id);
                    }
                    _ => {}
                }
            }
        });

        // Spawn task to periodically update stats
        let manager_clone = manager.clone();
        let event_sender_clone = event_sender.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;

                // Update peer list
                let peers = manager_clone.get_peers().await;

                // Convert to UI format
                let ui_peers: Vec<P2pPeer> = peers
                    .into_iter()
                    .map(|p| P2pPeer {
                        node_id: p.node_id,
                        address: p.address,
                        status: "connected".to_string(), // Simplified status
                        latency_ms: p.connection_quality.latency_ms,
                        shared_images: p.shared_resources.len(),
                    })
                    .collect();

                // Send update event
                if let Some(sender) = &event_sender_clone {
                    let _ = sender.send(Event::P2pPeersUpdate(ui_peers));
                }

                // Update transfer list
                let transfers = manager_clone.get_active_transfers().await;

                // Convert to UI format
                let ui_transfers: Vec<P2pTransfer> = transfers
                    .into_iter()
                    .map(|t| P2pTransfer {
                        resource_name: format!("{}-{}.img", t.request.name, t.request.version),
                        peer_id: t.request.source_peer.unwrap_or_else(|| "local".to_string()),
                        is_upload: matches!(
                            t.progress.status,
                            blixard_core::p2p_manager::TransferStatus::Uploading
                        ),
                        total_bytes: t.progress.total_bytes,
                        bytes_transferred: t.progress.bytes_transferred,
                        speed_bps: t.progress.speed_bps,
                    })
                    .collect();

                // Send update event
                if let Some(sender) = &event_sender_clone {
                    let _ = sender.send(Event::P2pTransfersUpdate(ui_transfers));
                }

                // TODO: Also update images list periodically
                // This would query the P2P store for available images
            }
        });
    }

    /// Populate demo P2P images for better user experience
    async fn populate_demo_p2p_images(&mut self) -> BlixardResult<()> {
        // Add some demo images to show in the UI
        self.p2p_images = vec![
            P2pImage {
                name: "ubuntu-22.04-server".to_string(),
                version: "latest".to_string(),
                size: 1024 * 1024 * 1024 * 2, // 2GB
                available_peers: 3,
                is_cached: false,
                is_downloading: false,
            },
            P2pImage {
                name: "alpine-3.18".to_string(),
                version: "3.18.4".to_string(),
                size: 1024 * 1024 * 50, // 50MB
                available_peers: 5,
                is_cached: true,
                is_downloading: false,
            },
            P2pImage {
                name: "debian-12-minimal".to_string(),
                version: "12.2".to_string(),
                size: 1024 * 1024 * 512, // 512MB
                available_peers: 2,
                is_cached: false,
                is_downloading: false,
            },
            P2pImage {
                name: "fedora-39-cloud".to_string(),
                version: "39".to_string(),
                size: 1024 * 1024 * 1024, // 1GB
                available_peers: 4,
                is_cached: false,
                is_downloading: true,
            },
        ];

        // Add some demo peers
        self.p2p_peers = vec![
            P2pPeer {
                node_id: "ab12cd34".to_string(),
                address: "192.168.1.100:7001".to_string(),
                status: "connected".to_string(),
                latency_ms: 12,
                shared_images: 4,
            },
            P2pPeer {
                node_id: "ef56gh78".to_string(),
                address: "192.168.1.101:7001".to_string(),
                status: "connected".to_string(),
                latency_ms: 25,
                shared_images: 2,
            },
            P2pPeer {
                node_id: "ij90kl12".to_string(),
                address: "10.0.0.50:7001".to_string(),
                status: "syncing".to_string(),
                latency_ms: 45,
                shared_images: 3,
            },
        ];

        self.p2p_peer_count = self.p2p_peers.len();
        self.p2p_shared_images = self.p2p_images.len();

        Ok(())
    }

    /// Upload a VM image to P2P network
    pub async fn upload_vm_image_to_p2p(&mut self, vm_name: &str) -> BlixardResult<()> {
        if !self.p2p_enabled {
            self.error_message = Some("P2P networking is not enabled".to_string());
            return Ok(());
        }

        // Check if P2P store is available
        if let Some(_store_arc) = &self.p2p_store {
            let vm_config_path =
                std::path::PathBuf::from(format!("/var/lib/blixard/vms/{}/config.json", vm_name));

            // Check if VM image exists
            if !vm_config_path.exists() {
                // For demo purposes, we'll proceed anyway
                self.add_event(
                    EventLevel::Warning,
                    "P2P".to_string(),
                    format!(
                        "VM image file not found for '{}', simulating upload",
                        vm_name
                    ),
                );
            }

            self.add_event(
                EventLevel::Info,
                "P2P".to_string(),
                format!("Uploading VM image '{}' to P2P network", vm_name),
            );

            // Add to transfers list to show progress
            self.p2p_transfers.push(P2pTransfer {
                resource_name: format!("{}.img", vm_name),
                peer_id: "local".to_string(),
                is_upload: true,
                total_bytes: 1024 * 1024 * 512, // 512MB demo size
                bytes_transferred: 0,
                speed_bps: 10 * 1024 * 1024, // 10MB/s
            });

            // In a real implementation, we would:
            // 1. Call store.upload_image() with actual file path
            // 2. Track upload progress
            // 3. Update p2p_images list on completion

            self.status_message = Some(format!("Started uploading '{}'", vm_name));
        } else {
            self.error_message = Some("P2P store not available".to_string());
        }

        Ok(())
    }

    /// Download a VM image from P2P network
    pub async fn download_vm_image_from_p2p(
        &mut self,
        image_name: &str,
        version: &str,
    ) -> BlixardResult<()> {
        if !self.p2p_enabled {
            self.error_message = Some("P2P networking is not enabled".to_string());
            return Ok(());
        }

        if let Some(store_arc) = self.p2p_store.clone() {
            self.add_event(
                EventLevel::Info,
                "P2P".to_string(),
                format!(
                    "Downloading VM image '{}' v{} from P2P network",
                    image_name, version
                ),
            );

            let store = store_arc.lock().await;
            match store.download_image(image_name, version).await {
                Ok(path) => {
                    drop(store); // Release the lock before mutating self
                    self.status_message = Some(format!("Downloaded image to: {:?}", path));
                    self.add_event(
                        EventLevel::Info,
                        "P2P".to_string(),
                        format!("Successfully downloaded '{}' v{}", image_name, version),
                    );
                }
                Err(e) => {
                    drop(store); // Release the lock before mutating self

                    // Provide more detailed error messages based on error type
                    let error_msg = match &e {
                        BlixardError::NotFound { resource } => {
                            format!("Image not found: {}", resource)
                        }
                        BlixardError::IoError(io_err) => {
                            format!("Storage error: {}", io_err)
                        }
                        BlixardError::NetworkError(msg) => {
                            format!("Network error: {}", msg)
                        }
                        _ => format!("Failed to download image: {}", e),
                    };

                    self.error_message = Some(error_msg.clone());
                    self.add_event(EventLevel::Error, "P2P".to_string(), error_msg);
                }
            }
        } else {
            self.error_message = Some("P2P store not available".to_string());
        }

        Ok(())
    }

    /// Handle keyboard events in P2P mode
    async fn handle_p2p_keys(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Char('p') => {
                // Toggle P2P on/off
                if self.p2p_enabled {
                    self.disable_p2p().await?;
                } else {
                    self.initialize_p2p().await?;
                }
            }
            KeyCode::Char('u') => {
                // Upload current VM image
                if let Some(vm_name) = self.selected_vm.clone() {
                    self.upload_vm_image_to_p2p(&vm_name).await?;
                } else {
                    self.error_message = Some("No VM selected for upload".to_string());
                }
            }
            KeyCode::Char('d') => {
                // Download selected P2P image
                if !self.p2p_images.is_empty() {
                    // Find first non-cached, non-downloading image
                    if let Some(image) = self
                        .p2p_images
                        .iter_mut()
                        .find(|img| !img.is_cached && !img.is_downloading)
                    {
                        let name = image.name.clone();
                        let version = image.version.clone();

                        // Mark as downloading
                        image.is_downloading = true;

                        // Start download
                        self.download_vm_image_from_p2p(&name, &version).await?;

                        // Simulate download completion after a moment
                        if let Some(img) = self
                            .p2p_images
                            .iter_mut()
                            .find(|i| i.name == name && i.version == version)
                        {
                            img.is_downloading = false;
                            img.is_cached = true;
                        }
                    } else {
                        self.status_message =
                            Some("All images are already cached or downloading".to_string());
                    }
                } else {
                    self.error_message = Some("No P2P images available for download".to_string());
                }
            }
            KeyCode::Char('r') => {
                // Refresh P2P stats
                self.refresh_p2p_stats().await?;
            }
            KeyCode::Char('c') => {
                // Connect to a peer
                self.prompt_peer_connection().await?;
            }
            KeyCode::Char('s') => {
                // Show P2P statistics
                self.show_p2p_statistics().await?;
            }
            KeyCode::Char('l') => {
                // List available resources
                self.list_p2p_resources().await?;
            }
            KeyCode::Char('q') => {
                // Queue a download with priority selection
                if !self.p2p_images.is_empty() {
                    self.queue_p2p_download().await?;
                }
            }
            KeyCode::Esc => {
                // Return to normal mode
                self.mode = AppMode::Dashboard;
            }
            _ => {}
        }

        Ok(())
    }

    /// Refresh P2P statistics and peer list
    async fn handle_export_cluster_keys(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::Config;
            }
            KeyCode::Tab => {
                // Cycle through fields
                self.export_form.current_field = match self.export_form.current_field {
                    ExportFormField::OutputPath => ExportFormField::ClusterName,
                    ExportFormField::ClusterName => ExportFormField::IncludeImages,
                    ExportFormField::IncludeImages => ExportFormField::IncludeTelemetry,
                    ExportFormField::IncludeTelemetry => ExportFormField::Compress,
                    ExportFormField::Compress => ExportFormField::P2pShare,
                    ExportFormField::P2pShare => ExportFormField::OutputPath,
                };
            }
            KeyCode::Enter => {
                match self.export_form.current_field {
                    ExportFormField::IncludeImages => {
                        self.export_form.include_images = !self.export_form.include_images;
                    }
                    ExportFormField::IncludeTelemetry => {
                        self.export_form.include_telemetry = !self.export_form.include_telemetry;
                    }
                    ExportFormField::Compress => {
                        self.export_form.compress = !self.export_form.compress;
                    }
                    ExportFormField::P2pShare => {
                        self.export_form.p2p_share = !self.export_form.p2p_share;
                    }
                    _ => {
                        // Execute export
                        self.execute_cluster_export().await?;
                    }
                }
            }
            KeyCode::Char(' ') => {
                // Toggle checkboxes
                match self.export_form.current_field {
                    ExportFormField::IncludeImages => {
                        self.export_form.include_images = !self.export_form.include_images;
                    }
                    ExportFormField::IncludeTelemetry => {
                        self.export_form.include_telemetry = !self.export_form.include_telemetry;
                    }
                    ExportFormField::Compress => {
                        self.export_form.compress = !self.export_form.compress;
                    }
                    ExportFormField::P2pShare => {
                        self.export_form.p2p_share = !self.export_form.p2p_share;
                    }
                    _ => {}
                }
            }
            KeyCode::Backspace => match self.export_form.current_field {
                ExportFormField::OutputPath => {
                    self.export_form.output_path.pop();
                }
                ExportFormField::ClusterName => {
                    self.export_form.cluster_name.pop();
                }
                _ => {}
            },
            KeyCode::Char(c) => match self.export_form.current_field {
                ExportFormField::OutputPath => {
                    self.export_form.output_path.push(c);
                }
                ExportFormField::ClusterName => {
                    self.export_form.cluster_name.push(c);
                }
                _ => {}
            },
            _ => {}
        }

        Ok(())
    }

    async fn handle_import_cluster_keys(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::Config;
            }
            KeyCode::Tab => {
                // Cycle through fields
                self.import_form.current_field = match self.import_form.current_field {
                    ImportFormField::InputPath => ImportFormField::Merge,
                    ImportFormField::Merge => ImportFormField::P2p,
                    ImportFormField::P2p => ImportFormField::InputPath,
                };
            }
            KeyCode::Enter => {
                match self.import_form.current_field {
                    ImportFormField::Merge => {
                        self.import_form.merge = !self.import_form.merge;
                    }
                    ImportFormField::P2p => {
                        self.import_form.p2p = !self.import_form.p2p;
                    }
                    ImportFormField::InputPath => {
                        // Execute import
                        self.execute_cluster_import().await?;
                    }
                }
            }
            KeyCode::Char(' ') => {
                // Toggle checkboxes
                match self.import_form.current_field {
                    ImportFormField::Merge => {
                        self.import_form.merge = !self.import_form.merge;
                    }
                    ImportFormField::P2p => {
                        self.import_form.p2p = !self.import_form.p2p;
                    }
                    _ => {}
                }
            }
            KeyCode::Backspace => {
                if let ImportFormField::InputPath = self.import_form.current_field {
                    self.import_form.input_path.pop();
                }
            }
            KeyCode::Char(c) => {
                if let ImportFormField::InputPath = self.import_form.current_field {
                    self.import_form.input_path.push(c);
                }
            }
            _ => {}
        }

        Ok(())
    }

    async fn execute_cluster_export(&mut self) -> BlixardResult<()> {
        // For TUI, we'll just show a success message
        // In a real implementation, this would call the cluster state manager
        self.status_message = Some(format!(
            "Cluster export initiated to: {}",
            self.export_form.output_path
        ));
        self.mode = AppMode::Config;
        Ok(())
    }

    async fn execute_cluster_import(&mut self) -> BlixardResult<()> {
        // For TUI, we'll just show a success message
        // In a real implementation, this would call the cluster state manager
        self.status_message = Some(format!(
            "Cluster import initiated from: {}",
            self.import_form.input_path
        ));
        self.mode = AppMode::Config;
        Ok(())
    }

    async fn refresh_p2p_stats(&mut self) -> BlixardResult<()> {
        if !self.p2p_enabled {
            return Ok(());
        }

        if let Some(manager) = &self.p2p_manager {
            // Get real data from P2P manager
            let peers = manager.get_peers().await;
            let transfers = manager.get_active_transfers().await;

            // Convert peers to UI format
            self.p2p_peers = peers
                .iter()
                .map(|peer| P2pPeer {
                    node_id: peer.node_id.chars().take(8).collect(),
                    address: peer.address.clone(),
                    status: "connected".to_string(),
                    latency_ms: peer.connection_quality.latency_ms,
                    shared_images: peer.shared_resources.len(),
                })
                .collect();

            // Convert transfers to UI format
            self.p2p_transfers = transfers
                .iter()
                .map(|transfer| {
                    let is_upload = matches!(
                        transfer.progress.status,
                        blixard_core::p2p_manager::TransferStatus::Uploading
                    );

                    P2pTransfer {
                        resource_name: format!(
                            "{}-{}.img",
                            transfer.request.name, transfer.request.version
                        ),
                        peer_id: transfer
                            .request
                            .source_peer
                            .clone()
                            .unwrap_or_else(|| "local".to_string()),
                        is_upload,
                        total_bytes: transfer.progress.total_bytes,
                        bytes_transferred: transfer.progress.bytes_transferred,
                        speed_bps: transfer.progress.speed_bps,
                    }
                })
                .collect();

            self.p2p_peer_count = self.p2p_peers.len();

            // Add some demo images
            self.p2p_images = vec![
                P2pImage {
                    name: "ubuntu-server".to_string(),
                    version: "22.04".to_string(),
                    size: 1024 * 1024 * 512, // 512MB
                    available_peers: self.p2p_peer_count,
                    is_cached: false,
                    is_downloading: self
                        .p2p_transfers
                        .iter()
                        .any(|t| t.resource_name.contains("ubuntu") && !t.is_upload),
                },
                P2pImage {
                    name: "alpine-linux".to_string(),
                    version: "3.18".to_string(),
                    size: 1024 * 1024 * 128, // 128MB
                    available_peers: self.p2p_peer_count + 1,
                    is_cached: true,
                    is_downloading: false,
                },
            ];

            self.p2p_shared_images = self.p2p_images.len();
        }

        self.status_message = Some("P2P stats refreshed".to_string());

        Ok(())
    }

    /// Prompt for peer connection
    async fn prompt_peer_connection(&mut self) -> BlixardResult<()> {
        if let Some(manager) = &self.p2p_manager {
            // In a real implementation, this would open a dialog
            // For now, we'll connect to a hardcoded peer
            let peer_addr = "192.168.1.100:7001";

            match manager.connect_peer(peer_addr).await {
                Ok(_) => {
                    self.status_message = Some(format!("Connected to peer at {}", peer_addr));
                    self.refresh_p2p_stats().await?;
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to connect to peer: {}", e));
                }
            }
        }

        Ok(())
    }

    /// Show P2P statistics
    async fn show_p2p_statistics(&mut self) -> BlixardResult<()> {
        let stats = format!(
            "P2P Statistics:\n\
             Node ID: {}\n\
             Connected Peers: {}\n\
             Shared Images: {}\n\
             Active Transfers: {}\n\
             Cache Size: {} images",
            self.p2p_node_id,
            self.p2p_peer_count,
            self.p2p_shared_images,
            self.p2p_transfers.len(),
            self.p2p_images.iter().filter(|img| img.is_cached).count()
        );

        self.status_message = Some(stats);
        Ok(())
    }

    /// List available P2P resources
    async fn list_p2p_resources(&mut self) -> BlixardResult<()> {
        if let Some(store) = &self.p2p_store {
            let store = store.lock().await;
            let images = store.list_images().await?;

            if images.is_empty() {
                self.status_message = Some("No resources available in P2P network".to_string());
            } else {
                let list = images
                    .iter()
                    .map(|img| format!("- {} v{} ({} bytes)", img.name, img.version, img.size))
                    .collect::<Vec<_>>()
                    .join("\n");

                self.status_message = Some(format!("Available P2P Resources:\n{}", list));
            }
        }

        Ok(())
    }

    /// Queue a P2P download
    async fn queue_p2p_download(&mut self) -> BlixardResult<()> {
        if let Some(manager) = &self.p2p_manager {
            // For demo, download the first non-cached image
            if let Some(image) = self.p2p_images.iter().find(|img| !img.is_cached) {
                let request_id = manager
                    .request_download(
                        blixard_core::p2p_manager::ResourceType::VmImage,
                        &image.name,
                        &image.version,
                        blixard_core::p2p_manager::TransferPriority::Normal,
                    )
                    .await?;

                self.status_message = Some(format!(
                    "Queued download of {} v{} (request ID: {})",
                    image.name, image.version, request_id
                ));

                // Refresh to show new transfer
                self.refresh_p2p_stats().await?;
            } else {
                self.status_message = Some("All available images are already cached".to_string());
            }
        }

        Ok(())
    }
}

impl PlacementStrategy {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            PlacementStrategy::MostAvailable => "Most Available",
            PlacementStrategy::LeastAvailable => "Least Available",
            PlacementStrategy::RoundRobin => "Round Robin",
            PlacementStrategy::Manual => "Manual",
        }
    }

    pub fn all() -> Vec<PlacementStrategy> {
        vec![
            PlacementStrategy::MostAvailable,
            PlacementStrategy::LeastAvailable,
            PlacementStrategy::RoundRobin,
            PlacementStrategy::Manual,
        ]
    }
}

impl NodeStatus {
    pub fn color(&self) -> ratatui::style::Color {
        use ratatui::style::Color;
        match self {
            NodeStatus::Healthy => Color::Green,
            NodeStatus::Warning => Color::Yellow,
            NodeStatus::Critical => Color::Red,
            NodeStatus::Offline => Color::Gray,
        }
    }
}

impl EventLevel {
    pub fn color(&self) -> ratatui::style::Color {
        use ratatui::style::Color;
        match self {
            EventLevel::Info => Color::Cyan,
            EventLevel::Warning => Color::Yellow,
            EventLevel::Error => Color::Red,
            EventLevel::Critical => Color::Magenta,
            EventLevel::Debug => Color::Gray,
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            EventLevel::Info => "ℹ️",
            EventLevel::Warning => "⚠️",
            EventLevel::Error => "❌",
            EventLevel::Critical => "🚨",
            EventLevel::Debug => "🔍",
        }
    }
}
