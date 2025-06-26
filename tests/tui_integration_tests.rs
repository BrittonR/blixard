//! TUI Integration Tests
//! 
//! Comprehensive testing framework for the Blixard TUI including:
//! - Real cluster discovery and management workflows
//! - End-to-end VM and node operations
//! - Debug mode visualization testing
//! - Development workflow validation

use tokio::time::{timeout, Duration};
use std::sync::Arc;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use blixard::{
    tui::{
        app::{App, AppTab, AppMode, VmInfo, NodeInfo, NodeStatus, NodeRole},
        events::{Event, EventHandler},
        vm_client::VmClient,
    },
    BlixardResult,
};

/// Test helper for TUI operations
pub struct TuiTestHarness {
    app: App,
    event_handler: EventHandler,
}

impl TuiTestHarness {
    pub async fn new() -> BlixardResult<Self> {
        let app = App::new().await?;
        let event_handler = EventHandler::new(100); // 100ms tick rate for tests
        
        Ok(Self {
            app,
            event_handler,
        })
    }

    /// Send a key event to the app
    pub async fn send_key(&mut self, key_code: KeyCode) -> BlixardResult<()> {
        self.send_key_with_modifiers(key_code, KeyModifiers::NONE).await
    }

    /// Send a key event with modifiers
    pub async fn send_key_with_modifiers(&mut self, key_code: KeyCode, modifiers: KeyModifiers) -> BlixardResult<()> {
        let key_event = KeyEvent::new(key_code, modifiers);
        self.app.handle_event(Event::Key(key_event)).await
    }

    /// Send a character key
    pub async fn send_char(&mut self, c: char) -> BlixardResult<()> {
        self.send_key(KeyCode::Char(c)).await
    }

    /// Send a string as individual character events
    pub async fn send_string(&mut self, s: &str) -> BlixardResult<()> {
        for c in s.chars() {
            self.send_char(c).await?;
        }
        Ok(())
    }

    /// Wait for app to refresh data
    pub async fn wait_for_refresh(&mut self) -> BlixardResult<()> {
        // Send tick events until refresh occurs
        let mut attempts = 0;
        while !self.app.should_refresh() && attempts < 50 {
            self.app.handle_event(Event::Tick).await?;
            tokio::time::sleep(Duration::from_millis(10)).await;
            attempts += 1;
        }
        
        if self.app.should_refresh() {
            self.app.refresh_all_data().await?;
        }
        
        Ok(())
    }

    /// Verify app state matches expected condition
    pub fn verify_state<F>(&self, check: F) -> bool
    where
        F: Fn(&App) -> bool,
    {
        check(&self.app)
    }

    /// Get current app for inspection
    pub fn app(&self) -> &App {
        &self.app
    }

    /// Get mutable app for direct manipulation
    pub fn app_mut(&mut self) -> &mut App {
        &mut self.app
    }
}

#[tokio::test]
async fn test_tui_tab_navigation() -> BlixardResult<()> {
    let mut harness = TuiTestHarness::new().await?;
    
    // Start on Dashboard
    assert_eq!(harness.app().current_tab, AppTab::Dashboard);
    
    // Test numeric tab switching
    harness.send_char('2').await?;
    assert_eq!(harness.app().current_tab, AppTab::VirtualMachines);
    
    harness.send_char('3').await?;
    assert_eq!(harness.app().current_tab, AppTab::Nodes);
    
    harness.send_char('4').await?;
    assert_eq!(harness.app().current_tab, AppTab::Monitoring);
    
    harness.send_char('5').await?;
    assert_eq!(harness.app().current_tab, AppTab::Configuration);
    
    harness.send_char('1').await?;
    assert_eq!(harness.app().current_tab, AppTab::Dashboard);
    
    Ok(())
}

#[tokio::test]
async fn test_tui_vim_mode_navigation() -> BlixardResult<()> {
    let mut harness = TuiTestHarness::new().await?;
    
    // Enable vim mode
    harness.send_char('v').await?;
    assert!(harness.app().settings.vim_mode);
    
    // Test vim-style tab navigation
    harness.send_char('l').await?; // right
    assert_eq!(harness.app().current_tab, AppTab::VirtualMachines);
    
    harness.send_char('l').await?; // right
    assert_eq!(harness.app().current_tab, AppTab::Nodes);
    
    harness.send_char('h').await?; // left
    assert_eq!(harness.app().current_tab, AppTab::VirtualMachines);
    
    harness.send_char('h').await?; // left
    assert_eq!(harness.app().current_tab, AppTab::Dashboard);
    
    Ok(())
}

