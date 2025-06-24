use crate::BlixardResult;
use super::{Event, VmClient};
use blixard_core::types::VmStatus;
use ratatui::widgets::ListState;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct VmInfo {
    pub name: String,
    pub status: VmStatus,
    pub vcpus: u32,
    pub memory: u32,
    pub node_id: u64,
    pub ip_address: Option<String>, // VM IP address for routed networking
    pub uptime: Option<String>,
    pub cpu_usage: Option<f32>,
    pub memory_usage: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    VmList,
    VmDetails,
    VmCreate,
    VmLogs,
    Help,
    SshSession,
    RaftStatus,
    ClusterResources, // New mode for cluster resource monitoring
    ServerStartup, // New mode for server startup popup
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    Editing,
}

pub struct App {
    /// Current application mode
    pub mode: AppMode,
    /// Input mode for forms
    pub input_mode: InputMode,
    /// Whether the app should quit
    pub should_quit: bool,
    /// VM client for API calls
    pub vm_client: Option<VmClient>,
    /// List of VMs
    pub vms: Vec<VmInfo>,
    /// VM list state for navigation
    pub vm_list_state: ListState,
    /// Selected VM index
    pub selected_vm: Option<usize>,
    /// VM logs for the selected VM
    pub vm_logs: Vec<String>,
    /// Log scroll position
    pub log_scroll: u16,
    /// VM creation form fields
    pub create_form: CreateVmForm,
    /// Status message
    pub status_message: Option<String>,
    /// Error message
    pub error_message: Option<String>,
    /// Log update receiver
    pub log_receiver: Option<mpsc::UnboundedReceiver<String>>,
    /// Tick counter for auto-refresh
    pub tick_counter: u32,
    /// Auto-refresh interval in ticks (refresh every 10 ticks = ~2.5 seconds)
    pub auto_refresh_interval: u32,
    /// Process info for the selected VM
    pub vm_process_info: Option<VmProcessInfo>,
    /// Live log panel logs (always visible)
    pub live_logs: Vec<String>,
    /// Live log panel receiver for continuous logging
    pub live_log_receiver: Option<mpsc::UnboundedReceiver<String>>,
    /// Currently followed VM for live logs (None = all VMs)
    pub live_log_vm: Option<String>,
    /// Cluster and node status information
    pub cluster_info: ClusterInfo,
    /// SSH connection information for selected VM
    pub ssh_info: Option<SshInfo>,
    /// Active SSH session for embedded terminal
    pub ssh_session: Option<SshSession>,
    /// Server startup popup state
    pub server_startup: ServerStartupState,
    /// Cluster resource information
    pub cluster_resources: Option<ClusterResourceInfo>,
}

#[derive(Debug, Clone)]
pub struct CreateVmForm {
    pub name: String,
    pub vcpus: String,
    pub memory: String,
    pub placement_strategy: usize, // Index into placement strategy options
    pub current_field: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlacementStrategy {
    MostAvailable,
    LeastAvailable,
    RoundRobin,
    Manual,
}

impl PlacementStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            PlacementStrategy::MostAvailable => "Most Available",
            PlacementStrategy::LeastAvailable => "Least Available (Bin Pack)",
            PlacementStrategy::RoundRobin => "Round Robin",
            PlacementStrategy::Manual => "Manual Selection",
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

impl ClusterResourceInfo {
    pub fn cpu_utilization(&self) -> f64 {
        if self.total_vcpus > 0 {
            (self.used_vcpus as f64 / self.total_vcpus as f64) * 100.0
        } else {
            0.0
        }
    }
    
    pub fn memory_utilization(&self) -> f64 {
        if self.total_memory_mb > 0 {
            (self.used_memory_mb as f64 / self.total_memory_mb as f64) * 100.0
        } else {
            0.0
        }
    }
    
    pub fn disk_utilization(&self) -> f64 {
        if self.total_disk_gb > 0 {
            (self.used_disk_gb as f64 / self.total_disk_gb as f64) * 100.0
        } else {
            0.0
        }
    }
    
    pub fn utilization_percentages(&self) -> (f64, f64, f64) {
        (self.cpu_utilization(), self.memory_utilization(), self.disk_utilization())
    }
}

impl Default for CreateVmForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            vcpus: "2".to_string(),
            memory: "1024".to_string(),
            placement_strategy: 0, // Default to MostAvailable
            current_field: 0,
        }
    }
}

impl App {
    pub async fn new() -> BlixardResult<Self> {
        // Try to connect to the server, but don't fail if it's not available
        // Try multiple times with a short delay to handle transient connection issues
        let (vm_client, server_available) = {
            let mut attempts = 0;
            let max_attempts = 3;
            
            loop {
                match VmClient::new("127.0.0.1:7001").await {
                    Ok(client) => break (Some(client), true),
                    Err(_) if attempts >= max_attempts - 1 => break (None, false),
                    Err(_) => {
                        attempts += 1;
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    }
                }
            }
        };
        
        let mut app = Self {
            mode: if server_available { AppMode::VmList } else { AppMode::ServerStartup },
            input_mode: InputMode::Normal,
            should_quit: false,
            vm_client,
            vms: Vec::new(),
            vm_list_state: ListState::default(),
            selected_vm: None,
            vm_logs: Vec::new(),
            log_scroll: 0,
            create_form: CreateVmForm::default(),
            status_message: None,
            error_message: None,
            log_receiver: None,
            tick_counter: 0,
            auto_refresh_interval: 10, // Refresh every 10 ticks (2.5 seconds)
            vm_process_info: None,
            live_logs: Vec::new(),
            live_log_receiver: None,
            live_log_vm: None,
            cluster_info: ClusterInfo::default(),
            ssh_info: None,
            ssh_session: None,
            server_startup: ServerStartupState::default(),
            cluster_resources: None,
        };
        
        // Only start live logging and get cluster status if server is available
        if server_available {
            // Start live log following for all VMs by default
            if let Err(e) = app.start_live_log_following(None).await {
                app.live_logs.push(format!("Failed to start live log following: {}", e));
            } else {
                app.live_logs.push("🔄 Live log following started for all VMs".to_string());
            }
            
            // Get initial cluster status
            let _ = app.refresh_cluster_status().await; // Don't fail initialization if cluster status fails
        } else {
            app.live_logs.push("⚠️  No server connection - use popup to start server".to_string());
        }
        
        Ok(app)
    }

    /// Handle events and update app state
    pub async fn handle_event(&mut self, event: Event) -> BlixardResult<()> {
        match event {
            Event::Key(key) => self.handle_key_event(key).await?,
            Event::Tick => self.handle_tick().await?,
            Event::LogLine(line) => {
                self.vm_logs.push(line);
                // Keep only the last 1000 log lines
                if self.vm_logs.len() > 1000 {
                    self.vm_logs.remove(0);
                }
            }
        }
        Ok(())
    }

    async fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
        use crossterm::event::{KeyCode, KeyModifiers};

