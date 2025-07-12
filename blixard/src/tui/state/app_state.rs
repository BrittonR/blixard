//! Main application state combining all state modules

use super::{vm_state::VmState, node_state::NodeState, monitoring_state::MonitoringState, debug_state::DebugState, ui_state::UiState};
use crate::tui::{Event, VmClient};
use crate::tui::types::ui::{AppTab, LogSourceType, LogLevel};
use crate::tui::types::monitoring::EventLevel;
use crate::tui::types::monitoring::SystemEvent;
use crate::tui::types::debug::{DebugLogEntry, DebugLevel, RaftDebugInfo, DebugMetrics};
use crate::{BlixardError, BlixardResult};
use std::time::{Duration, Instant};

pub struct App {
    // State modules
    pub vm_state: VmState,
    pub node_state: NodeState,
    pub monitoring_state: MonitoringState,
    pub debug_state: DebugState,
    pub ui_state: UiState,
    
    // Network client
    pub vm_client: Option<VmClient>,
}

impl App {
    pub fn new() -> Self {
        Self {
            vm_state: VmState::new(),
            node_state: NodeState::new(),
            monitoring_state: MonitoringState::new(),
            debug_state: DebugState::new(),
            ui_state: UiState::new(),
            vm_client: None,
        }
    }

    // Delegate methods to maintain API compatibility
    pub fn switch_tab(&mut self, tab: AppTab) {
        self.ui_state.switch_tab(tab);
    }

    pub fn should_refresh(&self) -> bool {
        self.ui_state.should_refresh()
    }

    pub fn apply_vm_filter(&mut self) {
        self.vm_state.apply_filter();
    }

    pub fn apply_node_filter(&mut self) {
        self.node_state.apply_filter();
    }

    pub fn get_displayed_vms(&self) -> &[crate::tui::types::vm::VmInfo] {
        self.vm_state.get_displayed_vms()
    }

    pub fn get_displayed_nodes(&self) -> &[crate::tui::types::node::NodeInfo] {
        self.node_state.get_displayed_nodes()
    }

    pub fn add_event(&mut self, level: EventLevel, source: String, message: String) {
        let event = SystemEvent {
            timestamp: Instant::now(),
            level,
            source,
            message,
            details: None,
        };
        self.monitoring_state.add_event(event);
    }

    pub fn add_log_entry(&mut self, source: LogSourceType, level: LogLevel, message: String) {
        let entry = crate::tui::types::ui::LogEntry {
            timestamp: Instant::now(),
            source,
            level,
            message,
            context: None,
        };
        self.monitoring_state.add_log_entry(entry);
    }

    pub fn add_debug_log(
        &mut self,
        level: DebugLevel,
        component: String,
        message: String,
        context: Option<String>,
    ) {
        let entry = DebugLogEntry {
            timestamp: Instant::now(),
            level,
            component,
            message,
            context,
        };
        self.debug_state.add_debug_log(entry);
    }

    // Delegation methods for UI compatibility
    pub fn current_tab(&self) -> &crate::tui::types::ui::AppTab {
        &self.ui_state.current_tab
    }

    pub fn mode(&self) -> &crate::tui::types::ui::AppMode {
        &self.ui_state.mode
    }

    pub fn search_mode(&self) -> &crate::tui::types::ui::SearchMode {
        &self.ui_state.search_mode
    }

    pub fn p2p_enabled(&self) -> bool {
        self.debug_state.p2p_enabled
    }

    pub fn p2p_node_id(&self) -> &str {
        &self.debug_state.p2p_node_id
    }

    pub fn p2p_peer_count(&self) -> usize {
        self.debug_state.p2p_peer_count
    }

    pub fn p2p_shared_images(&self) -> usize {
        self.debug_state.p2p_shared_images
    }

    pub fn p2p_peers(&self) -> &[crate::tui::types::p2p::P2pPeer] {
        &self.debug_state.p2p_peers
    }

    pub fn p2p_images(&self) -> &[crate::tui::types::p2p::P2pImage] {
        &self.debug_state.p2p_images
    }

    pub fn p2p_transfers(&self) -> &[crate::tui::types::p2p::P2pTransfer] {
        &self.debug_state.p2p_transfers
    }

    pub fn update_raft_debug_info(&mut self, debug_info: RaftDebugInfo) {
        self.debug_state.update_raft_debug_info(debug_info);
    }

    pub fn update_debug_metrics<F>(&mut self, update_fn: F)
    where
        F: FnOnce(&mut DebugMetrics),
    {
        self.debug_state.update_debug_metrics(update_fn);
    }

    pub fn reset_debug_metrics(&mut self) {
        self.debug_state.reset_debug_metrics();
    }

    pub fn update_metrics_history(&mut self) {
        self.monitoring_state.update_metrics_history();
    }

    pub fn set_event_sender(&mut self, sender: tokio::sync::mpsc::UnboundedSender<Event>) {
        self.ui_state.set_event_sender(sender);
    }

