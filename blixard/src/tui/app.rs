//! Modern TUI Application State
//! 
//! Comprehensive TUI for Blixard cluster management with:
//! - Dashboard-first design with real-time metrics
//! - Enhanced VM management with scheduling
//! - Node management with daemon mode integration
//! - Monitoring and observability features
//! - Configuration management interface

use crate::BlixardResult;
use super::{Event, VmClient};
use blixard_core::types::VmStatus;
use ratatui::widgets::{ListState, TableState};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use serde_json;

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
    pub placement_strategy: Option<String>,
    pub created_at: Option<String>,
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
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeStatus {
    Healthy,
    Warning,
    Critical,
    Offline,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeRole {
    Leader,
    Follower,
    Candidate,
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
    pub last_updated: Instant,
}

#[derive(Debug, Clone)]
pub struct SystemEvent {
    pub timestamp: Instant,
    pub level: EventLevel,
    pub source: String,
    pub message: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EventLevel {
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppTab {
    Dashboard,
    VirtualMachines,
    Nodes,
    Monitoring,
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
    ClusterHealthView,
    SettingsForm,
    LogViewer,
    SearchDialog,
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
pub struct DebugLogEntry {
    pub timestamp: Instant,
    pub level: DebugLevel,
    pub component: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
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

pub struct App {
    // Core application state
    pub current_tab: AppTab,
    pub mode: AppMode,
    pub input_mode: InputMode,
    pub should_quit: bool,
    
    // Network and data
    pub vm_client: Option<VmClient>,
    pub auto_refresh: bool,
    pub refresh_interval: Duration,
    pub last_refresh: Instant,
    
    // Dashboard data
    pub cluster_metrics: ClusterMetrics,
    pub recent_events: Vec<SystemEvent>,
    pub max_events: usize,
    
    // VM management
    pub vms: Vec<VmInfo>,
    pub vm_list_state: ListState,
    pub vm_table_state: TableState,
    pub selected_vm: Option<String>,
    pub vm_logs: Vec<String>,
    pub vm_log_state: ListState,
    
    // Node management
    pub nodes: Vec<NodeInfo>,
    pub node_list_state: ListState,
    pub node_table_state: TableState,
    pub selected_node: Option<u64>,
    pub daemon_processes: HashMap<u64, u32>, // node_id -> PID
    
    // Forms and dialogs
    pub create_vm_form: CreateVmForm,
    pub create_node_form: CreateNodeForm,
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
    
    // Debug mode data
    pub raft_debug_info: Option<RaftDebugInfo>,
    pub debug_metrics: DebugMetrics,
    pub debug_log_entries: Vec<DebugLogEntry>,
    pub max_debug_entries: usize,
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
    HighRefresh,  // btop-style high-frequency updates
    Balanced,     // default mode
    PowerSaver,   // reduced updates for battery/low-power
    Debug,        // maximum detail with logging
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
            auto_refresh: true,
            refresh_interval: Duration::from_secs(2),
            last_refresh: Instant::now(),
            
            cluster_metrics: ClusterMetrics::default(),
            recent_events: Vec::new(),
            
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
            
            nodes: Vec::new(),
            node_list_state: ListState::default(),
            node_table_state: TableState::default(),
            selected_node: None,
            daemon_processes: HashMap::new(),
            
            create_vm_form: CreateVmForm::default(),
            create_node_form: CreateNodeForm::default(),
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
            selected_cluster: None,
            cluster_discovery_active: false,
            cluster_scan_progress: 0.0,
            
            cpu_history: Vec::new(),
            memory_history: Vec::new(),
            network_history: Vec::new(),
            max_history_points: 50,
            
            config_dirty: false,
            settings: AppSettings::default(),
        };
        
        // Add startup event
        app.add_event(EventLevel::Info, "TUI".to_string(), "Blixard TUI starting up".to_string());
        
        // Try to connect to local server
        app.add_event(EventLevel::Info, "Connection".to_string(), "Attempting to connect to cluster at 127.0.0.1:7002".to_string());
        app.try_connect().await;
        
        // Start cluster discovery
        app.start_cluster_discovery().await;
        
        Ok(app)
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
    
    pub async fn try_connect(&mut self) {
        match VmClient::new("127.0.0.1:7002").await {
            Ok(client) => {
                self.vm_client = Some(client);
                self.status_message = Some("Connected to cluster".to_string());
                self.error_message = None;
                self.add_event(EventLevel::Info, "Connection".to_string(), "Successfully connected to cluster".to_string());
                
                // Initial data load
                match self.refresh_all_data().await {
                    Ok(_) => {
                        self.add_event(EventLevel::Info, "Data".to_string(), "Initial data refresh completed".to_string());
                    }
                    Err(e) => {
                        self.add_event(EventLevel::Error, "Data".to_string(), format!("Initial data refresh failed: {}", e));
                    }
                }
            }
            Err(e) => {
                self.vm_client = None;
                self.error_message = Some(format!("Failed to connect: {}", e));
                self.status_message = None;
                self.add_event(EventLevel::Error, "Connection".to_string(), format!("Failed to connect to cluster: {}", e));
            }
        }
    }
    
    pub async fn refresh_all_data(&mut self) -> BlixardResult<()> {
        if self.vm_client.is_none() {
            self.add_event(EventLevel::Warning, "Data".to_string(), "Cannot refresh data - no connection to cluster".to_string());
            return Ok(());
        }
        
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
        
        self.last_refresh = Instant::now();
        
        if !errors.is_empty() {
            let error_msg = format!("Data refresh errors: {}", errors.join(", "));
            self.add_event(EventLevel::Warning, "Data".to_string(), error_msg.clone());
            return Err(crate::BlixardError::Internal { message: error_msg });
        }
        
        // Log successful refresh with data counts
        self.add_event(EventLevel::Info, "Data".to_string(), 
            format!("Refreshed: {} VMs, {} nodes, leader={:?}", 
                self.vms.len(), 
                self.nodes.len(), 
                self.cluster_metrics.leader_id
            )
        );
        
        Ok(())
    }
    
    pub async fn refresh_vm_list(&mut self) -> BlixardResult<()> {
        if let Some(client) = &mut self.vm_client {
            match client.list_vms().await {
                Ok(vms) => {
                    self.vms = vms.into_iter().map(|vm| VmInfo {
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
                        created_at: None, // TODO: Add to proto
                        config_path: None, // TODO: Add to proto
                    }).collect();
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
    
    pub async fn refresh_cluster_metrics(&mut self) -> BlixardResult<()> {
        if let Some(client) = &mut self.vm_client {
            match client.get_cluster_status().await {
                Ok(status) => {
                    self.cluster_metrics = ClusterMetrics {
                        total_nodes: status.nodes.len() as u32,
                        healthy_nodes: status.nodes.len() as u32, // Simplified: assume all nodes are healthy
                        total_vms: self.vms.len() as u32,
                        running_vms: self.vms.iter().filter(|vm| vm.status == VmStatus::Running).count() as u32,
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
                    self.nodes = status.nodes.into_iter().map(|node| NodeInfo {
                        id: node.id,
                        address: node.address,
                        status: NodeStatus::Healthy, // Simplified: assume all nodes are healthy
                        role: if node.id == status.leader_id {
                            NodeRole::Leader
                        } else {
                            NodeRole::Follower
                        },
                        cpu_usage: 0.0, // TODO: Get actual metrics
                        memory_usage: 0.0, // TODO: Get actual metrics
                        vm_count: self.vms.iter().filter(|vm| vm.node_id == node.id).count() as u32,
                        last_seen: Some(Instant::now()),
                        version: None, // TODO: Add to proto
                    }).collect();
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
            (self.cluster_metrics.used_memory as f32 / self.cluster_metrics.total_memory as f32) * 100.0
        } else {
            0.0
        };
        self.memory_history.push(memory_usage);
        if self.memory_history.len() > self.max_history_points {
            self.memory_history.remove(0);
        }
        
        // Network history (placeholder)
        self.network_history.push(50.0); // TODO: Get actual network metrics
        if self.network_history.len() > self.max_history_points {
            self.network_history.remove(0);
        }
    }
    
    pub fn add_event(&mut self, level: EventLevel, source: String, message: String) {
        let event = SystemEvent {
            timestamp: Instant::now(),
            level,
            source,
            message,
            details: None,
        };
        
        self.recent_events.insert(0, event);
        if self.recent_events.len() > self.max_events {
            self.recent_events.truncate(self.max_events);
        }
    }
    
    pub fn switch_tab(&mut self, tab: AppTab) {
        self.current_tab = tab.clone();
        self.mode = match tab {
            AppTab::Dashboard => AppMode::Dashboard,
            AppTab::VirtualMachines => AppMode::VmList,
            AppTab::Nodes => AppMode::NodeList,
            AppTab::Monitoring => AppMode::Monitoring,
            AppTab::Configuration => AppMode::Config,
            AppTab::Debug => AppMode::Debug,
            AppTab::Help => AppMode::Help,
        };
        self.input_mode = InputMode::Normal;
    }
    
    pub fn should_refresh(&self) -> bool {
        let refresh_interval = match self.settings.performance_mode {
            PerformanceMode::HighRefresh => Duration::from_millis(self.settings.update_frequency_ms / 2),
            PerformanceMode::Balanced => self.refresh_interval,
            PerformanceMode::PowerSaver => Duration::from_secs(self.settings.refresh_rate * 2),
            PerformanceMode::Debug => Duration::from_millis(self.settings.update_frequency_ms),
        };
        
        self.auto_refresh && self.last_refresh.elapsed() >= refresh_interval
    }
    
    /// Apply current VM filter
    pub fn apply_vm_filter(&mut self) {
        self.filtered_vms = self.vms.iter()
            .filter(|vm| self.vm_matches_filter(vm))
            .cloned()
            .collect();
    }
    
    /// Apply current node filter
    pub fn apply_node_filter(&mut self) {
        self.filtered_nodes = self.nodes.iter()
            .filter(|node| self.node_matches_filter(node))
            .cloned()
            .collect();
    }
    
    fn vm_matches_filter(&self, vm: &VmInfo) -> bool {
        // Quick filter first
        if !self.quick_filter.is_empty() {
            if !vm.name.to_lowercase().contains(&self.quick_filter.to_lowercase()) {
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
            if !node.address.to_lowercase().contains(&self.quick_filter.to_lowercase()) {
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
    pub fn set_vm_filter(&mut self, filter: VmFilter) {
        self.vm_filter = filter;
        self.apply_vm_filter();
    }
    
    /// Set node filter and apply it
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
        
        self.add_event(EventLevel::Info, "Settings".to_string(), 
            format!("Performance mode: {:?}", self.settings.performance_mode));
    }
    
    /// Switch to tab by index (for vim navigation)
    pub fn switch_to_tab_by_index(&mut self, index: usize) {
        let tab = match index {
            0 => AppTab::Dashboard,
            1 => AppTab::VirtualMachines,
            2 => AppTab::Nodes,
            3 => AppTab::Monitoring,
            4 => AppTab::Configuration,
            5 => AppTab::Debug,
            6 => AppTab::Help,
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
                if self.should_refresh() {
                    if let Err(e) = self.refresh_all_data().await {
                        self.add_event(EventLevel::Error, "Refresh".to_string(), format!("Auto-refresh failed: {}", e));
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
                        AppTab::Dashboard => 6, // wrap to Help
                        AppTab::VirtualMachines => 0,
                        AppTab::Nodes => 1,
                        AppTab::Monitoring => 2,
                        AppTab::Configuration => 3,
                        AppTab::Debug => 5,
                        AppTab::Help => 4,
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
                        AppTab::Configuration => 5,
                        AppTab::Debug => 6,
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
                self.add_event(EventLevel::Info, "Settings".to_string(), 
                    format!("Vim mode: {}", if self.settings.vim_mode { "enabled" } else { "disabled" }));
                return Ok(());
            }
            (KeyCode::Char('d'), KeyModifiers::NONE) => {
                // Toggle debug mode
                if self.search_mode == SearchMode::None {
                    self.settings.debug_mode = !self.settings.debug_mode;
                    self.add_event(EventLevel::Info, "Settings".to_string(), 
                        format!("Debug mode: {}", if self.settings.debug_mode { "enabled" } else { "disabled" }));
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
            KeyCode::Char('5') => self.switch_tab(AppTab::Configuration),
            KeyCode::Char('6') => self.switch_tab(AppTab::Debug),
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
            AppMode::Help => self.handle_help_keys(key),
            AppMode::Debug | AppMode::RaftDebug | AppMode::DebugMetrics | AppMode::DebugLogs => {
                self.handle_debug_keys(key).await?;
            }
            _ => {}
        }
        
        Ok(())
    }
    
    async fn handle_create_cluster_keys(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
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
    
    async fn handle_cluster_discovery_keys(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
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
        
        self.add_event(EventLevel::Info, "Discovery".to_string(), "Starting cluster discovery...".to_string());
        
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
                        self.add_event(EventLevel::Info, "Discovery".to_string(), 
                            format!("Found cluster at {}", endpoint));
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
        self.add_event(EventLevel::Info, "Discovery".to_string(), 
            format!("Discovery complete: found {} cluster(s)", count));
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
                
                self.add_event(EventLevel::Info, "Connection".to_string(), 
                    format!("Connected to cluster at {}", cluster_address));
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to connect to {}: {}", cluster_address, e));
                self.add_event(EventLevel::Error, "Connection".to_string(), 
                    format!("Failed to connect to {}: {}", cluster_address, e));
            }
        }
        Ok(())
    }
    
    /// Create a new cluster from template
    pub async fn create_cluster_from_template(&mut self, template: &ClusterTemplate) -> BlixardResult<()> {
        self.add_event(EventLevel::Info, "Cluster".to_string(), 
            format!("Creating cluster from template: {}", template.name));
            
        // Start the bootstrap node
        let bootstrap_port = template.network_config.base_port + 1;
        let bootstrap_address = format!("127.0.0.1:{}", bootstrap_port);
        
        self.add_event(EventLevel::Info, "Cluster".to_string(), 
            format!("Starting bootstrap node at {}", bootstrap_address));
        
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
                self.status_message = Some(format!("Cluster '{}' created successfully", template.name));
            }
            Err(e) => {
                self.error_message = Some(format!("Cluster created but connection failed: {}", e));
            }
        }
        
        Ok(())
    }
    
    /// Scale cluster by adding nodes
    pub async fn scale_cluster(&mut self, target_node_count: u32) -> BlixardResult<()> {
        let selected_cluster = self.selected_cluster.clone();
        
        if let Some(selected) = selected_cluster {
            self.add_event(EventLevel::Info, "Cluster".to_string(), 
                format!("Scaling cluster to {} nodes", target_node_count));
            
            // Find current cluster
            if let Some(cluster) = self.discovered_clusters.iter_mut()
                .find(|c| c.leader_address == selected) {
                    
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
                    drop(cluster); // Explicitly drop the mutable borrow
                    
                    // Now add all the events
                    for event_msg in events {
                        self.add_event(EventLevel::Info, "Cluster".to_string(), event_msg);
                    }
                    self.status_message = Some(format!("Cluster scaled to {} nodes", target_node_count));
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
    
    async fn handle_dashboard_keys(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
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
                                    message: format!("Are you sure you want to stop VM '{}'?", vm.name),
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
            _ => {}
        }
        Ok(())
    }
    
    async fn handle_vm_create_keys(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
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
            KeyCode::Char(c) => {
                match self.create_vm_form.current_field {
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
                            let mut node_id_str = self.create_vm_form.node_id.map(|id| id.to_string()).unwrap_or_default();
                            node_id_str.push(c);
                            if let Ok(parsed) = node_id_str.parse::<u64>() {
                                self.create_vm_form.node_id = Some(parsed);
                            }
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Backspace => {
                match self.create_vm_form.current_field {
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
                }
            }
            KeyCode::Up | KeyCode::Down => {
                if self.create_vm_form.current_field == CreateVmField::PlacementStrategy {
                    // Cycle through placement strategies
                    let strategies = PlacementStrategy::all();
                    let current_index = strategies.iter().position(|s| *s == self.create_vm_form.placement_strategy).unwrap_or(0);
                    let new_index = if key.code == KeyCode::Up {
                        if current_index == 0 { strategies.len() - 1 } else { current_index - 1 }
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
            match client.create_vm(&self.create_vm_form.name, vcpus, memory).await {
                Ok(_) => {
                    self.status_message = Some(format!("VM '{}' created successfully", self.create_vm_form.name));
                    self.create_vm_form = CreateVmForm::default(); // Reset form
                    self.mode = AppMode::VmList;
                    self.refresh_vm_list().await?;
                    self.add_event(EventLevel::Info, "VM".to_string(), format!("Created VM '{}'", self.create_vm_form.name));
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to create VM: {}", e));
                    self.add_event(EventLevel::Error, "VM".to_string(), format!("Failed to create VM '{}': {}", self.create_vm_form.name, e));
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
            self.error_message = Some("Bind address must include port (e.g., 127.0.0.1:7003)".to_string());
            return Ok(());
        }
        
        // Join cluster (this is what "creating a node" means in cluster context)
        if let Some(client) = &mut self.vm_client {
            match client.join_cluster(node_id, &self.create_node_form.bind_address).await {
                Ok(message) => {
                    self.status_message = Some(format!("Node {} joined cluster: {}", node_id, message));
                    self.create_node_form = CreateNodeForm::default(); // Reset form
                    self.mode = AppMode::NodeList;
                    self.refresh_node_list().await?;
                    self.add_event(EventLevel::Info, "Node".to_string(), format!("Node {} joined cluster", node_id));
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to join cluster: {}", e));
                    self.add_event(EventLevel::Error, "Node".to_string(), format!("Failed to join cluster: {}", e));
                }
            }
        } else {
            self.error_message = Some("Not connected to cluster".to_string());
        }
        
        Ok(())
    }
    
    async fn handle_node_list_keys(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
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
                            message: format!("Are you sure you want to destroy node {} at {}?", node.id, node.address),
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
            _ => {}
        }
        Ok(())
    }
    
    async fn handle_create_node_keys(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
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
            KeyCode::Char(c) => {
                match self.create_node_form.current_field {
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
                }
            }
            KeyCode::Backspace => {
                match self.create_node_form.current_field {
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
                }
            }
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
    
    async fn handle_confirm_dialog_keys(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
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
                    self.add_event(EventLevel::Info, "VM".to_string(), format!("Deleted VM '{}'", name));
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to delete VM '{}': {}", name, e));
                    self.add_event(EventLevel::Error, "VM".to_string(), format!("Failed to delete VM '{}': {}", name, e));
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
                    self.add_event(EventLevel::Info, "VM".to_string(), format!("Started VM '{}'", name));
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to start VM '{}': {}", name, e));
                    self.add_event(EventLevel::Error, "VM".to_string(), format!("Failed to start VM '{}': {}", name, e));
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
                    self.add_event(EventLevel::Info, "VM".to_string(), format!("Stopped VM '{}'", name));
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to stop VM '{}': {}", name, e));
                    self.add_event(EventLevel::Error, "VM".to_string(), format!("Failed to stop VM '{}': {}", name, e));
                }
            }
        }
        Ok(())
    }
    
    /// Restart a node (daemon mode)
    async fn restart_node(&mut self, node_id: u64) -> BlixardResult<()> {
        self.add_event(EventLevel::Info, "Node".to_string(), 
            format!("Restarting node {}", node_id));
        
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
        self.add_event(EventLevel::Info, "Cluster".to_string(), 
            format!("Current cluster size: {} nodes. Use 's' to scale.", current_count));
        
        // In a full implementation, this would show a dialog to input target node count
        self.status_message = Some("Cluster scaling: Use dashboard 'N' for templates".to_string());
    }
    
    /// Enhanced node removal with cleanup
    async fn remove_node(&mut self, id: u64) -> BlixardResult<()> {
        self.add_event(EventLevel::Warning, "Node".to_string(), 
            format!("Removing node {} from cluster", id));
        
        // In a real implementation, this would:
        // 1. Gracefully remove the node from Raft cluster
        // 2. Migrate any VMs running on that node
        // 3. Clean up data directories
        // 4. Stop daemon processes
        // 5. Update cluster configuration
        
        // For now, simulate by removing from discovered clusters and UI
        if let Some(cluster) = self.discovered_clusters.iter_mut()
            .find(|c| c.leader_address == self.selected_cluster.as_deref().unwrap_or("")) {
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
        self.add_event(EventLevel::Info, "Node".to_string(), 
            format!("Node {} successfully removed", id));
        
        Ok(())
    }
    
    /// Destroy an entire cluster
    async fn destroy_cluster(&mut self, cluster_name: &str) -> BlixardResult<()> {
        self.add_event(EventLevel::Warning, "Cluster".to_string(), 
            format!("Destroying cluster '{}'", cluster_name));
        
        // In a real implementation, this would:
        // 1. Stop all VMs on all nodes
        // 2. Gracefully shutdown all nodes
        // 3. Clean up all data directories
        // 4. Remove network configurations
        // 5. Terminate daemon processes
        
        // For now, simulate by removing from discovered clusters
        self.discovered_clusters.retain(|cluster| cluster.name != cluster_name);
        
        // Clear connection if we were connected to this cluster
        if let Some(selected) = &self.selected_cluster {
            if self.discovered_clusters.iter()
                .any(|c| c.leader_address == *selected && c.name == cluster_name) {
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
        self.add_event(EventLevel::Info, "Cluster".to_string(), 
            format!("Cluster '{}' successfully destroyed", cluster_name));
        
        Ok(())
    }
    
    async fn handle_mouse_event(&mut self, mouse: crossterm::event::MouseEvent) -> BlixardResult<()> {
        use crossterm::event::{MouseEventKind, MouseButton};
        
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
                self.status_message = Some(format!("Scrolled {} in current view", if scroll_up { "up" } else { "down" }));
            }
        }
        Ok(())
    }
    
    /// Add debug log entry
    pub fn add_debug_log(&mut self, level: DebugLevel, component: String, message: String, details: Option<serde_json::Value>) {
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
            format!("State: {:?}, Term: {}, Commit: {}", 
                debug_info.state, debug_info.current_term, debug_info.commit_index),
            None
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
            None
        );
    }
    
    /// Handle debug mode key events
    pub async fn handle_debug_keys(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
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
}

impl PlacementStrategy {
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
        }
    }
    
    pub fn icon(&self) -> &'static str {
        match self {
            EventLevel::Info => "ℹ️",
            EventLevel::Warning => "⚠️",
            EventLevel::Error => "❌",
            EventLevel::Critical => "🚨",
        }
    }
}