        // Global key bindings
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), KeyModifiers::NONE) if self.input_mode == InputMode::Normal => {
                self.should_quit = true;
                return Ok(());
            }
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                self.should_quit = true;
                return Ok(());
            }
            (KeyCode::Esc, _) => {
                if self.input_mode == InputMode::Editing {
                    self.input_mode = InputMode::Normal;
                } else {
                    // Stop log following when leaving logs view
                    if matches!(self.mode, AppMode::VmLogs) {
                        self.stop_log_following();
                    }
                    
                    self.mode = AppMode::VmList;
                    self.error_message = None;
                    self.vm_process_info = None; // Clear process info when leaving details
                }
                return Ok(());
            }
            _ => {}
        }

        match self.mode {
            AppMode::VmList => self.handle_vm_list_keys(key).await?,
            AppMode::VmDetails => self.handle_vm_details_keys(key).await?,
            AppMode::VmCreate => self.handle_vm_create_keys(key).await?,
            AppMode::VmLogs => self.handle_vm_logs_keys(key).await?,
            AppMode::Help => self.handle_help_keys(key).await?,
            AppMode::SshSession => self.handle_ssh_keys(key).await?,
            AppMode::RaftStatus => self.handle_raft_status_keys(key).await?,
            AppMode::ClusterResources => self.handle_cluster_resources_keys(key).await?,
            AppMode::ServerStartup => self.handle_server_startup_keys(key).await?,
        }

        Ok(())
    }

    async fn handle_vm_list_keys(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(selected) = self.vm_list_state.selected() {
                    if selected > 0 {
                        self.vm_list_state.select(Some(selected - 1));
                        self.selected_vm = Some(selected - 1);
                    }
                } else if !self.vms.is_empty() {
                    self.vm_list_state.select(Some(0));
                    self.selected_vm = Some(0);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(selected) = self.vm_list_state.selected() {
                    if selected < self.vms.len().saturating_sub(1) {
                        self.vm_list_state.select(Some(selected + 1));
                        self.selected_vm = Some(selected + 1);
                    }
                } else if !self.vms.is_empty() {
                    self.vm_list_state.select(Some(0));
                    self.selected_vm = Some(0);
                }
            }
            KeyCode::Enter => {
                if let Some(_vm_index) = self.selected_vm {
                    self.mode = AppMode::VmDetails;
                }
            }
            KeyCode::Char('n') => {
                self.mode = AppMode::VmCreate;
                self.create_form = CreateVmForm::default();
                self.input_mode = InputMode::Editing;
            }
            KeyCode::Char('r') => {
                self.refresh_vm_list().await?;
            }
            KeyCode::Char('s') => {
                if let Some(vm_index) = self.selected_vm {
                    if let Some(vm) = self.vms.get(vm_index) {
                        let vm_name = vm.name.clone();
                        self.start_vm(&vm_name).await?;
                    }
                }
            }
            KeyCode::Char('x') => {
                if let Some(vm_index) = self.selected_vm {
                    if let Some(vm) = self.vms.get(vm_index) {
                        let vm_name = vm.name.clone();
                        self.stop_vm(&vm_name).await?;
                    }
                }
            }
            KeyCode::Char('l') => {
                if let Some(vm_index) = self.selected_vm {
                    if let Some(vm) = self.vms.get(vm_index) {
                        let vm_name = vm.name.clone();
                        self.start_log_viewer(&vm_name).await?;
                        self.mode = AppMode::VmLogs;
                    }
                }
            }
            KeyCode::Char('?') => {
                self.mode = AppMode::Help;
            }
            KeyCode::Char('R') => {
                self.mode = AppMode::RaftStatus;
            }
            KeyCode::Char('C') => {
                // Switch to cluster resources view
                self.mode = AppMode::ClusterResources;
                // Refresh cluster resources when entering this mode
                if let Err(e) = self.refresh_cluster_resources().await {
                    self.error_message = Some(format!("Failed to load cluster resources: {}", e));
                }
            }
            KeyCode::Char('f') => {
                // Follow logs for selected VM in live panel
                if let Some(vm_index) = self.selected_vm {
                    if let Some(vm) = self.vms.get(vm_index) {
                        let vm_name = vm.name.clone();
                        if let Err(e) = self.start_live_log_following(Some(vm_name.clone())).await {
                            self.error_message = Some(format!("Failed to follow VM logs: {}", e));
                        } else {
                            self.status_message = Some(format!("Now following logs for VM '{}'", vm_name));
                        }
                    }
                }
            }
            KeyCode::Char('F') => {
                // Follow logs for all VMs in live panel
                if let Err(e) = self.start_live_log_following(None).await {
                    self.error_message = Some(format!("Failed to follow all VM logs: {}", e));
                } else {
                    self.status_message = Some("Now following logs for all VMs".to_string());
                }
            }
            KeyCode::Char('c') => {
                // SSH Connect to selected VM
                if let Some(vm_index) = self.selected_vm {
                    if let Some(vm) = self.vms.get(vm_index) {
                        if matches!(vm.status, blixard_core::types::VmStatus::Running) {
                            let vm_name = vm.name.clone();
                            self.start_ssh_session(&vm_name).await?;
                            self.mode = AppMode::SshSession;
                        } else {
                            self.error_message = Some(format!("VM '{}' must be running to SSH", vm.name));
                        }
                    }
                }
            }
            KeyCode::Char('D') => {
                // Delete selected VM (requires VM to be stopped or failed)
                if let Some(vm_index) = self.selected_vm {
                    if let Some(vm) = self.vms.get(vm_index) {
                        if matches!(vm.status, blixard_core::types::VmStatus::Stopped | blixard_core::types::VmStatus::Failed) {
                            let vm_name = vm.name.clone();
                            self.delete_vm(&vm_name).await?;
                        } else {
                            self.error_message = Some(format!("VM '{}' must be stopped or failed before deletion", vm.name));
                        }
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    async fn handle_vm_details_keys(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Char('s') => {
                if let Some(vm_index) = self.selected_vm {
                    if let Some(vm) = self.vms.get(vm_index) {
                        let vm_name = vm.name.clone();
                        self.start_vm(&vm_name).await?;
                    }
                }
            }
            KeyCode::Char('x') => {
                if let Some(vm_index) = self.selected_vm {
                    if let Some(vm) = self.vms.get(vm_index) {
                        let vm_name = vm.name.clone();
                        self.stop_vm(&vm_name).await?;
                    }
                }
            }
            KeyCode::Char('l') => {
                if let Some(vm_index) = self.selected_vm {
                    if let Some(vm) = self.vms.get(vm_index) {
                        let vm_name = vm.name.clone();
                        self.start_log_viewer(&vm_name).await?;
                        self.mode = AppMode::VmLogs;
                    }
                }
            }
            KeyCode::Char('c') => {
                // SSH Connect to selected VM
                if let Some(vm_index) = self.selected_vm {
                    if let Some(vm) = self.vms.get(vm_index) {
                        if matches!(vm.status, blixard_core::types::VmStatus::Running) {
                            let vm_name = vm.name.clone();
                            self.start_ssh_session(&vm_name).await?;
                            self.mode = AppMode::SshSession;
                        } else {
                            self.error_message = Some(format!("VM '{}' must be running to SSH", vm.name));
                        }
                    }
                }
            }
            KeyCode::Char('D') => {
                // Delete selected VM (requires VM to be stopped or failed)
                if let Some(vm_index) = self.selected_vm {
                    if let Some(vm) = self.vms.get(vm_index) {
                        if matches!(vm.status, blixard_core::types::VmStatus::Stopped | blixard_core::types::VmStatus::Failed) {
                            let vm_name = vm.name.clone();
                            self.delete_vm(&vm_name).await?;
                            self.mode = AppMode::VmList; // Return to list after deletion
                        } else {
                            self.error_message = Some(format!("VM '{}' must be stopped or failed before deletion", vm.name));
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

        if self.input_mode == InputMode::Editing {
            match key.code {
                KeyCode::Enter => {
                    if self.create_form.current_field < 3 {
                        self.create_form.current_field += 1;
                    } else {
                        // Submit form
                        self.create_vm().await?;
                    }
                }
                KeyCode::Tab => {
                    self.create_form.current_field = (self.create_form.current_field + 1) % 4;
                }
                KeyCode::BackTab => {
                    self.create_form.current_field = if self.create_form.current_field == 0 {
                        3
                    } else {
                        self.create_form.current_field - 1
                    };
                }
                KeyCode::Backspace => {
                    match self.create_form.current_field {
                        0 => { self.create_form.name.pop(); }
                        1 => { self.create_form.vcpus.pop(); }
                        2 => { self.create_form.memory.pop(); }
                        _ => {}
                    }
                }
                KeyCode::Char(c) => {
                    match self.create_form.current_field {
                        0 => self.create_form.name.push(c),
                        1 => if c.is_ascii_digit() { self.create_form.vcpus.push(c); }
                        2 => if c.is_ascii_digit() { self.create_form.memory.push(c); }
                        3 => {
                            // For placement strategy, cycle through options with arrow keys or numbers
                            match c {
                                '1' => self.create_form.placement_strategy = 0, // MostAvailable
                                '2' => self.create_form.placement_strategy = 1, // LeastAvailable
                                '3' => self.create_form.placement_strategy = 2, // RoundRobin  
                                '4' => self.create_form.placement_strategy = 3, // Manual
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
                KeyCode::Up | KeyCode::Left => {
                    if self.create_form.current_field == 3 {
                        // Cycle placement strategy backwards
                        self.create_form.placement_strategy = if self.create_form.placement_strategy == 0 {
                            3
                        } else {
                            self.create_form.placement_strategy - 1
                        };
                    }
                }
                KeyCode::Down | KeyCode::Right => {
                    if self.create_form.current_field == 3 {
                        // Cycle placement strategy forwards
                        self.create_form.placement_strategy = (self.create_form.placement_strategy + 1) % 4;
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    async fn handle_vm_logs_keys(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.log_scroll = self.log_scroll.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.log_scroll = self.log_scroll.saturating_add(1);
            }
            KeyCode::PageUp => {
                self.log_scroll = self.log_scroll.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.log_scroll = self.log_scroll.saturating_add(10);
            }
            KeyCode::Home => {
                self.log_scroll = 0;
            }
            KeyCode::End => {
                // Jump to the end and follow new logs
                self.log_scroll = u16::MAX;
            }
            _ => {}
        }

        Ok(())
    }

    async fn handle_help_keys(&mut self, _key: crossterm::event::KeyEvent) -> BlixardResult<()> {
        // Any key exits help
        self.mode = AppMode::VmList;
        Ok(())
    }

    async fn handle_raft_status_keys(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Char('r') => {
                // Refresh Raft status
                self.refresh_cluster_status().await?;
            }
            _ => {
                // Any other key exits Raft status view
                self.mode = AppMode::VmList;
            }
        }
        Ok(())
    }

    async fn handle_ssh_keys(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
        use crossterm::event::{KeyCode, KeyModifiers};

        match key.code {
            KeyCode::Esc => {
                // Exit SSH session
                self.close_ssh_session();
                self.mode = AppMode::VmList;
            }
            KeyCode::Enter => {
                // Send current input buffer as command
                if let Some(session) = &mut self.ssh_session {
                    if !session.input_buffer.trim().is_empty() {
                        let command = session.input_buffer.clone();
                        session.output_lines.push(format!("$ {}", command));
                        
                        // Send command to SSH process (if connected)
                        if let Some(sender) = &session.input_sender {
                            let _ = sender.send(command + "\n");
                        }
                        
                        session.input_buffer.clear();
                    }
                }
            }
            KeyCode::Backspace => {
                // Remove last character from input buffer
                if let Some(session) = &mut self.ssh_session {
                    session.input_buffer.pop();
                }
            }
            KeyCode::Char(c) => {
                // Add character to input buffer
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match c {
                        'c' => {
                            // Ctrl+C - interrupt current command
                            if let Some(session) = &mut self.ssh_session {
                                if let Some(sender) = &session.input_sender {
                                    let _ = sender.send("\x03".to_string()); // Send Ctrl+C
                                }
                                session.output_lines.push("^C".to_string());
                            }
                        }
                        'd' => {
                            // Ctrl+D - EOF/logout
                            if let Some(session) = &mut self.ssh_session {
                                if let Some(sender) = &session.input_sender {
                                    let _ = sender.send("\x04".to_string()); // Send Ctrl+D
                                }
                            }
                        }
                        _ => {}
                    }
                } else {
                    // Regular character input
                    if let Some(session) = &mut self.ssh_session {
                        session.input_buffer.push(c);
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    async fn handle_server_startup_keys(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match self.server_startup.status {
            ServerStartupStatus::Prompt => {
                if self.server_startup.show_config {
                    // Handle configuration input
                    match key.code {
                        KeyCode::Up | KeyCode::Char('k') => {
                            if self.server_startup.config.current_field > 0 {
                                self.server_startup.config.current_field -= 1;
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if self.server_startup.config.current_field < 3 {
                                self.server_startup.config.current_field += 1;
                            }
                        }
                        KeyCode::Enter => {
                            self.start_server_with_config().await?;
                        }
                        KeyCode::Esc => {
                            self.server_startup.show_config = false;
                        }
                        KeyCode::Char(c) => {
                            match self.server_startup.config.current_field {
                                0 => self.server_startup.config.node_id.push(c),
                                1 => self.server_startup.config.bind_address.push(c),
                                2 => self.server_startup.config.data_dir.push(c),
                                3 => self.server_startup.config.vm_backend.push(c),
                                _ => {}
                            }
                        }
                        KeyCode::Backspace => {
                            match self.server_startup.config.current_field {
                                0 => { self.server_startup.config.node_id.pop(); }
                                1 => { self.server_startup.config.bind_address.pop(); }
                                2 => { self.server_startup.config.data_dir.pop(); }
                                3 => { self.server_startup.config.vm_backend.pop(); }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                } else {
                    // Handle main popup options
                    match key.code {
                        KeyCode::Up | KeyCode::Char('k') => {
                            if self.server_startup.selected_option > 0 {
                                self.server_startup.selected_option -= 1;
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if self.server_startup.selected_option < 2 {
                                self.server_startup.selected_option += 1;
                            }
                        }
                        KeyCode::Enter | KeyCode::Char(' ') => {
                            match self.server_startup.selected_option {
                                0 => {
                                    // Yes - start with defaults
                                    self.start_server_with_defaults().await?;
                                }
                                1 => {
                                    // No - quit
                                    self.should_quit = true;
                                }
                                2 => {
                                    // Configure - show config form
                                    self.server_startup.show_config = true;
                                }
                                _ => {}
                            }
                        }
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            // Quick yes
                            self.start_server_with_defaults().await?;
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') => {
                            // Quick no
                            self.should_quit = true;
                        }
                        KeyCode::Char('c') | KeyCode::Char('C') => {
                            // Quick configure
                            self.server_startup.show_config = true;
                        }
                        _ => {}
                    }
                }
            }
            ServerStartupStatus::Starting => {
                // Can't do anything while starting, just wait
            }
            ServerStartupStatus::Success => {
                // Any key continues to main app
                self.mode = AppMode::VmList;
                self.refresh_vm_list().await?;
            }
            ServerStartupStatus::Failed => {
                match key.code {
                    KeyCode::Enter | KeyCode::Char('r') => {
                        // Retry
                        self.server_startup.status = ServerStartupStatus::Prompt;
                    }
                    KeyCode::Char('q') | KeyCode::Esc => {
                        // Quit
                        self.should_quit = true;
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    async fn handle_cluster_resources_keys(&mut self, key: crossterm::event::KeyEvent) -> BlixardResult<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Char('r') => {
                // Refresh cluster resources
                if let Err(e) = self.refresh_cluster_resources().await {
                    self.error_message = Some(format!("Failed to refresh cluster resources: {}", e));
                }
            }
            _ => {
                // Any other key exits cluster resources view
                self.mode = AppMode::VmList;
            }
        }
        Ok(())
    }

    async fn handle_tick(&mut self) -> BlixardResult<()> {
        // Increment tick counter
        self.tick_counter += 1;
        
        // Auto-refresh VM list every interval
        if self.mode == AppMode::VmList && self.tick_counter % self.auto_refresh_interval == 0 {
            // Only auto-refresh in VM list mode to avoid interrupting user interactions
            if let Err(e) = self.refresh_vm_list().await {
                self.error_message = Some(format!("Auto-refresh failed: {}", e));
            }
        }

        // Refresh cluster status every 30 ticks (7.5 seconds) to avoid too frequent requests
        if self.tick_counter % 30 == 0 {
            let _ = self.refresh_cluster_status().await; // Silently fail
        }

        // Update process info when in VM details view
        if self.mode == AppMode::VmDetails && self.tick_counter % 5 == 0 {
            if let Some(vm_index) = self.selected_vm {
                if let Some(vm) = self.vms.get(vm_index) {
                    match self.get_vm_process_info(&vm.name).await {
                        Ok(process_info) => {
                            self.vm_process_info = Some(process_info);
                        }
                        Err(_) => {
                            // Don't show errors for process info updates
                            self.vm_process_info = None;
                        }
                    }
                }
            }
        }
        
        // Clear status/error messages after a while
        if self.tick_counter % 20 == 0 { // Clear after 5 seconds
            self.status_message = None;
            self.error_message = None;
        }
        
        // Handle any log updates if in log mode
        if let Some(receiver) = &mut self.log_receiver {
            let mut new_logs = false;
            while let Ok(line) = receiver.try_recv() {
                self.vm_logs.push(line);
                new_logs = true;
                if self.vm_logs.len() > 1000 {
                    self.vm_logs.remove(0);
                }
            }
            
            // Auto-scroll to bottom when new logs arrive and we're in log mode
            if new_logs && self.mode == AppMode::VmLogs {
                // Set scroll to follow the end - the UI renderer will handle the proper positioning
                self.log_scroll = u16::MAX;
            }
        }
        
        // Handle live log panel updates (always active)
        if let Some(receiver) = &mut self.live_log_receiver {
            while let Ok(line) = receiver.try_recv() {
                self.live_logs.push(line);
                // Keep only the last 100 lines in the live panel
                if self.live_logs.len() > 100 {
                    self.live_logs.remove(0);
                }
            }
        }
        
        // Handle SSH session output updates
        if let Some(session) = &mut self.ssh_session {
            if let Some(receiver) = &mut session.output_receiver {
                while let Ok(line) = receiver.try_recv() {
                    session.output_lines.push(line);
                    // Keep only the last 1000 lines
                    if session.output_lines.len() > 1000 {
                        session.output_lines.remove(0);
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn refresh_vm_list(&mut self) -> BlixardResult<()> {
        if let Some(client) = &mut self.vm_client {
            match client.list_vms().await {
                Ok(vms) => {
                    self.vms = vms;
                    self.error_message = None;
                    
                    // Adjust selection if needed
                    if self.vms.is_empty() {
                        self.vm_list_state.select(None);
                        self.selected_vm = None;
                    } else if self.selected_vm.is_none() || self.selected_vm.unwrap() >= self.vms.len() {
                        self.vm_list_state.select(Some(0));
                        self.selected_vm = Some(0);
                    }
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to list VMs: {}", e));
                    self.status_message = None;
                }
            }
        } else {
            self.error_message = Some("No server connection available".to_string());
        }

        Ok(())
    }

    pub async fn refresh_cluster_status(&mut self) -> BlixardResult<()> {
        if let Some(client) = &mut self.vm_client {
            match client.get_cluster_status().await {
                Ok(cluster_info) => {
                    self.cluster_info = cluster_info;
                    // Don't show error message for successful cluster status updates
                }
                Err(_) => {
                    // Silently fail cluster status updates to avoid cluttering UI
                    // Keep previous cluster info
                }
            }
        }
        
        Ok(())
    }

    pub async fn refresh_cluster_resources(&mut self) -> BlixardResult<()> {
        if let Some(client) = &mut self.vm_client {
            match client.get_cluster_resources().await {
                Ok(cluster_resources) => {
                    self.cluster_resources = Some(cluster_resources);
                }
                Err(e) => {
                    // Provide a more helpful error message for common cases
                    let error_msg = if e.to_string().contains("VM manager not initialized") {
                        "VM manager is still initializing. Please wait a moment and try again (press 'r')".to_string()
                    } else if e.to_string().contains("No server connection") {
                        "Cannot connect to blixard server. Ensure a node is running".to_string()
                    } else {
                        format!("Failed to get cluster resources: {}", e)
                    };
                    
                    return Err(crate::BlixardError::Internal { message: error_msg });
                }
            }
        } else {
            return Err(crate::BlixardError::Internal { 
                message: "No server connection available. Start a blixard node first".to_string() 
            });
        }
        
        Ok(())
    }

    async fn start_vm(&mut self, name: &str) -> BlixardResult<()> {
        if let Some(client) = &mut self.vm_client {
            match client.start_vm(name).await {
                Ok(_) => {
                    self.status_message = Some(format!("✓ Started VM '{}'", name));
                    self.error_message = None;
                    // Refresh to update status
                    self.refresh_vm_list().await?;
                    
                    // If we're following all VMs, refresh the live log following to include the newly started VM
                    if self.live_log_vm.is_none() && self.live_log_receiver.is_some() {
                        if let Err(e) = self.start_live_log_following(None).await {
                            // Don't fail the whole operation, just log the error
                            self.live_logs.push(format!("Failed to refresh live log following: {}", e));
                        } else {
                            self.live_logs.push(format!("🔄 Refreshed live log following for started VM '{}'", name));
                        }
                    }
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to start VM '{}': {}", name, e));
                    self.status_message = None;
                }
            }
        } else {
            self.error_message = Some("No server connection available".to_string());
        }

        Ok(())
    }

    async fn stop_vm(&mut self, name: &str) -> BlixardResult<()> {
        if let Some(client) = &mut self.vm_client {
            match client.stop_vm(name).await {
                Ok(_) => {
                    self.status_message = Some(format!("✓ Stopped VM '{}'", name));
                    self.error_message = None;
                    // Refresh to update status
                    self.refresh_vm_list().await?;
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to stop VM '{}': {}", name, e));
                    self.status_message = None;
                }
            }
        } else {
            self.error_message = Some("No server connection available".to_string());
        }

        Ok(())
    }

    async fn delete_vm(&mut self, name: &str) -> BlixardResult<()> {
        if let Some(client) = &mut self.vm_client {
            match client.delete_vm(name).await {
                Ok(_) => {
                    self.status_message = Some(format!("✓ Deleted VM '{}'", name));
                    self.error_message = None;
                    // Refresh to update list (VM should be removed)
                    self.refresh_vm_list().await?;
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to delete VM '{}': {}", name, e));
                    self.status_message = None;
                }
            }
        } else {
            self.error_message = Some("No server connection available".to_string());
        }

        Ok(())
    }

    async fn create_vm(&mut self) -> BlixardResult<()> {
        let name = self.create_form.name.trim().to_string();
        if name.is_empty() {
            self.error_message = Some("VM name cannot be empty".to_string());
            return Ok(());
        }

        let vcpus: u32 = self.create_form.vcpus.parse().unwrap_or(2);
        let memory: u32 = self.create_form.memory.parse().unwrap_or(1024);
        
        // Convert placement strategy index to PlacementStrategy enum
        let placement_strategy = match self.create_form.placement_strategy {
            0 => PlacementStrategy::MostAvailable,
            1 => PlacementStrategy::LeastAvailable,
            2 => PlacementStrategy::RoundRobin,
            3 => PlacementStrategy::Manual,
            _ => PlacementStrategy::MostAvailable, // Default fallback
        };

        if let Some(client) = &mut self.vm_client {
            match client.create_vm_with_scheduling(&name, vcpus, memory, placement_strategy).await {
                Ok((selected_node_id, placement_reason)) => {
                    self.status_message = Some(format!("✓ Created VM '{}' on node {} ({})", name, selected_node_id, placement_reason));
                    self.error_message = None;
                    self.mode = AppMode::VmList;
                    self.input_mode = InputMode::Normal;
                    self.refresh_vm_list().await?;
                    
                    // If we're following all VMs, refresh the live log following to include the new VM
                    if self.live_log_vm.is_none() && self.live_log_receiver.is_some() {
                        if let Err(e) = self.start_live_log_following(None).await {
                            // Don't fail the whole operation, just log the error
                            self.live_logs.push(format!("Failed to refresh live log following: {}", e));
                        } else {
                            self.live_logs.push(format!("🔄 Refreshed live log following for new VM '{}'", name));
                        }
                    }
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to create VM '{}': {}", name, e));
                    self.status_message = None;
                }
            }
        } else {
            self.error_message = Some("No server connection available".to_string());
        }

        Ok(())
    }


    async fn start_log_viewer(&mut self, vm_name: &str) -> BlixardResult<()> {
        // Clear existing logs
        self.vm_logs.clear();
        self.log_scroll = 0;

        // Start with loading recent logs for context
        match self.load_systemd_logs(vm_name).await {
            Ok(logs) => {
                self.vm_logs = logs;
                if self.vm_logs.is_empty() {
                    self.vm_logs.push("Starting log viewer...".to_string());
                    self.vm_logs.push("Waiting for logs...".to_string());
                }
            }
            Err(e) => {
                self.vm_logs.push(format!("Failed to load initial logs: {}", e));
                self.vm_logs.push(format!("Try running: journalctl --user -u blixard-vm-{} -n 50", vm_name));
            }
        }

        // Start live log following
        if let Err(e) = self.start_log_following(vm_name).await {
            self.vm_logs.push(format!("Live log following failed: {}", e));
            self.vm_logs.push("Showing static logs only".to_string());
        } else {
            self.vm_logs.push("".to_string());
            self.vm_logs.push("🔄 Following live logs (Press ESC to stop)".to_string());
        }

        // Set scroll position to follow the end (most recent logs)
        // Use a high value that the UI renderer will clamp appropriately
        self.log_scroll = u16::MAX;

        Ok(())
    }

    async fn load_systemd_logs(&self, vm_name: &str) -> BlixardResult<Vec<String>> {
        use tokio::process::Command;
        
        let service_name = format!("blixard-vm-{}", vm_name);
        
        // Try to get the last 100 lines of logs for this VM
        let output = Command::new("journalctl")
            .args(&["--user", "-u", &service_name, "-n", "100", "--no-pager"])
            .output()
            .await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to run journalctl: {}", e),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(crate::BlixardError::Internal {
                message: format!("journalctl failed: {}", stderr),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<String> = stdout
            .lines()
            .map(|line| line.to_string())
            .collect();

        if lines.is_empty() {
            Ok(vec![
                format!("No logs found for service '{}'", service_name),
                "This could mean:".to_string(),
                "1. The VM was never started".to_string(),
                "2. The systemd service doesn't exist".to_string(),
                "3. Logs have been rotated out".to_string(),
            ])
        } else {
            Ok(lines)
        }
    }

    async fn start_log_following(&mut self, vm_name: &str) -> BlixardResult<()> {
        use tokio::process::Command;
        use tokio::io::{AsyncBufReadExt, BufReader};
        
        let service_name = format!("blixard-vm-{}", vm_name);
        
        // Create a channel for log updates
        let (sender, receiver) = mpsc::unbounded_channel();
        self.log_receiver = Some(receiver);
        
        // Spawn a background task to follow logs
        let service_name_clone = service_name.clone();
        tokio::spawn(async move {
            // Use journalctl --follow for live log streaming
            let mut child = match Command::new("journalctl")
                .args(&[
                    "--user",
                    "-u", &service_name_clone,
                    "-n", "0",  // Don't show initial lines (we already loaded them)
                    "--follow",
                    "--no-pager"
                ])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(child) => child,
                Err(e) => {
                    let _ = sender.send(format!("Failed to start journalctl: {}", e));
                    return;
                }
            };
            
            if let Some(stdout) = child.stdout.take() {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                
                while let Ok(Some(line)) = lines.next_line().await {
                    if sender.send(line).is_err() {
                        // Receiver dropped, stop following
                        break;
                    }
                }
            }
            
            // Clean up the child process
            let _ = child.kill().await;
        });
        
        Ok(())
    }

    async fn start_live_log_following(&mut self, vm_name: Option<String>) -> BlixardResult<()> {
        use tokio::process::Command;
        use tokio::io::{AsyncBufReadExt, BufReader};
        
        // Create a channel for live log updates
        let (sender, receiver) = mpsc::unbounded_channel();
        self.live_log_receiver = Some(receiver);
        self.live_log_vm = vm_name.clone();
        
        // Spawn a background task to follow logs
        tokio::spawn(async move {
            // Use journalctl to follow logs from all blixard-vm services or specific VM
            let child = if let Some(vm_name) = vm_name {
                // Follow specific VM
                let service_name = format!("blixard-vm-{}", vm_name);
                let _ = sender.send(format!("Following logs for VM: {}", vm_name));
                
                Command::new("journalctl")
                    .args(&[
                        "--user",
                        "-u", &service_name,
                        "-n", "10",  // Show last 10 lines initially
                        "--follow",
                        "--no-pager"
                    ])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
            } else {
                // For all VMs, we need to find existing blixard-vm services first
                // Get list of all user systemd services matching blixard-vm-*
                let service_list_output = Command::new("systemctl")
                    .args(&["--user", "list-units", "--type=service", "--all", "--no-legend", "--no-pager"])
                    .output()
                    .await;
                    
                if let Ok(output) = service_list_output {
                    let services_text = String::from_utf8_lossy(&output.stdout);
                    
                    // Find all blixard-vm services
                    let blixard_services: Vec<String> = services_text
                        .lines()
                        .filter_map(|line| {
                            // Parse systemctl output format: service.name   loaded  state  state  description
                            let parts: Vec<&str> = line.split_whitespace().collect();
                            if !parts.is_empty() {
                                // Service name is the first field
                                let service_name = parts[0];
                                if service_name.starts_with("blixard-vm-") && service_name.ends_with(".service") {
                                    // Also check that the line doesn't indicate "not-found" state
                                    if !line.contains("not-found") {
                                        Some(service_name.to_string())
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        })
                        .collect();
                    
                    // If no services found, send a message
                    if blixard_services.is_empty() {
                        let _ = sender.send("No blixard-vm services found".to_string());
                        return;
                    }
                    
                    // Send a debug message about what services we found
                    let _ = sender.send(format!("Following {} services: {}", 
                        blixard_services.len(), 
                        blixard_services.join(", ")
                    ));
                    
                    // Build args dynamically with owned strings
                    let mut cmd = Command::new("journalctl");
                    cmd.args(&["--user", "--follow", "--no-pager", "-n", "10"]);
                    
                    // Add each service with -u flag
                    for service in &blixard_services {
                        cmd.args(&["-u", service]);
                    }
                    
                    cmd.stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::piped())
                        .spawn()
                } else {
                    let _ = sender.send("Failed to list systemd services".to_string());
                    return;
                }
            };
            
            let mut child = match child {
                Ok(child) => child,
                Err(e) => {
                    let _ = sender.send(format!("Failed to start journalctl: {}", e));
                    return;
                }
            };
            
            if let Some(stdout) = child.stdout.take() {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                
                while let Ok(Some(line)) = lines.next_line().await {
                    // Use simple timestamp - journalctl already includes timestamps
                    if sender.send(line).is_err() {
                        // Receiver dropped, stop following
                        break;
                    }
                }
            }
            
            // Clean up the child process
            let _ = child.kill().await;
        });
        
        Ok(())
    }

    fn stop_log_following(&mut self) {
        // Drop the receiver to signal the background task to stop
        self.log_receiver = None;
    }

    fn stop_live_log_following(&mut self) {
        // Drop the live log receiver to signal the background task to stop
        self.live_log_receiver = None;
        self.live_log_vm = None;
    }

    async fn get_vm_process_info(&self, vm_name: &str) -> BlixardResult<VmProcessInfo> {
        use tokio::process::Command;
        
        let service_name = format!("blixard-vm-{}", vm_name);
        
        // Get systemd service status
        let status_output = Command::new("systemctl")
            .args(&["--user", "status", &service_name, "--no-pager", "-l"])
            .output()
            .await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to run systemctl: {}", e),
            })?;

        let status_text = String::from_utf8_lossy(&status_output.stdout);
        
        // Parse the systemctl output for useful information
        let mut process_info = VmProcessInfo {
            service_status: "Unknown".to_string(),
            uptime: None,
            memory_usage: None,
            cpu_usage: None,
            pid: None,
        };

        // Parse status
        if status_text.contains("Active: active (running)") {
            process_info.service_status = "Running".to_string();
        } else if status_text.contains("Active: inactive") {
            process_info.service_status = "Stopped".to_string();
        } else if status_text.contains("Active: failed") {
            process_info.service_status = "Failed".to_string();
        }

        // Extract PID if running
        for line in status_text.lines() {
            if line.contains("Main PID:") {
                if let Some(pid_str) = line.split("Main PID:").nth(1) {
                    if let Some(pid_part) = pid_str.trim().split_whitespace().next() {
                        if let Ok(pid) = pid_part.parse::<u32>() {
                            process_info.pid = Some(pid);
                        }
                    }
                }
            }
        }

        // If we have a PID, get process details
        if let Some(pid) = process_info.pid {
            if let Ok(proc_info) = self.get_process_details(pid).await {
                process_info.memory_usage = proc_info.memory_mb;
                process_info.cpu_usage = proc_info.cpu_percent;
                process_info.uptime = proc_info.uptime;
            }
        }

        Ok(process_info)
    }

    async fn get_process_details(&self, pid: u32) -> BlixardResult<ProcessDetails> {
        use tokio::process::Command;
        
        // Use ps to get process information
        let ps_output = Command::new("ps")
            .args(&["-p", &pid.to_string(), "-o", "pid,etime,rss,%cpu", "--no-headers"])
            .output()
            .await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to run ps: {}", e),
            })?;

        let ps_text = String::from_utf8_lossy(&ps_output.stdout);
        
        if ps_text.trim().is_empty() {
            return Err(crate::BlixardError::Internal {
                message: "Process not found".to_string(),
            });
        }

        let mut details = ProcessDetails {
            uptime: None,
            memory_mb: None,
            cpu_percent: None,
        };

        // Parse ps output: PID ELAPSED RSS %CPU
        if let Some(line) = ps_text.lines().next() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                // ELAPSED (uptime)
                details.uptime = Some(parts[1].to_string());
                
                // RSS (memory in KB)
                if let Ok(rss_kb) = parts[2].parse::<f32>() {
                    details.memory_mb = Some(rss_kb / 1024.0);
                }
                
                // %CPU
                if let Ok(cpu) = parts[3].parse::<f32>() {
                    details.cpu_percent = Some(cpu);
                }
            }
        }

        Ok(details)
    }

    async fn start_ssh_session(&mut self, vm_name: &str) -> BlixardResult<()> {
        let ip_address = self.get_vm_ip_address(vm_name).await?;
        
        let (output_sender, output_receiver) = mpsc::unbounded_channel();
        let (input_sender, input_receiver) = mpsc::unbounded_channel();
        
        // Initialize SSH session
        let session = SshSession {
            vm_name: vm_name.to_string(),
            host: ip_address.clone(),
            port: 22, // Standard SSH port for direct connection
            username: "root".to_string(),
            output_lines: vec![
                format!("🔌 Starting SSH session to VM '{}'", vm_name),
                format!("Host: {}", ip_address),
                format!("User: root"),
                "".to_string(),
                "Connecting...".to_string(),
            ],
            input_buffer: String::new(),
            is_connected: false,
            connection_status: "Connecting...".to_string(),
            output_receiver: Some(output_receiver),
            input_sender: Some(input_sender),
        };
        
        // Start SSH process in background
        self.spawn_ssh_process(vm_name, &ip_address, output_sender, input_receiver).await?;
        
        self.ssh_session = Some(session);
        Ok(())
    }

    async fn get_vm_ip_address(&self, vm_name: &str) -> BlixardResult<String> {
        // Find the VM in our current list and get its IP address
        if let Some(vm) = self.vms.iter().find(|v| v.name == vm_name) {
            if let Some(ip) = &vm.ip_address {
                return Ok(ip.clone());
            }
        }
        
        Err(crate::BlixardError::Internal {
            message: format!("Could not determine IP address for VM '{}'. Make sure VM list is refreshed.", vm_name),
        })
    }

    fn get_ssh_key_path() -> String {
        // Try to find available SSH keys in order of preference
        let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/home/brittonr".to_string());
        
        let key_candidates = vec![
            format!("{}/.ssh/id_ed25519", home_dir),
            format!("{}/.ssh/id_rsa", home_dir),
            format!("{}/.ssh/id_ecdsa", home_dir),
        ];
        
        for key_path in &key_candidates {
            if std::path::Path::new(key_path).exists() {
                return key_path.clone();
            }
        }
        
        // Default to ed25519 key even if it doesn't exist (user will get an error)
        key_candidates[0].clone()
    }

    #[cfg(not(madsim))]
    async fn spawn_ssh_process(
        &self, 
        vm_name: &str, 
        ip_address: &str, 
        output_sender: mpsc::UnboundedSender<String>,
        mut input_receiver: mpsc::UnboundedReceiver<String>
    ) -> BlixardResult<()> {
        use tokio::process::Command;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        
        let vm_name = vm_name.to_string();
        let ip_address = ip_address.to_string();
        
        tokio::spawn(async move {
            // Send initial connection status
            let _ = output_sender.send("Attempting to connect via SSH...".to_string());
            
            // Try to establish SSH connection using key-based authentication
            let mut child = match Command::new("ssh")
                .args(&[
                    "-o", "StrictHostKeyChecking=no",
                    "-o", "UserKnownHostsFile=/dev/null", 
                    "-o", "ConnectTimeout=5",
                    "-o", "BatchMode=yes",                    // Don't prompt for passwords interactively
                    "-o", "PasswordAuthentication=no",        // Disable password auth  
                    "-o", "PubkeyAuthentication=yes",         // Enable key auth
                    "-o", "PreferredAuthentications=publickey", // Prefer key auth
                    "-i", &Self::get_ssh_key_path(),   // Use user's SSH key
                    &format!("root@{}", ip_address)   // Connect directly to VM IP
                ])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(child) => {
                    let _ = output_sender.send(format!("✓ Connected to VM '{}'", vm_name));
                    let _ = output_sender.send("".to_string());
                    child
                }
                Err(e) => {
                    let _ = output_sender.send(format!("✗ Failed to connect: {}", e));
                    let _ = output_sender.send("".to_string());
                    let _ = output_sender.send("Tips:".to_string());
                    let _ = output_sender.send("1. Make sure the VM is running".to_string());
                    let _ = output_sender.send("2. Check if SSH service is enabled in the VM".to_string());
                    let _ = output_sender.send("3. Verify the port is correct".to_string());
                    return;
                }
            };
            
            // Handle SSH process I/O
            let mut stdin = child.stdin.take().unwrap();
            let stdout = child.stdout.take().unwrap();
            let stderr = child.stderr.take().unwrap();
            
            // Spawn task to handle stdout
            let output_sender_stdout = output_sender.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if output_sender_stdout.send(line).is_err() {
                        break;
                    }
                }
            });
            
            // Spawn task to handle stderr
            let output_sender_stderr = output_sender.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if output_sender_stderr.send(format!("stderr: {}", line)).is_err() {
                        break;
                    }
                }
            });
            
            // Handle input from TUI
            while let Some(input) = input_receiver.recv().await {
                if stdin.write_all(input.as_bytes()).await.is_err() {
                    break;
                }
                if stdin.flush().await.is_err() {
                    break;
                }
            }
            
            // Clean up
            let _ = child.kill().await;
        });
        
        Ok(())
    }

    #[cfg(madsim)]
    async fn spawn_ssh_process(
        &self, 
        vm_name: &str, 
        port: u16, 
        output_sender: mpsc::UnboundedSender<String>,
        _input_receiver: mpsc::UnboundedReceiver<String>
    ) -> BlixardResult<()> {
        // In simulation mode, just simulate a connection
        let vm_name = vm_name.to_string();
        tokio::spawn(async move {
            let _ = output_sender.send(format!("✓ Simulated SSH connection to VM '{}'", vm_name));
            let _ = output_sender.send("".to_string());
            let _ = output_sender.send("Simulation mode - no actual SSH connection".to_string());
            let _ = output_sender.send("Commands entered will be echoed back".to_string());
        });
        Ok(())
    }
    
    fn close_ssh_session(&mut self) {
        self.ssh_session = None;
        self.ssh_info = None;
    }

    /// Start server with default configuration
    async fn start_server_with_defaults(&mut self) -> BlixardResult<()> {
        self.server_startup.status = ServerStartupStatus::Starting;
        self.server_startup.message = "Starting blixard server with default settings...".to_string();

        // Use default config values
        let node_id = 1u64;
        let bind_addr = "127.0.0.1:7001".to_string();
        let data_dir = "./data".to_string();
        let vm_backend = "microvm".to_string();

        self.start_server_internal(node_id, bind_addr, data_dir, vm_backend).await
    }

    /// Start server with user-configured settings
    async fn start_server_with_config(&mut self) -> BlixardResult<()> {
        self.server_startup.status = ServerStartupStatus::Starting;
        self.server_startup.message = "Starting blixard server with custom configuration...".to_string();

        // Parse node_id from string
        let node_id = match self.server_startup.config.node_id.parse::<u64>() {
            Ok(id) if id > 0 => id,
            _ => {
                self.server_startup.status = ServerStartupStatus::Failed;
                self.server_startup.message = "Invalid node ID: must be a positive number".to_string();
                return Ok(());
            }
        };

        // Validate bind address
        if self.server_startup.config.bind_address.parse::<std::net::SocketAddr>().is_err() {
            self.server_startup.status = ServerStartupStatus::Failed;
            self.server_startup.message = "Invalid bind address format".to_string();
            return Ok(());
        }

        self.start_server_internal(
            node_id,
            self.server_startup.config.bind_address.clone(),
            self.server_startup.config.data_dir.clone(),
            self.server_startup.config.vm_backend.clone(),
        ).await
    }

    /// Internal server startup logic shared by both default and configured startup
    async fn start_server_internal(&mut self, node_id: u64, bind_addr: String, data_dir: String, vm_backend: String) -> BlixardResult<()> {
        use std::time::Duration;
        use std::net::TcpStream;

        // Start the server in a background task (same logic as the command-line version)
        let bind_addr_clone = bind_addr.clone();
        let data_dir_clone = data_dir.clone();
        let vm_backend_clone = vm_backend.clone();
        
        tokio::spawn(async move {
            use tokio::process::Command;
            use std::process::Stdio;
            
            // Get the current executable path
            let current_exe = std::env::current_exe().unwrap_or_else(|_| {
                // Fallback to cargo run if we can't get the exe path
                std::path::PathBuf::from("cargo")
            });
            
            // Spawn the node as a separate process with output redirected
            let mut cmd = if current_exe.file_name().unwrap_or_default() == "cargo" {
                let mut cmd = Command::new("cargo");
                cmd.args(&["run", "--", "node"]);
                cmd
            } else {
                let mut cmd = Command::new(&current_exe);
                cmd.arg("node");
                cmd
            };
            
            // Add node arguments
            cmd.args(&[
                "--id", &node_id.to_string(),
                "--bind", &bind_addr_clone,
                "--data-dir", &data_dir_clone,
                "--vm-backend", &vm_backend_clone
            ]);
            
            // Redirect all output to null to prevent interference with TUI
            cmd.stdout(Stdio::null())
               .stderr(Stdio::null())
               .stdin(Stdio::null());
            
            // Set environment to suppress most logging
            cmd.env("RUST_LOG", "error");
            
            // Start the server process and don't wait for it
            let _ = cmd.spawn();
        });
        
        // Wait for server to start with timeout
        self.server_startup.message = "Waiting for server to start...".to_string();
        let server_addr = bind_addr.parse::<std::net::SocketAddr>()
            .map_err(|e| crate::BlixardError::ConfigError(format!("Invalid bind address: {}", e)))?;
        
        let mut attempts = 0;
        let max_attempts = 30; // 15 seconds timeout
        
        while attempts < max_attempts {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if TcpStream::connect_timeout(&server_addr, Duration::from_millis(500)).is_ok() {
                // Server is up! Try to connect with TUI client
                match super::VmClient::new(&bind_addr).await {
                    Ok(client) => {
                        self.vm_client = Some(client);
                        self.server_startup.status = ServerStartupStatus::Success;
                        self.server_startup.message = "Server started successfully!".to_string();
                        return Ok(());
                    }
                    Err(e) => {
                        self.server_startup.status = ServerStartupStatus::Failed;
                        self.server_startup.message = format!("Server started but failed to connect TUI client: {}", e);
                        return Ok(());
                    }
                }
            }
            attempts += 1;
        }
        
        // Timeout
        self.server_startup.status = ServerStartupStatus::Failed;
        self.server_startup.message = "Timeout waiting for server to start".to_string();
        Ok(())
    }
}

#[derive(Debug)]
pub struct VmProcessInfo {
    pub service_status: String,
    pub uptime: Option<String>,
    pub memory_usage: Option<f32>,
    pub cpu_usage: Option<f32>,
    pub pid: Option<u32>,
}

#[derive(Debug)]
pub struct ProcessDetails {
    pub uptime: Option<String>,
    pub memory_mb: Option<f32>,
    pub cpu_percent: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct ClusterInfo {
    pub leader_id: u64,
    pub term: u64,
    pub node_count: usize,
    pub current_node_id: u64,
    pub current_node_state: String,
    pub nodes: Vec<NodeInfo>,
}

#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub id: u64,
    pub address: String,
    pub state: String,
    pub is_current: bool,
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

#[derive(Debug, Clone)]
pub struct SshInfo {
    pub vm_name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub command: String,
}

#[derive(Debug)]
pub struct SshSession {
    pub vm_name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub output_lines: Vec<String>,
    pub input_buffer: String,
    pub is_connected: bool,
    pub connection_status: String,
    pub output_receiver: Option<mpsc::UnboundedReceiver<String>>,
    pub input_sender: Option<mpsc::UnboundedSender<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServerStartupStatus {
    Prompt,      // Asking user if they want to start server
    Starting,    // Server is starting
    Success,     // Server started successfully
    Failed,      // Server failed to start
}

#[derive(Debug, Clone)]
pub struct ServerStartupState {
    pub status: ServerStartupStatus,
    pub message: String,
    pub selected_option: usize, // 0 = Yes, 1 = No, 2 = Configure
    pub show_config: bool,
    pub config: ServerConfig,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub node_id: String,
    pub bind_address: String,
    pub data_dir: String,
    pub vm_backend: String,
    pub current_field: usize,
}

impl Default for ServerStartupState {
    fn default() -> Self {
        Self {
            status: ServerStartupStatus::Prompt,
            message: "No blixard server detected on 127.0.0.1:7001".to_string(),
            selected_option: 0,
            show_config: false,
            config: ServerConfig::default(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            node_id: "1".to_string(),
            bind_address: "127.0.0.1:7001".to_string(),
            data_dir: "./data".to_string(),
            vm_backend: "microvm".to_string(),
            current_field: 0,
        }
    }
}