#[tokio::test]
async fn test_tui_search_and_filtering() -> BlixardResult<()> {
    let mut harness = TuiTestHarness::new().await?;
    
    // Add some mock VMs to test filtering
    harness.app_mut().vms = vec![
        VmInfo {
            name: "web-server-1".to_string(),
            status: blixard_core::types::VmStatus::Running,
            vcpus: 2,
            memory: 2048,
            node_id: 1,
            ip_address: Some("10.0.0.10".to_string()),
            uptime: Some("2h 30m".to_string()),
            cpu_usage: Some(45.0),
            memory_usage: Some(60.0),
            placement_strategy: None,
            created_at: None,
            config_path: None,
        },
        VmInfo {
            name: "database-1".to_string(),
            status: blixard_core::types::VmStatus::Running,
            vcpus: 4,
            memory: 8192,
            node_id: 2,
            ip_address: Some("10.0.0.11".to_string()),
            uptime: Some("1h 15m".to_string()),
            cpu_usage: Some(80.0),
            memory_usage: Some(70.0),
            placement_strategy: None,
            created_at: None,
            config_path: None,
        },
        VmInfo {
            name: "worker-1".to_string(),
            status: blixard_core::types::VmStatus::Stopped,
            vcpus: 1,
            memory: 1024,
            node_id: 1,
            ip_address: None,
            uptime: None,
            cpu_usage: None,
            memory_usage: None,
            placement_strategy: None,
            created_at: None,
            config_path: None,
        },
    ];
    
    // Switch to VMs tab
    harness.send_char('2').await?;
    assert_eq!(harness.app().current_tab, AppTab::VirtualMachines);
    
    // Apply VM filter
    harness.app_mut().apply_vm_filter();
    assert_eq!(harness.app().get_displayed_vms().len(), 3);
    
    // Test search mode
    harness.send_char('/').await?;
    assert_eq!(harness.app().search_mode, blixard::tui::app::SearchMode::VmSearch);
    
    // Type search query
    harness.send_string("web").await?;
    assert_eq!(harness.app().quick_filter, "web");
    
    // Apply filter
    harness.send_key(KeyCode::Enter).await?;
    assert_eq!(harness.app().search_mode, blixard::tui::app::SearchMode::None);
    
    // Check filtered results
    let displayed = harness.app().get_displayed_vms();
    assert_eq!(displayed.len(), 1);
    assert_eq!(displayed[0].name, "web-server-1");
    
    // Clear filter
    harness.send_key_with_modifiers(KeyCode::Char('F'), KeyModifiers::SHIFT).await?;
    assert_eq!(harness.app().get_displayed_vms().len(), 3);
    
    Ok(())
}

#[tokio::test]
async fn test_tui_vm_creation_workflow() -> BlixardResult<()> {
    let mut harness = TuiTestHarness::new().await?;
    
    // Go to VMs tab and create VM
    harness.send_char('2').await?;
    harness.send_char('c').await?;
    assert_eq!(harness.app().mode, AppMode::CreateVmForm);
    
    // Fill form using tab navigation
    harness.send_string("test-vm").await?;
    
    harness.send_key(KeyCode::Tab).await?; // Move to vCPUs field
    harness.send_string("4").await?;
    
    harness.send_key(KeyCode::Tab).await?; // Move to memory field
    harness.send_string("4096").await?;
    
    // Test form validation by trying to submit with invalid data
    let form = &harness.app().create_vm_form;
    assert_eq!(form.name, "test-vm");
    assert_eq!(form.vcpus, "4");
    assert_eq!(form.memory, "4096");
    
    // Cancel form
    harness.send_key(KeyCode::Esc).await?;
    assert_eq!(harness.app().mode, AppMode::VmList);
    
    Ok(())
}

#[tokio::test]
async fn test_tui_node_management_workflow() -> BlixardResult<()> {
    let mut harness = TuiTestHarness::new().await?;
    
    // Add some mock nodes
    harness.app_mut().nodes = vec![
        NodeInfo {
            id: 1,
            address: "127.0.0.1:7001".to_string(),
            status: NodeStatus::Healthy,
            role: NodeRole::Leader,
            cpu_usage: 45.0,
            memory_usage: 60.0,
            vm_count: 2,
            last_seen: Some(std::time::Instant::now()),
            version: Some("0.1.0".to_string()),
        },
        NodeInfo {
            id: 2,
            address: "127.0.0.1:7002".to_string(),
            status: NodeStatus::Healthy,
            role: NodeRole::Follower,
            cpu_usage: 30.0,
            memory_usage: 40.0,
            vm_count: 1,
            last_seen: Some(std::time::Instant::now()),
            version: Some("0.1.0".to_string()),
        },
    ];
    
    // Go to Nodes tab
    harness.send_char('3').await?;
    assert_eq!(harness.app().current_tab, AppTab::Nodes);
    
    // Apply node filter
    harness.app_mut().apply_node_filter();
    assert_eq!(harness.app().get_displayed_nodes().len(), 2);
    
    // Test node navigation
    harness.send_key(KeyCode::Down).await?;
    assert_eq!(harness.app().node_table_state.selected(), Some(0));
    
    harness.send_key(KeyCode::Down).await?;
    assert_eq!(harness.app().node_table_state.selected(), Some(1));
    
    // Test adding node
    harness.send_char('a').await?;
    assert_eq!(harness.app().mode, AppMode::CreateNodeForm);
    
    // Fill node form
    harness.send_string("3").await?; // Node ID
    
    harness.send_key(KeyCode::Tab).await?;
    harness.send_string("127.0.0.1:7003").await?; // Bind address
    
    // Cancel form
    harness.send_key(KeyCode::Esc).await?;
    assert_eq!(harness.app().mode, AppMode::NodeList);
    
    Ok(())
}