    pub fn update_connection_latency(&mut self, operation_duration: Duration) {
        use crate::tui::types::ui::ConnectionStatus;
        
        if matches!(self.ui_state.connection_status, ConnectionStatus::Connected) {
            if operation_duration > Duration::from_secs(5) {
                self.ui_state.connection_status = ConnectionStatus::PartiallyConnected {
                    connected: self.node_state.nodes.len(),
                    total: self.node_state.nodes.len(),
                };
                self.add_event(
                    EventLevel::Warning,
                    "Connection".to_string(),
                    format!("High latency detected: {:?}", operation_duration),
                );
            }
        }
    }

    pub fn check_reconnection_needed(&mut self) {
        use crate::tui::types::ui::ConnectionStatus;
        
        if let ConnectionStatus::Error(ref error) = self.ui_state.connection_status {
            if error.contains("connection refused") || error.contains("timeout") {
                self.ui_state.status_message = Some("Attempting to reconnect...".to_string());
                
                if let Some(event_sender) = &self.ui_state.event_sender {
                    let _ = event_sender.send(Event::Reconnect);
                }
            }
        }
    }

    pub fn update_cluster_health(&mut self) {
        use crate::tui::types::cluster::ClusterHealth;
        use crate::tui::types::node::NodeStatus;
        
        let healthy_nodes = self.node_state.nodes.iter()
            .filter(|n| matches!(n.status, NodeStatus::Healthy))
            .count();
        let total_nodes = self.node_state.nodes.len();

        self.monitoring_state.cluster_health = if total_nodes == 0 {
            ClusterHealth::Unknown
        } else if healthy_nodes == total_nodes {
            ClusterHealth::Healthy
        } else if healthy_nodes > total_nodes / 2 {
            ClusterHealth::Degraded
        } else {
            ClusterHealth::Critical
        };
    }

    pub fn open_log_viewer(&mut self, source: Option<LogSourceType>) {
        use crate::tui::types::ui::AppMode;
        
        self.ui_state.mode = AppMode::LogViewer;
        if let Some(source) = source {
            self.monitoring_state.log_stream_config.sources = vec![source];
        }
    }

    pub fn simulate_log_entries(&mut self) {
        // For testing/demo purposes
        let sources = vec![
            LogSourceType::System,
            LogSourceType::VM("vm-001".to_string()),
            LogSourceType::Node(1),
            LogSourceType::Cluster,
        ];
        let levels = vec![LogLevel::Info, LogLevel::Warning, LogLevel::Error, LogLevel::Debug];
        let messages = vec![
            "System initialized successfully",
            "VM boot sequence started",
            "Network interface configured",
            "Health check completed",
            "Resource allocation updated",
        ];

        use rand::Rng;
        let mut rng = rand::thread_rng();
        
        for _ in 0..5 {
            let source = sources[rng.gen_range(0..sources.len())].clone();
            let level = levels[rng.gen_range(0..levels.len())].clone();
            let message = messages[rng.gen_range(0..messages.len())].to_string();
            
            self.add_log_entry(source, level, message);
        }
    }

    pub fn set_vm_filter(&mut self, filter: crate::tui::types::vm::VmFilter) {
        self.vm_state.vm_filter = filter;
        self.apply_vm_filter();
    }

    pub fn set_node_filter(&mut self, filter: crate::tui::types::node::NodeFilter) {
        self.node_state.node_filter = filter;
        self.apply_node_filter();
    }

    pub fn set_quick_filter(&mut self, filter: String) {
        self.ui_state.quick_filter = filter;
        self.apply_vm_filter();
        self.apply_node_filter();
    }

    pub fn cycle_performance_mode(&mut self) {
        use crate::tui::types::ui::PerformanceMode;
        
        self.ui_state.settings.performance_mode = match self.ui_state.settings.performance_mode {
            PerformanceMode::HighRefresh => PerformanceMode::Balanced,
            PerformanceMode::Balanced => PerformanceMode::PowerSaver,
            PerformanceMode::PowerSaver => PerformanceMode::Debug,
            PerformanceMode::Debug => PerformanceMode::HighRefresh,
        };

        // Update refresh settings based on mode
        let (refresh_rate, update_freq) = match self.ui_state.settings.performance_mode {
            PerformanceMode::HighRefresh => (1, 100),
            PerformanceMode::Balanced => (5, 1000),
            PerformanceMode::PowerSaver => (10, 5000),
            PerformanceMode::Debug => (2, 500),
        };

        self.ui_state.settings.refresh_rate = refresh_rate;
        self.ui_state.settings.update_frequency_ms = update_freq;
    }

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

    pub fn handle_vim_down(&mut self) {
        // Vim-style navigation
        // This is a simplified version - the full implementation is in app.rs
    }

    pub fn handle_vim_up(&mut self) {
        // Vim-style navigation
        // This is a simplified version - the full implementation is in app.rs
    }

    // Event handling - this is simplified, the full implementation remains in app.rs
    pub async fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
        // This is a placeholder - the full implementation should be moved to handlers/
        Ok(())
    }
}