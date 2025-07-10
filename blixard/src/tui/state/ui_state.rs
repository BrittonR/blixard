//! UI-related state management

use crate::tui::types::ui::{AppTab, AppMode, InputMode, ConnectionStatus, SearchMode};
use crate::tui::types::cluster::{ClusterInfo, DiscoveredCluster, ClusterTemplate};
use crate::tui::types::node::NodeTemplate;
use crate::tui::types::vm::VmTemplate;
use crate::tui::forms::{ConfirmDialog, SaveConfigField};
use crate::tui::forms::cluster::CreateClusterForm;
use crate::tui::forms::export_import::{ExportForm, ImportForm};
use crate::tui::forms::settings::AppSettings;
use crate::tui::Event;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct UiState {
    // Core UI state
    pub current_tab: AppTab,
    pub mode: AppMode,
    pub input_mode: InputMode,
    pub should_quit: bool,
    
    // Network connection
    pub connection_status: ConnectionStatus,
    pub auto_refresh: bool,
    pub refresh_interval: Duration,
    pub last_refresh: Instant,
    
    // Messages and dialogs
    pub status_message: Option<String>,
    pub error_message: Option<String>,
    pub confirm_dialog: Option<ConfirmDialog>,
    pub show_help: bool,
    
    // Search and filtering
    pub search_query: String,
    pub search_mode: SearchMode,
    pub quick_filter: String,
    
    // Configuration
    pub config_dirty: bool,
    pub settings: AppSettings,
    pub config_file_path: Option<String>,
    pub config_description: String,
    pub save_config_field: SaveConfigField,
    
    // Cluster discovery
    pub discovered_clusters: Vec<DiscoveredCluster>,
    pub cluster_templates: Vec<ClusterTemplate>,
    pub node_templates: Vec<NodeTemplate>,
    pub vm_templates: Vec<VmTemplate>,
    pub selected_cluster: Option<String>,
    pub cluster_discovery_active: bool,
    pub cluster_scan_progress: f32,
    pub current_cluster_info: Option<ClusterInfo>,
    
    // Forms
    pub create_cluster_form: CreateClusterForm,
    pub export_form: ExportForm,
    pub import_form: ImportForm,
    
    // Event handling
    pub event_sender: Option<tokio::sync::mpsc::UnboundedSender<Event>>,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            current_tab: AppTab::Dashboard,
            mode: AppMode::Dashboard,
            input_mode: InputMode::Normal,
            should_quit: false,
            connection_status: ConnectionStatus::Disconnected,
            auto_refresh: true,
            refresh_interval: Duration::from_secs(5),
            last_refresh: Instant::now(),
            status_message: None,
            error_message: None,
            confirm_dialog: None,
            show_help: false,
            search_query: String::new(),
            search_mode: SearchMode::None,
            quick_filter: String::new(),
            config_dirty: false,
            settings: AppSettings::default(),
            config_file_path: None,
            config_description: String::new(),
            save_config_field: SaveConfigField::FilePath,
            discovered_clusters: Vec::new(),
            cluster_templates: Self::default_cluster_templates(),
            node_templates: Self::default_node_templates(),
            vm_templates: Self::default_vm_templates(),
            selected_cluster: None,
            cluster_discovery_active: false,
            cluster_scan_progress: 0.0,
            current_cluster_info: None,
            create_cluster_form: CreateClusterForm::new(),
            export_form: ExportForm::new(),
            import_form: ImportForm::new(),
            event_sender: None,
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
    }

    pub fn should_refresh(&self) -> bool {
        self.auto_refresh && self.last_refresh.elapsed() >= self.refresh_interval
    }

    pub fn set_event_sender(&mut self, sender: tokio::sync::mpsc::UnboundedSender<Event>) {
        self.event_sender = Some(sender);
    }

    fn default_cluster_templates() -> Vec<ClusterTemplate> {
        vec![
            ClusterTemplate {
                name: "Small Development".to_string(),
                description: "3-node cluster for development".to_string(),
                node_count: 3,
                vm_count: 5,
                total_vcpus: 12,
                total_memory_gb: 16,
                replication_factor: 3,
            },
            ClusterTemplate {
                name: "Medium Production".to_string(),
                description: "5-node cluster for production workloads".to_string(),
                node_count: 5,
                vm_count: 20,
                total_vcpus: 40,
                total_memory_gb: 64,
                replication_factor: 3,
            },
        ]
    }

    fn default_node_templates() -> Vec<NodeTemplate> {
        vec![
            NodeTemplate {
                name: "Standard Node".to_string(),
                cpu_cores: 8,
                memory_gb: 16,
                disk_gb: 500,
                features: vec!["microvm".to_string()],
                location: None,
            },
            NodeTemplate {
                name: "High Memory Node".to_string(),
                cpu_cores: 16,
                memory_gb: 64,
                disk_gb: 1000,
                features: vec!["microvm".to_string(), "gpu".to_string()],
                location: None,
            },
        ]
    }

    fn default_vm_templates() -> Vec<VmTemplate> {
        vec![
            VmTemplate {
                name: "Micro".to_string(),
                description: "Minimal VM for testing".to_string(),
                vcpus: 1,
                memory: 512,
                disk_gb: 10,
                template_type: crate::tui::types::vm::VmTemplateType::Development,
                features: vec![],
            },
            VmTemplate {
                name: "Web Server".to_string(),
                description: "Optimized for web workloads".to_string(),
                vcpus: 2,
                memory: 2048,
                disk_gb: 20,
                template_type: crate::tui::types::vm::VmTemplateType::WebServer,
                features: vec!["nginx".to_string()],
            },
        ]
    }
}