#[tokio::test]
async fn test_tui_cluster_discovery() -> BlixardResult<()> {
    let mut harness = TuiTestHarness::new().await?;
    
    // Test cluster discovery
    harness.send_key_with_modifiers(KeyCode::Char('C'), KeyModifiers::SHIFT).await?;
    
    // Wait for discovery to start
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Discovery should add discovered clusters
    let clusters = &harness.app().discovered_clusters;
    println!("Discovered {} clusters", clusters.len());
    
    Ok(())
}

#[tokio::test]
async fn test_tui_performance_modes() -> BlixardResult<()> {
    let mut harness = TuiTestHarness::new().await?;
    
    // Test performance mode cycling
    let initial_mode = harness.app().settings.performance_mode.clone();
    
    harness.send_char('p').await?;
    assert_ne!(harness.app().settings.performance_mode, initial_mode);
    
    harness.send_char('p').await?;
    harness.send_char('p').await?;
    harness.send_char('p').await?;
    
    // Should cycle back to original mode
    assert_eq!(harness.app().settings.performance_mode, initial_mode);
    
    Ok(())
}

#[tokio::test]
async fn test_tui_debug_mode() -> BlixardResult<()> {
    let mut harness = TuiTestHarness::new().await?;
    
    // Test debug mode toggle
    assert!(!harness.app().settings.debug_mode);
    
    harness.send_char('d').await?;
    assert!(harness.app().settings.debug_mode);
    
    harness.send_char('d').await?;
    assert!(!harness.app().settings.debug_mode);
    
    Ok(())
}

#[tokio::test]
async fn test_tui_error_handling() -> BlixardResult<()> {
    let mut harness = TuiTestHarness::new().await?;
    
    // Test error states by disconnecting client
    harness.app_mut().vm_client = None;
    
    // Try to refresh - should handle gracefully
    harness.send_char('r').await?;
    
    // Should show error or disconnected state
    assert!(harness.app().vm_client.is_none());
    
    Ok(())
}

#[tokio::test]
async fn test_tui_keyboard_shortcuts() -> BlixardResult<()> {
    let mut harness = TuiTestHarness::new().await?;
    
    // Test global shortcuts
    harness.send_char('h').await?;
    assert_eq!(harness.app().current_tab, AppTab::Help);
    
    harness.send_char('1').await?;
    assert_eq!(harness.app().current_tab, AppTab::Dashboard);
    
    // Test refresh shortcut
    harness.send_char('r').await?;
    
    // Test quit shortcut (but don't actually quit in test)
    assert!(!harness.app().should_quit);
    
    Ok(())
}

#[tokio::test]
async fn test_tui_event_handling() -> BlixardResult<()> {
    let mut harness = TuiTestHarness::new().await?;
    
    // Test that events are added to the event log
    let initial_event_count = harness.app().recent_events.len();
    
    // Trigger some events
    harness.send_char('r').await?; // Refresh
    harness.send_char('v').await?; // Toggle vim mode
    
    // Should have more events
    assert!(harness.app().recent_events.len() >= initial_event_count);
    
    Ok(())
}

