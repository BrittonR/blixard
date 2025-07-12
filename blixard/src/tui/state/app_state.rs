//! Main application state combining all state modules
//!
//! This module provides the central `App` struct that manages all TUI state and
//! coordinates interactions between different subsystems like VMs, nodes, monitoring,
//! and debugging capabilities.

use super::{vm_state::VmState, node_state::NodeState, monitoring_state::MonitoringState, debug_state::DebugState, ui_state::UiState};
use crate::tui::{Event, VmClient};
use crate::tui::types::ui::{AppTab, LogSourceType, LogLevel};
use crate::tui::types::monitoring::EventLevel;
use crate::tui::types::monitoring::SystemEvent;
use crate::tui::types::debug::{DebugLogEntry, DebugLevel, RaftDebugInfo, DebugMetrics};
use crate::BlixardResult;
use std::time::{Duration, Instant};

/// Main application state that manages all TUI components
///
/// The `App` struct serves as the central coordinator for the Blixard TUI,
/// managing state across VMs, cluster nodes, monitoring data, debug information,
/// and UI state. It handles data refresh cycles, event processing, and network
/// client connections.
///
/// # Examples
///
/// ```rust
/// use blixard::tui::state::app_state::App;
///
/// let mut app = App::new();
/// // Connect to cluster and refresh data
/// // app.try_connect().await;
/// // app.refresh_all_data().await?;
/// ```
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
    /// Create a new application instance with default state
    ///
    /// Initializes all state modules with their default values and no network connection.
    /// A connection must be established separately using internal connection methods.
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

    /// Update connection status based on operation latency
    ///
    /// Monitors the duration of network operations and adjusts the connection
    /// status accordingly. High latency may indicate network issues and
    /// triggers appropriate status changes and warnings.
    ///
    /// # Arguments
    ///
    /// * `operation_duration` - How long the last network operation took
    ///
    /// # Behavior
    ///
    /// - Operations over 5 seconds trigger a "PartiallyConnected" status
    /// - Generates warning events for high latency conditions
    /// - Only affects status if currently in Connected state
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

    /// Update cluster health status based on node states
    ///
    /// Analyzes the current state of all cluster nodes to determine
    /// overall cluster health. This provides a quick health indicator
    /// for the dashboard and monitoring views.
    ///
    /// # Health Calculation
    ///
    /// - `Unknown` - No nodes available for assessment
    /// - `Healthy` - All nodes are in healthy state
    /// - `Degraded` - More than half of nodes are healthy
    /// - `Critical` - Half or fewer nodes are healthy
    ///
    /// # Side Effects
    ///
    /// Updates the `cluster_health` field in monitoring state, which
    /// affects dashboard displays and health indicators.
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
            LogSourceType::Vm("vm-001".to_string()),
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
    pub async fn handle_key_event(&mut self, _key: crossterm::event::KeyEvent) -> BlixardResult<()> {
        // This is a placeholder - the full implementation should be moved to handlers/
        Ok(())
    }

    /// Refresh all data from the backend cluster
    ///
    /// Attempts to fetch the latest data from the connected Blixard cluster,
    /// including VM status, node information, and cluster metrics. This method
    /// implements error handling and connection latency tracking.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If all data was refreshed successfully
    /// * `Err(BlixardError)` - If any refresh operation failed
    ///
    /// # Behavior
    ///
    /// - Returns early with a warning event if no cluster connection exists
    /// - Refreshes VMs, cluster metrics, and nodes in sequence
    /// - Updates metrics history and connection latency tracking
    /// - Updates cluster health status based on node states
    /// - Logs successful refresh with data counts
    /// - Accumulates and reports any partial failures
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use blixard::tui::state::app_state::App;
    /// # async fn example(app: &mut App) -> blixard::BlixardResult<()> {
    /// // Refresh all data and handle potential errors
    /// match app.refresh_all_data().await {
    ///     Ok(()) => println!("Data refreshed successfully"),
    ///     Err(e) => eprintln!("Refresh failed: {}", e),
    /// }
    /// # Ok(())
    /// # }
    /// ```
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

        self.ui_state.last_refresh = Instant::now();

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
                "Refreshed: {} VMs, {} nodes",
                self.vm_state.vms.len(),
                self.node_state.nodes.len()
            ),
        );

        Ok(())
    }

    /// Handle incoming application events
    ///
    /// Processes various types of events that drive the TUI application,
    /// including user input, system ticks, network events, and P2P updates.
    /// This is the main event dispatch method for the application.
    ///
    /// # Arguments
    ///
    /// * `event` - The event to process (Tick, Key, Mouse, LogLine, etc.)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the event was handled successfully
    /// * `Err(BlixardError)` - If event handling failed
    ///
    /// # Event Types
    ///
    /// - `Event::Tick` - Regular timer event for data refresh and reconnection checks
    /// - `Event::Key(key)` - Keyboard input events
    /// - `Event::Mouse(mouse)` - Mouse interaction events (currently unused)
    /// - `Event::LogLine(line)` - Log entries to add to debug logs
    /// - `Event::Reconnect` - Trigger reconnection to cluster
    /// - `Event::P2p*Update` - P2P peer, transfer, and image updates
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use blixard::tui::{Event, state::app_state::App};
    /// # async fn example(app: &mut App) -> blixard::BlixardResult<()> {
    /// // Handle a tick event
    /// app.handle_event(Event::Tick).await?;
    ///
    /// // Handle a log line
    /// app.handle_event(Event::LogLine("System started".to_string())).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn handle_event(&mut self, event: Event) -> BlixardResult<()> {
        use crate::tui::types::ui::ConnectionStatus;
        
        match event {
            Event::Tick => {
                // Check if reconnection is needed
                self.check_reconnection_needed();

                // Regular data refresh if connected
                if self.should_refresh()
                    && matches!(self.ui_state.connection_status, ConnectionStatus::Connected)
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
                            self.ui_state.connection_status = ConnectionStatus::Disconnected;
                            self.vm_client = None;
                        }
                    }
                }
            }
            Event::Key(key) => {
                self.handle_key_event(key).await?;
            }
            Event::Mouse(_mouse) => {
                // Mouse events can be handled here if needed
            }
            Event::LogLine(line) => {
                // Add log line to debug logs
                self.add_debug_log(
                    DebugLevel::Info,
                    "system".to_string(),
                    line,
                    None,
                );
            }
            Event::Reconnect => {
                // Trigger reconnection
                self.try_connect().await;
            }
            Event::P2pPeersUpdate(peers) => {
                self.debug_state.p2p_peers = peers;
                self.debug_state.p2p_peer_count = self.debug_state.p2p_peers.len();
            }
            Event::P2pTransfersUpdate(transfers) => {
                self.debug_state.p2p_transfers = transfers;
            }
            Event::P2pImagesUpdate(images) => {
                self.debug_state.p2p_images = images;
                self.debug_state.p2p_shared_images = self.debug_state.p2p_images.len();
            }
        }
        Ok(())
    }

    /// Attempt to establish a connection to the Blixard cluster
    ///
    /// Tries to connect to the cluster at a hardcoded endpoint (127.0.0.1:7001)
    /// with connection timing and error handling. Updates connection status and
    /// performs initial data refresh on successful connection.
    ///
    /// # Behavior
    ///
    /// - Sets connection status to "Connecting" during attempt
    /// - Creates a new VmClient instance with retry logic
    /// - On success: Updates status to "Connected" and refreshes data
    /// - On failure: Sets error status with failure message
    /// - Tracks connection duration for latency monitoring
    /// - Logs connection events for debugging
    ///
    /// # Note
    ///
    /// This method currently uses a hardcoded endpoint. Future versions
    /// should accept configurable cluster addresses.
    async fn try_connect(&mut self) {
        use crate::tui::types::ui::ConnectionStatus;
        
        let endpoint = "127.0.0.1:7001";
        self.ui_state.connection_status = ConnectionStatus::Connecting;
        
        self.ui_state.status_message = Some("Connecting to cluster...".to_string());
        self.add_event(
            EventLevel::Info,
            "Connection".to_string(),
            "Attempting to connect to cluster".to_string(),
        );

        let start_time = Instant::now();
        match VmClient::new(endpoint).await {
            Ok(client) => {
                let connect_duration = start_time.elapsed();
                self.vm_client = Some(client);

                // Update connection status
                self.ui_state.connection_status = ConnectionStatus::Connected;
                
                self.ui_state.status_message = Some(format!(
                    "Connected to cluster ({}ms)",
                    connect_duration.as_millis()
                ));
                
                self.add_event(
                    EventLevel::Info,
                    "Connection".to_string(),
                    format!("Successfully connected in {:?}", connect_duration),
                );

                // Perform initial data refresh
                let _ = self.refresh_all_data().await;
            }
            Err(e) => {
                self.ui_state.connection_status = ConnectionStatus::Error(e.to_string());
                self.ui_state.status_message = Some(format!("Connection failed: {}", e));
                
                self.add_event(
                    EventLevel::Error,
                    "Connection".to_string(),
                    format!("Failed to connect: {}", e),
                );
            }
        }
    }

    /// Refresh VM list from the backend
    async fn refresh_vm_list(&mut self) -> BlixardResult<()> {
        if let Some(client) = &mut self.vm_client {
            match client.list_vms().await {
                Ok(vms) => {
                    self.vm_state.vms = vms;
                    self.vm_state.apply_filter();
                    self.ui_state.error_message = None;
                }
                Err(e) => {
                    let error_msg = format!("Failed to refresh VMs: {}", e);
                    self.ui_state.error_message = Some(error_msg.clone());
                    return Err(crate::BlixardError::Internal { message: error_msg });
                }
            }
        }
        Ok(())
    }

    /// Refresh cluster metrics from the backend
    async fn refresh_cluster_metrics(&mut self) -> BlixardResult<()> {
        use crate::tui::types::cluster::ClusterMetrics;
        use blixard_core::types::VmStatus;
        
        if let Some(client) = &mut self.vm_client {
            match client.get_cluster_status().await {
                Ok(status) => {
                    self.monitoring_state.cluster_metrics = ClusterMetrics {
                        total_nodes: status.nodes.len() as u32,
                        healthy_nodes: status.nodes.len() as u32, // Simplified: assume all nodes are healthy
                        total_vms: self.vm_state.vms.len() as u32,
                        running_vms: self.vm_state.vms
                            .iter()
                            .filter(|vm| vm.status == VmStatus::Running)
                            .count() as u32,
                        total_cpu: status.nodes.len() as u32 * 8, // TODO: Get actual CPU count
                        used_cpu: self.vm_state.vms.iter().map(|vm| vm.vcpus).sum::<u32>(),
                        total_memory: status.nodes.len() as u64 * 16 * 1024, // TODO: Get actual memory
                        used_memory: self.vm_state.vms.iter().map(|vm| vm.memory as u64).sum(),
                        leader_id: Some(status.leader_id),
                        raft_term: status.term,
                        last_updated: Instant::now(),
                    };
                    self.ui_state.error_message = None;
                }
                Err(e) => {
                    let error_msg = format!("Failed to refresh cluster: {}", e);
                    self.ui_state.error_message = Some(error_msg.clone());
                    return Err(crate::BlixardError::Internal { message: error_msg });
                }
            }
        }
        Ok(())
    }

    /// Refresh node list from the backend
    async fn refresh_node_list(&mut self) -> BlixardResult<()> {
        use crate::tui::types::node::{NodeInfo, NodeStatus, NodeRole};
        
        if let Some(client) = &mut self.vm_client {
            match client.get_cluster_status().await {
                Ok(status) => {
                    self.node_state.nodes = status.nodes
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
                            cpu_usage: 20.0 + rand::random::<f32>() * 60.0, // Simulated CPU usage 20-80%
                            memory_usage: 30.0 + rand::random::<f32>() * 50.0, // Simulated memory usage 30-80%
                            vm_count: self.vm_state.vms.iter().filter(|vm| vm.node_id == node.id).count() as u32,
                            last_seen: Some(Instant::now()),
                            version: None, // TODO: Add to proto
                        })
                        .collect();
                    self.node_state.apply_filter();
                    self.ui_state.error_message = None;
                }
                Err(e) => {
                    let error_msg = format!("Failed to refresh nodes: {}", e);
                    self.ui_state.error_message = Some(error_msg.clone());
                    return Err(crate::BlixardError::Internal { message: error_msg });
                }
            }
        }
        Ok(())
    }
}