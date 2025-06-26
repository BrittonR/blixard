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
use blixard_core::types::{VmStatus, VmConfig};
use ratatui::widgets::{ListState, TableState};
use tokio::sync::mpsc;
use std::collections::HashMap;
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
    
    // Popup/overlay modes
    ConfirmDialog,
    CreateVmForm,
    CreateNodeForm,
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
    ShutdownCluster,
    ResetData,
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
    
    // Monitoring
    pub cpu_history: Vec<f32>,
    pub memory_history: Vec<f32>,
    pub network_history: Vec<f32>,
    pub max_history_points: usize,
    
    // Configuration
    pub config_dirty: bool,
    pub settings: AppSettings,
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
            name: String::new(),
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
            id: String::new(),
            bind_address: "127.0.0.1:7001".to_string(),
            data_dir: "./data".to_string(),
            peers: String::new(),
            vm_backend: "microvm".to_string(),
            daemon_mode: true,
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
            
            cpu_history: Vec::new(),
            memory_history: Vec::new(),
            network_history: Vec::new(),
            max_history_points: 50,
            
            config_dirty: false,
            settings: AppSettings::default(),
        };
        
        // Try to connect to local server
        app.try_connect().await;
        
        Ok(app)
    }
    
    pub async fn try_connect(&mut self) {
        match VmClient::new("127.0.0.1:7001").await {
            Ok(client) => {
                self.vm_client = Some(client);
                self.status_message = Some("Connected to cluster".to_string());
                self.error_message = None;
                
                // Initial data load
                let _ = self.refresh_all_data().await;
            }
            Err(e) => {
                self.vm_client = None;
                self.error_message = Some(format!("Failed to connect: {}", e));
                self.status_message = None;
            }
        }
    }
    
    pub async fn refresh_all_data(&mut self) -> BlixardResult<()> {
        if self.vm_client.is_none() {
            return Ok(());
        }
        
        // Refresh VMs
        self.refresh_vm_list().await?;
        
        // Refresh cluster metrics
        self.refresh_cluster_metrics().await?;
        
        // Refresh nodes
        self.refresh_node_list().await?;
        
        // Update history
        self.update_metrics_history();
        
        self.last_refresh = Instant::now();
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
                    self.error_message = None;
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to refresh VMs: {}", e));
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
                    self.error_message = Some(format!("Failed to refresh cluster: {}", e));
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
                    self.error_message = None;
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to refresh nodes: {}", e));
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
            AppTab::Help => AppMode::Help,
        };
        self.input_mode = InputMode::Normal;
    }
    
    pub fn should_refresh(&self) -> bool {
        self.auto_refresh && self.last_refresh.elapsed() >= self.refresh_interval
    }
    
    
    pub async fn handle_event(&mut self, event: Event) -> BlixardResult<()> {
        match event {
            Event::Tick => {
                if self.should_refresh() {
                    let _ = self.refresh_all_data().await;
                }
            }
            Event::Key(key) => {
                self.handle_key_event(key).await?;
            }
            Event::LogLine(_) => {
                // TODO: Handle log lines for live log viewing
            }
        }
        Ok(())
    }
    
    async fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
        use crossterm::event::{KeyCode, KeyModifiers};
        
        // Global keybindings
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), KeyModifiers::NONE) => {
                if self.mode == AppMode::ConfirmDialog {
                    // Handle confirm dialog
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
                self.switch_tab(AppTab::Help);
                return Ok(());
            }
            (KeyCode::Char('r'), KeyModifiers::NONE) => {
                let _ = self.refresh_all_data().await;
                self.status_message = Some("Data refreshed".to_string());
                return Ok(());
            }
            _ => {}
        }
        
        // Tab switching
        match key.code {
            KeyCode::Char('1') => self.switch_tab(AppTab::Dashboard),
            KeyCode::Char('2') => self.switch_tab(AppTab::VirtualMachines),
            KeyCode::Char('3') => self.switch_tab(AppTab::Nodes),
            KeyCode::Char('4') => self.switch_tab(AppTab::Monitoring),
            KeyCode::Char('5') => self.switch_tab(AppTab::Configuration),
            _ => {}
        }
        
        // Mode-specific handling
        match self.mode {
            AppMode::Dashboard => self.handle_dashboard_keys(key).await?,
            AppMode::VmList => self.handle_vm_list_keys(key).await?,
            AppMode::VmCreate => self.handle_vm_create_keys(key).await?,
            AppMode::NodeList => self.handle_node_list_keys(key).await?,
            AppMode::CreateNodeForm => self.handle_create_node_keys(key).await?,
            AppMode::ConfirmDialog => self.handle_confirm_dialog_keys(key).await?,
            AppMode::Help => self.handle_help_keys(key),
            _ => {}
        }
        
        Ok(())
    }
    
    async fn handle_dashboard_keys(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
        use crossterm::event::KeyCode;
        
        match key.code {
            KeyCode::Char('c') => {
                self.mode = AppMode::CreateVmForm;
                self.create_vm_form = CreateVmForm::default();
            }
            KeyCode::Char('n') => {
                self.mode = AppMode::CreateNodeForm;
                self.create_node_form = CreateNodeForm::default();
            }
            KeyCode::Char('s') => {
                // Show cluster status
                let _ = self.refresh_cluster_metrics().await;
                self.status_message = Some("Cluster status updated".to_string());
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
                self.create_vm_form = CreateVmForm::default();
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
            _ => {}
        }
        Ok(())
    }
    
    async fn handle_vm_create_keys(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
        use crossterm::event::KeyCode;
        
        // TODO: Implement VM creation form handling
        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::VmList;
            }
            _ => {}
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
                self.create_node_form = CreateNodeForm::default();
            }
            _ => {}
        }
        Ok(())
    }
    
    async fn handle_create_node_keys(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
        use crossterm::event::KeyCode;
        
        // TODO: Implement node creation form handling
        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::NodeList;
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
    
    async fn remove_node(&mut self, id: u64) -> BlixardResult<()> {
        // TODO: Implement node removal
        self.status_message = Some(format!("Node {} removal not yet implemented", id));
        Ok(())
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