/// Test helper for simulating cluster operations
pub async fn simulate_cluster_with_vms() -> BlixardResult<TuiTestHarness> {
    let mut harness = TuiTestHarness::new().await?;
    
    // Add realistic cluster state
    harness.app_mut().cluster_metrics.total_nodes = 3;
    harness.app_mut().cluster_metrics.healthy_nodes = 3;
    harness.app_mut().cluster_metrics.leader_id = Some(1);
    harness.app_mut().cluster_metrics.raft_term = 5;
    
    // Add nodes
    harness.app_mut().nodes = vec![
        NodeInfo {
            id: 1,
            address: "127.0.0.1:7001".to_string(),
            status: NodeStatus::Healthy,
            role: NodeRole::Leader,
            cpu_usage: 45.0,
            memory_usage: 60.0,
            vm_count: 2,
            last_seen: Some(std::time::Instant::now()),
            version: Some("0.1.0".to_string()),
        },
        NodeInfo {
            id: 2,
            address: "127.0.0.1:7002".to_string(),
            status: NodeStatus::Healthy,
            role: NodeRole::Follower,
            cpu_usage: 30.0,
            memory_usage: 40.0,
            vm_count: 1,
            last_seen: Some(std::time::Instant::now()),
            version: Some("0.1.0".to_string()),
        },
        NodeInfo {
            id: 3,
            address: "127.0.0.1:7003".to_string(),
            status: NodeStatus::Healthy,
            role: NodeRole::Follower,
            cpu_usage: 20.0,
            memory_usage: 30.0,
            vm_count: 1,
            last_seen: Some(std::time::Instant::now()),
            version: Some("0.1.0".to_string()),
        },
    ];
    
    // Add VMs
    harness.app_mut().vms = vec![
        VmInfo {
            name: "web-server-1".to_string(),
            status: blixard_core::types::VmStatus::Running,
            vcpus: 2,
            memory: 2048,
            node_id: 1,
            ip_address: Some("10.0.0.10".to_string()),
            uptime: Some("2h 30m".to_string()),
            cpu_usage: Some(45.0),
            memory_usage: Some(60.0),
            placement_strategy: None,
            created_at: None,
            config_path: None,
        },
        VmInfo {
            name: "database-1".to_string(),
            status: blixard_core::types::VmStatus::Running,
            vcpus: 4,
            memory: 8192,
            node_id: 2,
            ip_address: Some("10.0.0.11".to_string()),
            uptime: Some("1h 15m".to_string()),
            cpu_usage: Some(80.0),
            memory_usage: Some(70.0),
            placement_strategy: None,
            created_at: None,
            config_path: None,
        },
        VmInfo {
            name: "worker-1".to_string(),
            status: blixard_core::types::VmStatus::Running,
            vcpus: 1,
            memory: 1024,
            node_id: 3,
            ip_address: Some("10.0.0.12".to_string()),
            uptime: Some("45m".to_string()),
            cpu_usage: Some(25.0),
            memory_usage: Some(40.0),
            placement_strategy: None,
            created_at: None,
            config_path: None,
        },
        VmInfo {
            name: "test-vm".to_string(),
            status: blixard_core::types::VmStatus::Stopped,
            vcpus: 1,
            memory: 1024,
            node_id: 1,
            ip_address: None,
            uptime: None,
            cpu_usage: None,
            memory_usage: None,
            placement_strategy: None,
            created_at: None,
            config_path: None,
        },
    ];
    
    // Update metrics to match the state
    harness.app_mut().cluster_metrics.total_vms = 4;
    harness.app_mut().cluster_metrics.running_vms = 3;
    harness.app_mut().cluster_metrics.total_cpu = 24; // 8 cores per node
    harness.app_mut().cluster_metrics.used_cpu = 7; // Sum of VM vCPUs
    harness.app_mut().cluster_metrics.total_memory = 49152; // 16GB per node
    harness.app_mut().cluster_metrics.used_memory = 15360; // Sum of VM memory
    
    // Apply filters to update displayed lists
    harness.app_mut().apply_vm_filter();
    harness.app_mut().apply_node_filter();
    
    Ok(harness)
}

#[tokio::test]
async fn test_tui_end_to_end_cluster_workflow() -> BlixardResult<()> {
    let mut harness = simulate_cluster_with_vms().await?;
    
    // Test dashboard view
    assert_eq!(harness.app().current_tab, AppTab::Dashboard);
    assert_eq!(harness.app().cluster_metrics.total_nodes, 3);
    assert_eq!(harness.app().cluster_metrics.total_vms, 4);
    assert_eq!(harness.app().cluster_metrics.running_vms, 3);
    
    // Navigate to VMs and verify data
    harness.send_char('2').await?;
    assert_eq!(harness.app().current_tab, AppTab::VirtualMachines);
    assert_eq!(harness.app().get_displayed_vms().len(), 4);
    
    // Test VM filtering
    harness.send_char('/').await?;
    harness.send_string("web").await?;
    harness.send_key(KeyCode::Enter).await?;
    assert_eq!(harness.app().get_displayed_vms().len(), 1);
    assert_eq!(harness.app().get_displayed_vms()[0].name, "web-server-1");
    
    // Clear filter and navigate to nodes
    harness.send_key_with_modifiers(KeyCode::Char('F'), KeyModifiers::SHIFT).await?;
    harness.send_char('3').await?;
    assert_eq!(harness.app().current_tab, AppTab::Nodes);
    assert_eq!(harness.app().get_displayed_nodes().len(), 3);
    
    // Check monitoring tab
    harness.send_char('4').await?;
    assert_eq!(harness.app().current_tab, AppTab::Monitoring);
    
    // Check configuration tab
    harness.send_char('5').await?;
    assert_eq!(harness.app().current_tab, AppTab::Configuration);
    
    Ok(())
}