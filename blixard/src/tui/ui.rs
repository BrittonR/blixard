//! Modern TUI User Interface
//! 
//! Comprehensive UI rendering for Blixard cluster management with:
//! - Tabbed interface with dashboard-first design
//! - Real-time metrics and resource visualization
//! - Enhanced VM and node management interfaces
//! - Monitoring graphs and observability features
//! - Modern styling and responsive layout

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Paragraph, Wrap, Table, Row, Cell, Gauge, List, ListItem,
        Clear, Tabs, Sparkline,
    },
    Frame,
};

use super::app::{
    App, AppTab, AppMode, NodeRole, 
    CreateVmField, CreateNodeField, RaftDebugInfo, 
    RaftNodeState, DebugLevel
};
use blixard_core::types::VmStatus;

// Color scheme for modern UI
const PRIMARY_COLOR: Color = Color::Cyan;
const SECONDARY_COLOR: Color = Color::Blue;
const SUCCESS_COLOR: Color = Color::Green;
const WARNING_COLOR: Color = Color::Yellow;
const ERROR_COLOR: Color = Color::Red;
const INFO_COLOR: Color = Color::Magenta;
const BACKGROUND_COLOR: Color = Color::Black;
const TEXT_COLOR: Color = Color::White;

pub fn render(f: &mut Frame, app: &App) {
    let size = f.size();
    
    // Main layout with tabs
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Tab bar
            Constraint::Min(0),     // Main content
            Constraint::Length(3),  // Status bar
        ])
        .split(size);

    // Render tab bar
    render_tab_bar(f, chunks[0], app);
    
    // Render main content based on current tab
    match app.current_tab {
        AppTab::Dashboard => render_dashboard(f, chunks[1], app),
        AppTab::VirtualMachines => render_vm_management(f, chunks[1], app),
        AppTab::Nodes => render_node_management(f, chunks[1], app),
        AppTab::Monitoring => render_monitoring(f, chunks[1], app),
        AppTab::Configuration => render_configuration(f, chunks[1], app),
        AppTab::Debug => render_debug(f, chunks[1], app),
        AppTab::Help => render_help(f, chunks[1], app),
    }
    
    // Render status bar
    render_status_bar(f, chunks[2], app);
    
    // Render overlays/popups
    match app.mode {
        AppMode::CreateVmForm => render_create_vm_form(f, app),
        AppMode::CreateNodeForm => render_create_node_form(f, app),
        AppMode::CreateClusterForm => render_create_cluster_form(f, app),
        AppMode::ClusterDiscovery => render_cluster_discovery(f, app),
        AppMode::ConfirmDialog => render_confirm_dialog(f, app),
        AppMode::LogViewer => render_log_viewer(f, app),
        _ => {}
    }
    
    // Render search overlay if in search mode
    if app.search_mode != super::app::SearchMode::None {
        render_search_overlay(f, app);
    }
}

fn render_tab_bar(f: &mut Frame, area: Rect, app: &App) {
    let titles = vec![
        "📊 Dashboard",
        "🖥️ VMs", 
        "🔗 Nodes",
        "📈 Monitoring",
        "⚙️ Config",
        "🐛 Debug",
        "❓ Help"
    ];
    
    let selected_tab = match app.current_tab {
        AppTab::Dashboard => 0,
        AppTab::VirtualMachines => 1,
        AppTab::Nodes => 2,
        AppTab::Monitoring => 3,
        AppTab::Configuration => 4,
        AppTab::Debug => 5,
        AppTab::Help => 6,
    };
    
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title("Blixard Cluster Manager"))
        .style(Style::default().fg(TEXT_COLOR))
        .highlight_style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD))
        .select(selected_tab);
    
    f.render_widget(tabs, area);
}

fn render_dashboard(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),  // Top metrics row
            Constraint::Min(8),     // Content area
        ])
        .split(area);
    
    // Top metrics row
    let metrics_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25), // Cluster status
            Constraint::Percentage(25), // Resource usage
            Constraint::Percentage(25), // VM status
            Constraint::Percentage(25), // Quick actions
        ])
        .split(chunks[0]);
    
    render_cluster_status_card(f, metrics_chunks[0], app);
    render_resource_usage_card(f, metrics_chunks[1], app);
    render_vm_status_card(f, metrics_chunks[2], app);
    render_quick_actions_card(f, metrics_chunks[3], app);
    
    // Bottom content area
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(70), // Recent events
            Constraint::Percentage(30), // System overview
        ])
        .split(chunks[1]);
    
    render_recent_events(f, content_chunks[0], app);
    render_system_overview(f, content_chunks[1], app);
}

fn render_cluster_status_card(f: &mut Frame, area: Rect, app: &App) {
    let metrics = &app.cluster_metrics;
    
    let status_text = if metrics.healthy_nodes == metrics.total_nodes && metrics.total_nodes > 0 {
        "🟢 Healthy"
    } else if metrics.healthy_nodes > 0 {
        "🟡 Degraded"
    } else {
        "🔴 Critical"
    };
    
    let leader_text = match metrics.leader_id {
        Some(id) => format!("Leader: Node {}", id),
        None => "Leader: None".to_string(),
    };
    
    let content = vec![
        Line::from(vec![
            Span::styled("Status: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(status_text, Style::default().fg(SUCCESS_COLOR).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("Nodes: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(format!("{}/{}", metrics.healthy_nodes, metrics.total_nodes), 
                        Style::default().fg(PRIMARY_COLOR)),
        ]),
        Line::from(vec![
            Span::styled(leader_text, Style::default().fg(INFO_COLOR)),
        ]),
        Line::from(vec![
            Span::styled(format!("Term: {}", metrics.raft_term), Style::default().fg(TEXT_COLOR)),
        ]),
    ];
    
    let paragraph = Paragraph::new(content)
        .block(Block::default()
            .borders(Borders::ALL)
            .title("🔗 Cluster Status")
            .border_style(Style::default().fg(PRIMARY_COLOR)))
        .wrap(Wrap { trim: true });
    
    f.render_widget(paragraph, area);
}

fn render_resource_usage_card(f: &mut Frame, area: Rect, app: &App) {
    let metrics = &app.cluster_metrics;
    
    let cpu_usage = if metrics.total_cpu > 0 {
        (metrics.used_cpu as f64 / metrics.total_cpu as f64) * 100.0
    } else {
        0.0
    };
    
    let memory_usage = if metrics.total_memory > 0 {
        (metrics.used_memory as f64 / metrics.total_memory as f64) * 100.0
    } else {
        0.0
    };
    
    let inner_area = area.inner(&Margin { vertical: 1, horizontal: 1 });
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner_area);
    
    // CPU gauge
    let cpu_gauge = Gauge::default()
        .block(Block::default())
        .gauge_style(Style::default().fg(if cpu_usage > 80.0 { ERROR_COLOR } else if cpu_usage > 60.0 { WARNING_COLOR } else { SUCCESS_COLOR }))
        .percent(cpu_usage as u16)
        .label(format!("CPU: {:.1}%", cpu_usage));
    
    // Memory gauge  
    let memory_gauge = Gauge::default()
        .block(Block::default())
        .gauge_style(Style::default().fg(if memory_usage > 80.0 { ERROR_COLOR } else if memory_usage > 60.0 { WARNING_COLOR } else { SUCCESS_COLOR }))
        .percent(memory_usage as u16)
        .label(format!("Memory: {:.1}%", memory_usage));
    
    let block = Block::default()
        .borders(Borders::ALL)
        .title("📊 Resource Usage")
        .border_style(Style::default().fg(PRIMARY_COLOR));
    
    f.render_widget(block, area);
    
    if chunks.len() >= 2 {
        f.render_widget(cpu_gauge, chunks[0]);
        f.render_widget(memory_gauge, chunks[1]);
    }
}

fn render_vm_status_card(f: &mut Frame, area: Rect, app: &App) {
    let metrics = &app.cluster_metrics;
    
    let running_ratio = if metrics.total_vms > 0 {
        (metrics.running_vms as f64 / metrics.total_vms as f64) * 100.0
    } else {
        0.0
    };
    
    let stopped_vms = metrics.total_vms - metrics.running_vms;
    
    let content = vec![
        Line::from(vec![
            Span::styled("Total: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(metrics.total_vms.to_string(), Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("Running: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(metrics.running_vms.to_string(), Style::default().fg(SUCCESS_COLOR)),
        ]),
        Line::from(vec![
            Span::styled("Stopped: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(stopped_vms.to_string(), Style::default().fg(WARNING_COLOR)),
        ]),
        Line::from(vec![
            Span::styled(format!("Health: {:.1}%", running_ratio), 
                        Style::default().fg(if running_ratio > 80.0 { SUCCESS_COLOR } else { WARNING_COLOR })),
        ]),
    ];
    
    let paragraph = Paragraph::new(content)
        .block(Block::default()
            .borders(Borders::ALL)
            .title("🖥️ VM Status")
            .border_style(Style::default().fg(PRIMARY_COLOR)))
        .wrap(Wrap { trim: true });
    
    f.render_widget(paragraph, area);
}

fn render_quick_actions_card(f: &mut Frame, area: Rect, _app: &App) {
    let actions = vec![
        "⌨️ Shortcuts:",
        "",
        "c - Create VM",
        "n - Add Node", 
        "r - Refresh",
        "1-5 - Switch Tabs",
        "h - Help",
        "q - Quit",
    ];
    
    let content: Vec<Line> = actions.iter()
        .map(|&action| {
            if action.starts_with("⌨️") {
                Line::from(Span::styled(action, Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)))
            } else if action.is_empty() {
                Line::from("")
            } else {
                Line::from(Span::styled(action, Style::default().fg(TEXT_COLOR)))
            }
        })
        .collect();
    
    let paragraph = Paragraph::new(content)
        .block(Block::default()
            .borders(Borders::ALL)
            .title("⚡ Quick Actions")
            .border_style(Style::default().fg(PRIMARY_COLOR)))
        .wrap(Wrap { trim: true });
    
    f.render_widget(paragraph, area);
}

fn render_recent_events(f: &mut Frame, area: Rect, app: &App) {
    let events: Vec<ListItem> = app.recent_events
        .iter()
        .take(10) // Show last 10 events
        .map(|event| {
            let time_ago = format!("{:.0}s ago", event.timestamp.elapsed().as_secs());
            let content = format!("{} [{}] {}: {}", 
                event.level.icon(),
                time_ago,
                event.source,
                event.message
            );
            ListItem::new(content)
                .style(Style::default().fg(event.level.color()))
        })
        .collect();
    
    let events_list = List::new(events)
        .block(Block::default()
            .borders(Borders::ALL)
            .title("📋 Recent Events")
            .border_style(Style::default().fg(PRIMARY_COLOR)))
        .style(Style::default().fg(TEXT_COLOR));
    
    f.render_widget(events_list, area);
}

fn render_system_overview(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50), // CPU history sparkline
            Constraint::Percentage(50), // Memory history sparkline
        ])
        .split(area.inner(&Margin { vertical: 1, horizontal: 1 }));
    
    // CPU sparkline
    if !app.cpu_history.is_empty() {
        let cpu_data: Vec<u64> = app.cpu_history.iter().map(|&x| x as u64).collect();
        let cpu_sparkline = Sparkline::default()
            .block(Block::default().title("CPU History"))
            .data(&cpu_data)
            .style(Style::default().fg(PRIMARY_COLOR));
        f.render_widget(cpu_sparkline, chunks[0]);
    }
    
    // Memory sparkline
    if !app.memory_history.is_empty() {
        let memory_data: Vec<u64> = app.memory_history.iter().map(|&x| x as u64).collect();
        let memory_sparkline = Sparkline::default()
            .block(Block::default().title("Memory History"))
            .data(&memory_data)
            .style(Style::default().fg(SECONDARY_COLOR));
        f.render_widget(memory_sparkline, chunks[1]);
    }
    
    let block = Block::default()
        .borders(Borders::ALL)
        .title("📈 System Overview")
        .border_style(Style::default().fg(PRIMARY_COLOR));
    
    f.render_widget(block, area);
}

fn render_vm_management(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // VM management toolbar
            Constraint::Min(0),     // VM table
        ])
        .split(area);
    
    render_vm_toolbar(f, chunks[0], app);
    render_vm_table(f, chunks[1], app);
}

fn render_vm_toolbar(f: &mut Frame, area: Rect, _app: &App) {
    let content = vec![
        Line::from(vec![
            Span::styled("VM Management", Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
            Span::styled(" | ", Style::default().fg(TEXT_COLOR)),
            Span::styled("c", Style::default().fg(SUCCESS_COLOR).add_modifier(Modifier::BOLD)),
            Span::styled(" Create", Style::default().fg(TEXT_COLOR)),
            Span::styled(" | ", Style::default().fg(TEXT_COLOR)),
            Span::styled("Enter", Style::default().fg(SUCCESS_COLOR).add_modifier(Modifier::BOLD)),
            Span::styled(" Details", Style::default().fg(TEXT_COLOR)),
            Span::styled(" | ", Style::default().fg(TEXT_COLOR)),
            Span::styled("d", Style::default().fg(ERROR_COLOR).add_modifier(Modifier::BOLD)),
            Span::styled(" Delete", Style::default().fg(TEXT_COLOR)),
            Span::styled(" | ", Style::default().fg(TEXT_COLOR)),
            Span::styled("↑↓", Style::default().fg(INFO_COLOR).add_modifier(Modifier::BOLD)),
            Span::styled(" Navigate", Style::default().fg(TEXT_COLOR)),
        ]),
    ];
    
    let paragraph = Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);
    
    f.render_widget(paragraph, area);
}

fn render_vm_table(f: &mut Frame, area: Rect, app: &App) {
    let header = Row::new(vec![
        Cell::from("Name").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("Status").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("Node").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("vCPUs").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("Memory").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("IP Address").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("Uptime").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
    ]);
    
    let displayed_vms = app.get_displayed_vms();
    let rows: Vec<Row> = displayed_vms.iter().map(|vm| {
        let status_style = match vm.status {
            VmStatus::Running => Style::default().fg(SUCCESS_COLOR),
            VmStatus::Stopped => Style::default().fg(WARNING_COLOR),
            VmStatus::Failed => Style::default().fg(ERROR_COLOR),
            _ => Style::default().fg(TEXT_COLOR),
        };
        
        let ip_display = vm.ip_address.as_deref().unwrap_or("Not assigned");
        let uptime_display = vm.uptime.as_deref().unwrap_or("N/A");
        
        Row::new(vec![
            Cell::from(vm.name.clone()),
            Cell::from(format!("{:?}", vm.status)).style(status_style),
            Cell::from(vm.node_id.to_string()),
            Cell::from(vm.vcpus.to_string()),
            Cell::from(format!("{}MB", vm.memory)),
            Cell::from(ip_display),
            Cell::from(uptime_display),
        ])
    }).collect();
    
    let table = Table::new(rows, [
        Constraint::Min(15),  // Name
        Constraint::Min(10),  // Status
        Constraint::Min(8),   // Node
        Constraint::Min(8),   // vCPUs
        Constraint::Min(12),  // Memory
        Constraint::Min(15),  // IP Address
        Constraint::Min(12),  // Uptime
    ])
    .header(header)
    .block(Block::default()
        .borders(Borders::ALL)
        .title(get_vm_table_title(app))
        .border_style(Style::default().fg(PRIMARY_COLOR)))
    .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
    .highlight_symbol(">> ");
    
    f.render_stateful_widget(table, area, &mut app.vm_table_state.clone());
}

fn render_node_management(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Node management toolbar
            Constraint::Min(0),     // Node table
        ])
        .split(area);
    
    render_node_toolbar(f, chunks[0], app);
    render_node_table(f, chunks[1], app);
}

fn render_node_toolbar(f: &mut Frame, area: Rect, _app: &App) {
    let content = vec![
        Line::from(vec![
            Span::styled("Node Management", Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
            Span::styled(" | ", Style::default().fg(TEXT_COLOR)),
            Span::styled("a", Style::default().fg(SUCCESS_COLOR).add_modifier(Modifier::BOLD)),
            Span::styled(" Add Node", Style::default().fg(TEXT_COLOR)),
            Span::styled(" | ", Style::default().fg(TEXT_COLOR)),
            Span::styled("Enter", Style::default().fg(SUCCESS_COLOR).add_modifier(Modifier::BOLD)),
            Span::styled(" Details", Style::default().fg(TEXT_COLOR)),
            Span::styled(" | ", Style::default().fg(TEXT_COLOR)),
            Span::styled("s", Style::default().fg(WARNING_COLOR).add_modifier(Modifier::BOLD)),
            Span::styled(" Start Daemon", Style::default().fg(TEXT_COLOR)),
        ]),
    ];
    
    let paragraph = Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);
    
    f.render_widget(paragraph, area);
}

fn render_node_table(f: &mut Frame, area: Rect, app: &App) {
    let header = Row::new(vec![
        Cell::from("ID").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("Address").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("Status").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("Role").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("VMs").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("CPU%").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("Memory%").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("Last Seen").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
    ]);
    
    let displayed_nodes = app.get_displayed_nodes();
    let rows: Vec<Row> = displayed_nodes.iter().map(|node| {
        let status_style = Style::default().fg(node.status.color());
        
        let role_text = match node.role {
            NodeRole::Leader => "👑 Leader",
            NodeRole::Follower => "🔄 Follower", 
            NodeRole::Candidate => "🗳️ Candidate",
            NodeRole::Unknown => "❓ Unknown",
        };
        
        let last_seen = match node.last_seen {
            Some(instant) => {
                let secs = instant.elapsed().as_secs();
                if secs < 60 {
                    format!("{}s ago", secs)
                } else {
                    format!("{}m ago", secs / 60)
                }
            }
            None => "Never".to_string(),
        };
        
        Row::new(vec![
            Cell::from(node.id.to_string()),
            Cell::from(node.address.clone()),
            Cell::from(format!("{:?}", node.status)).style(status_style),
            Cell::from(role_text),
            Cell::from(node.vm_count.to_string()),
            Cell::from(format!("{:.1}%", node.cpu_usage)),
            Cell::from(format!("{:.1}%", node.memory_usage)),
            Cell::from(last_seen),
        ])
    }).collect();
    
    let table = Table::new(rows, [
        Constraint::Min(6),   // ID
        Constraint::Min(15),  // Address
        Constraint::Min(10),  // Status
        Constraint::Min(12),  // Role
        Constraint::Min(6),   // VMs
        Constraint::Min(8),   // CPU%
        Constraint::Min(10),  // Memory%
        Constraint::Min(12),  // Last Seen
    ])
    .header(header)
    .block(Block::default()
        .borders(Borders::ALL)
        .title(get_node_table_title(app))
        .border_style(Style::default().fg(PRIMARY_COLOR)))
    .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
    .highlight_symbol(">> ");
    
    f.render_stateful_widget(table, area, &mut app.node_table_state.clone());
}

fn render_monitoring(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50), // Resource charts
            Constraint::Percentage(50), // Performance metrics
        ])
        .split(area);
    
    let resource_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // CPU chart
            Constraint::Percentage(50), // Memory chart
        ])
        .split(chunks[0]);
    
    render_cpu_chart(f, resource_chunks[0], app);
    render_memory_chart(f, resource_chunks[1], app);
    render_performance_metrics(f, chunks[1], app);
}

fn render_cpu_chart(f: &mut Frame, area: Rect, app: &App) {
    if app.cpu_history.is_empty() {
        let empty = Paragraph::new("No CPU data available")
            .block(Block::default()
                .borders(Borders::ALL)
                .title("📊 CPU Usage")
                .border_style(Style::default().fg(PRIMARY_COLOR)))
            .alignment(Alignment::Center);
        f.render_widget(empty, area);
        return;
    }
    
    let data: Vec<u64> = app.cpu_history.iter().map(|&x| x as u64).collect();
    let sparkline = Sparkline::default()
        .block(Block::default()
            .borders(Borders::ALL)
            .title("📊 CPU Usage History")
            .border_style(Style::default().fg(PRIMARY_COLOR)))
        .data(&data)
        .style(Style::default().fg(SUCCESS_COLOR));
    
    f.render_widget(sparkline, area);
}

fn render_memory_chart(f: &mut Frame, area: Rect, app: &App) {
    if app.memory_history.is_empty() {
        let empty = Paragraph::new("No memory data available")
            .block(Block::default()
                .borders(Borders::ALL)
                .title("🧠 Memory Usage")
                .border_style(Style::default().fg(PRIMARY_COLOR)))
            .alignment(Alignment::Center);
        f.render_widget(empty, area);
        return;
    }
    
    let data: Vec<u64> = app.memory_history.iter().map(|&x| x as u64).collect();
    let sparkline = Sparkline::default()
        .block(Block::default()
            .borders(Borders::ALL)
            .title("🧠 Memory Usage History")
            .border_style(Style::default().fg(PRIMARY_COLOR)))
        .data(&data)
        .style(Style::default().fg(SECONDARY_COLOR));
    
    f.render_widget(sparkline, area);
}

fn render_performance_metrics(f: &mut Frame, area: Rect, _app: &App) {
    let content = vec![
        Line::from("📈 Performance Metrics"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Cluster Health: ", Style::default().fg(TEXT_COLOR)),
            Span::styled("98.5%", Style::default().fg(SUCCESS_COLOR).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("Average Response Time: ", Style::default().fg(TEXT_COLOR)),
            Span::styled("125ms", Style::default().fg(PRIMARY_COLOR)),
        ]),
        Line::from(vec![
            Span::styled("Requests/sec: ", Style::default().fg(TEXT_COLOR)),
            Span::styled("1,247", Style::default().fg(PRIMARY_COLOR)),
        ]),
        Line::from(vec![
            Span::styled("Raft Consensus: ", Style::default().fg(TEXT_COLOR)),
            Span::styled("Healthy", Style::default().fg(SUCCESS_COLOR)),
        ]),
        Line::from(vec![
            Span::styled("Storage: ", Style::default().fg(TEXT_COLOR)),
            Span::styled("Synchronized", Style::default().fg(SUCCESS_COLOR)),
        ]),
    ];
    
    let paragraph = Paragraph::new(content)
        .block(Block::default()
            .borders(Borders::ALL)
            .title("⚡ Performance Overview")
            .border_style(Style::default().fg(PRIMARY_COLOR)))
        .wrap(Wrap { trim: true });
    
    f.render_widget(paragraph, area);
}

fn render_configuration(f: &mut Frame, area: Rect, _app: &App) {
    let content = vec![
        Line::from("⚙️ Configuration Management"),
        Line::from(""),
        Line::from("🔐 Security Settings"),
        Line::from("  • Authentication: Enabled"),
        Line::from("  • TLS: Disabled"),
        Line::from("  • API Keys: 3 active"),
        Line::from(""),
        Line::from("📊 Resource Quotas"),
        Line::from("  • Default CPU Limit: 8 cores"),
        Line::from("  • Default Memory Limit: 16GB"),
        Line::from("  • Max VMs per tenant: 10"),
        Line::from(""),
        Line::from("🔧 VM Templates"),
        Line::from("  • Web Server: 2 vCPU, 4GB RAM"),
        Line::from("  • Database: 4 vCPU, 8GB RAM"),
        Line::from("  • Worker Node: 1 vCPU, 2GB RAM"),
        Line::from(""),
        Line::from("🌐 Network Configuration"),
        Line::from("  • Default Backend: microvm"),
        Line::from("  • Networking: Routed (TAP)"),
        Line::from("  • IP Range: 10.0.0.0/24"),
    ];
    
    let paragraph = Paragraph::new(content)
        .block(Block::default()
            .borders(Borders::ALL)
            .title("⚙️ System Configuration")
            .border_style(Style::default().fg(PRIMARY_COLOR)))
        .wrap(Wrap { trim: true });
    
    f.render_widget(paragraph, area);
}

fn render_debug(f: &mut Frame, area: Rect, app: &App) {
    match app.mode {
        AppMode::RaftDebug => render_raft_debug(f, area, app),
        AppMode::DebugMetrics => render_debug_metrics(f, area, app),
        AppMode::DebugLogs => render_debug_logs(f, area, app),
        _ => render_debug_overview(f, area, app),
    }
}

fn render_debug_overview(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),  // Debug mode selector
            Constraint::Min(0),     // Content area
        ])
        .split(area);
    
    // Debug mode selector
    render_debug_mode_selector(f, chunks[0], app);
    
    // Debug overview content
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // Raft status
            Constraint::Percentage(50), // Debug metrics summary
        ])
        .split(chunks[1]);
    
    render_raft_status_summary(f, content_chunks[0], app);
    render_debug_metrics_summary(f, content_chunks[1], app);
}

fn render_debug_mode_selector(f: &mut Frame, area: Rect, _app: &App) {
    let content = vec![
        Line::from(vec![
            Span::styled("🐛 Debug Mode", Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
            Span::styled(" - Advanced cluster diagnostics", Style::default().fg(TEXT_COLOR)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("r", Style::default().fg(SUCCESS_COLOR).add_modifier(Modifier::BOLD)),
            Span::styled(" Raft Debug", Style::default().fg(TEXT_COLOR)),
            Span::styled(" | ", Style::default().fg(TEXT_COLOR)),
            Span::styled("m", Style::default().fg(SUCCESS_COLOR).add_modifier(Modifier::BOLD)),
            Span::styled(" Metrics", Style::default().fg(TEXT_COLOR)),
            Span::styled(" | ", Style::default().fg(TEXT_COLOR)),
            Span::styled("l", Style::default().fg(SUCCESS_COLOR).add_modifier(Modifier::BOLD)),
            Span::styled(" Logs", Style::default().fg(TEXT_COLOR)),
            Span::styled(" | ", Style::default().fg(TEXT_COLOR)),
            Span::styled("s", Style::default().fg(WARNING_COLOR).add_modifier(Modifier::BOLD)),
            Span::styled(" Simulate", Style::default().fg(TEXT_COLOR)),
        ]),
        Line::from(vec![
            Span::styled("R", Style::default().fg(ERROR_COLOR).add_modifier(Modifier::BOLD)),
            Span::styled(" Reset Metrics", Style::default().fg(TEXT_COLOR)),
            Span::styled(" | ", Style::default().fg(TEXT_COLOR)),
            Span::styled("c", Style::default().fg(ERROR_COLOR).add_modifier(Modifier::BOLD)),
            Span::styled(" Clear Logs", Style::default().fg(TEXT_COLOR)),
            Span::styled(" | ", Style::default().fg(TEXT_COLOR)),
            Span::styled("Esc", Style::default().fg(INFO_COLOR).add_modifier(Modifier::BOLD)),
            Span::styled(" Exit Debug", Style::default().fg(TEXT_COLOR)),
        ]),
    ];
    
    let paragraph = Paragraph::new(content)
        .block(Block::default()
            .borders(Borders::ALL)
            .title("🐛 Debug Controls")
            .border_style(Style::default().fg(PRIMARY_COLOR)))
        .wrap(Wrap { trim: true });
    
    f.render_widget(paragraph, area);
}

fn render_raft_status_summary(f: &mut Frame, area: Rect, app: &App) {
    let content = if let Some(ref raft_info) = app.raft_debug_info {
        vec![
            Line::from(vec![
                Span::styled("State: ", Style::default().fg(TEXT_COLOR)),
                Span::styled(format!("{:?}", raft_info.state), 
                           get_raft_state_style(&raft_info.state)),
            ]),
            Line::from(vec![
                Span::styled("Term: ", Style::default().fg(TEXT_COLOR)),
                Span::styled(raft_info.current_term.to_string(), Style::default().fg(PRIMARY_COLOR)),
            ]),
            Line::from(vec![
                Span::styled("Log Length: ", Style::default().fg(TEXT_COLOR)),
                Span::styled(raft_info.log_length.to_string(), Style::default().fg(INFO_COLOR)),
            ]),
            Line::from(vec![
                Span::styled("Commit Index: ", Style::default().fg(TEXT_COLOR)),
                Span::styled(raft_info.commit_index.to_string(), Style::default().fg(SUCCESS_COLOR)),
            ]),
            Line::from(vec![
                Span::styled("Last Applied: ", Style::default().fg(TEXT_COLOR)),
                Span::styled(raft_info.last_applied.to_string(), Style::default().fg(SUCCESS_COLOR)),
            ]),
            Line::from(vec![
                Span::styled("Peers: ", Style::default().fg(TEXT_COLOR)),
                Span::styled(raft_info.peer_states.len().to_string(), Style::default().fg(SECONDARY_COLOR)),
            ]),
        ]
    } else {
        vec![
            Line::from("No Raft debug info available"),
            Line::from(""),
            Line::from("Press 's' to simulate debug data"),
        ]
    };
    
    let paragraph = Paragraph::new(content)
        .block(Block::default()
            .borders(Borders::ALL)
            .title("⚙️ Raft Status")
            .border_style(Style::default().fg(PRIMARY_COLOR)))
        .wrap(Wrap { trim: true });
    
    f.render_widget(paragraph, area);
}

fn render_debug_metrics_summary(f: &mut Frame, area: Rect, app: &App) {
    let metrics = &app.debug_metrics;
    let uptime = metrics.last_reset.elapsed();
    
    let content = vec![
        Line::from(vec![
            Span::styled("Messages Sent: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(metrics.messages_sent.to_string(), Style::default().fg(SUCCESS_COLOR)),
        ]),
        Line::from(vec![
            Span::styled("Messages Received: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(metrics.messages_received.to_string(), Style::default().fg(SUCCESS_COLOR)),
        ]),
        Line::from(vec![
            Span::styled("Proposals: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(format!("{}/{}", metrics.proposals_committed, metrics.proposals_submitted), 
                       Style::default().fg(PRIMARY_COLOR)),
        ]),
        Line::from(vec![
            Span::styled("Elections: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(metrics.elections_started.to_string(), Style::default().fg(WARNING_COLOR)),
        ]),
        Line::from(vec![
            Span::styled("Leadership Changes: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(metrics.leadership_changes.to_string(), Style::default().fg(WARNING_COLOR)),
        ]),
        Line::from(vec![
            Span::styled("Uptime: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(format!("{:.1}s", uptime.as_secs_f64()), Style::default().fg(INFO_COLOR)),
        ]),
    ];
    
    let paragraph = Paragraph::new(content)
        .block(Block::default()
            .borders(Borders::ALL)
            .title("📈 Debug Metrics")
            .border_style(Style::default().fg(PRIMARY_COLOR)))
        .wrap(Wrap { trim: true });
    
    f.render_widget(paragraph, area);
}

fn get_raft_state_style(state: &RaftNodeState) -> Style {
    match state {
        RaftNodeState::Leader => Style::default().fg(SUCCESS_COLOR).add_modifier(Modifier::BOLD),
        RaftNodeState::Follower => Style::default().fg(PRIMARY_COLOR),
        RaftNodeState::Candidate => Style::default().fg(WARNING_COLOR).add_modifier(Modifier::BOLD),
        RaftNodeState::PreCandidate => Style::default().fg(WARNING_COLOR),
    }
}

fn render_help(f: &mut Frame, area: Rect, _app: &App) {
    let content = vec![
        Line::from("❓ Blixard TUI Help - Enhanced Edition"),
        Line::from(""),
        Line::from("🔥 Global Shortcuts:"),
        Line::from("  q / Ctrl+C   - Quit application"),
        Line::from("  h            - Show this help"),
        Line::from("  r            - Refresh all data"),
        Line::from("  1-6          - Switch between tabs (6 = Debug mode)"),
        Line::from("  p            - Cycle performance mode (PowerSaver → Balanced → HighRefresh → Debug)"),
        Line::from("  v            - Toggle vim mode (hjkl navigation)"),
        Line::from("  d            - Toggle debug mode"),
        Line::from(""),
        Line::from("🔍 Search & Filtering (lazygit-inspired):"),
        Line::from("  /            - Enter search mode (context-aware)"),
        Line::from("  f            - Quick filter mode"),
        Line::from("  Shift+F      - Clear all filters"),
        Line::from("  In search:   Type to filter, Enter to apply, Esc to cancel"),
        Line::from(""),
        Line::from("🎮 Vim Mode (when enabled):"),
        Line::from("  h/l          - Switch tabs left/right"),
        Line::from("  j/k          - Navigate down/up in lists"),
        Line::from("  All standard navigation still works (arrow keys, mouse)"),
        Line::from(""),
        Line::from("🖱️ Mouse Support:"),
        Line::from("  Left Click   - Switch tabs / Select items"),
        Line::from("  Scroll Up    - Navigate up in lists"),
        Line::from("  Scroll Down  - Navigate down in lists"),
        Line::from("  Click Tabs   - Switch between sections"),
        Line::from("  Click Lists  - Select VMs/Nodes"),
        Line::from(""),
        Line::from("📊 Dashboard:"),
        Line::from("  c            - Create new VM"),
        Line::from("  n            - Add new node"),
        Line::from("  s            - Show cluster status"),
        Line::from("  C            - Discover clusters"),
        Line::from("  N            - Create new cluster"),
        Line::from(""),
        Line::from("🖥️ VM Management:"),
        Line::from("  c            - Create VM"),
        Line::from("  Enter        - View VM details"),
        Line::from("  d            - Delete VM"),
        Line::from("  /            - Search VMs by name"),
        Line::from("  f            - Quick filter VMs"),
        Line::from("  ↑/↓ or j/k   - Navigate list"),
        Line::from(""),
        Line::from("🔗 Node Management:"),
        Line::from("  a            - Add node"),
        Line::from("  t            - Select from node templates"),
        Line::from("  d            - Destroy selected node"),
        Line::from("  r            - Restart selected node"),
        Line::from("  s            - Scale cluster"),
        Line::from("  Enter        - View node details"),
        Line::from("  /            - Search nodes by address"),
        Line::from("  f            - Quick filter nodes"),
        Line::from("  ↑/↓ or j/k   - Navigate list"),
        Line::from(""),
        Line::from("⚡ Performance Modes (btop-inspired):"),
        Line::from("  PowerSaver   - Reduced refresh rate for battery saving"),
        Line::from("  Balanced     - Default refresh rate (2 seconds)"),
        Line::from("  HighRefresh  - Fast updates for monitoring"),
        Line::from("  Debug        - Maximum detail with logging"),
        Line::from(""),
        Line::from("🏗️ Cluster Management:"),
        Line::from("  Shift+C      - Discover clusters on the network"),
        Line::from("  Shift+N      - Quick cluster creation from templates"),
        Line::from("  In Discovery - Enter to connect, R to refresh"),
        Line::from("  In Creation  - 1-2 to select template"),
        Line::from(""),
        Line::from("🐛 Debug Mode (Tab 6):"),
        Line::from("  r            - Raft state visualization"),
        Line::from("  m            - Debug metrics detail"),
        Line::from("  l            - Debug logs viewer"),
        Line::from("  s            - Simulate debug data"),
        Line::from("  R            - Reset debug metrics"),
        Line::from("  c            - Clear debug logs"),
        Line::from("  Esc          - Exit debug mode"),
        Line::from(""),
        Line::from("💡 Pro Tips:"),
        Line::from("  • Mouse compatible! Click anywhere to interact"),
        Line::from("  • Watch live shortcuts in bottom-right corner"),
        Line::from("  • Filter indicators show in table titles"),
        Line::from("  • Status bar shows performance mode and active features"),
        Line::from("  • Search is live - results update as you type"),
        Line::from("  • All navigation methods work together (vim + mouse + arrows)"),
        Line::from("  • Cluster discovery scans common ports automatically"),
        Line::from("  • Templates provide quick cluster deployment"),
        Line::from("  • Debug mode provides real-time Raft state visualization"),
        Line::from("  • Hot-reload development: ./scripts/dev-workflow.sh start"),
    ];
    
    let paragraph = Paragraph::new(content)
        .block(Block::default()
            .borders(Borders::ALL)
            .title("❓ Help & Shortcuts")
            .border_style(Style::default().fg(PRIMARY_COLOR)))
        .wrap(Wrap { trim: true });
    
    f.render_widget(paragraph, area);
}

fn render_status_bar(f: &mut Frame, area: Rect, app: &App) {
    // Split status bar into left (status) and right (shortcuts)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(70),  // Status info
            Constraint::Percentage(30),  // Shortcuts
        ])
        .split(area);
    
    // Left side: Status information
    let connection_status = if app.vm_client.is_some() {
        "🟢 Connected"
    } else {
        "🔴 Disconnected"
    };
    
    let leader_info = match app.cluster_metrics.leader_id {
        Some(id) => format!("Leader: Node {}", id),
        None => "Leader: None".to_string(),
    };
    
    let vm_count = format!("VMs: {}/{}", app.cluster_metrics.running_vms, app.cluster_metrics.total_vms);
    let node_count = format!("Nodes: {}/{}", app.cluster_metrics.healthy_nodes, app.cluster_metrics.total_nodes);
    
    // Add performance and mode indicators
    let perf_mode = format!("Perf: {:?}", app.settings.performance_mode);
    let mode_indicators = format!("{}{}{}",
        if app.settings.vim_mode { " VIM" } else { "" },
        if app.settings.debug_mode { " DEBUG" } else { "" },
        if app.search_mode != super::app::SearchMode::None { " SEARCH" } else { "" }
    );
    
    let status_text = if let Some(msg) = &app.status_message {
        format!("✅ {}", msg)
    } else if let Some(msg) = &app.error_message {
        format!("❌ {}", msg)
    } else {
        format!("{} | {} | {} | {} | {}{}", 
            connection_status, leader_info, vm_count, node_count, perf_mode, mode_indicators)
    };
    
    let status_style = if app.error_message.is_some() {
        Style::default().fg(ERROR_COLOR)
    } else if app.status_message.is_some() {
        Style::default().fg(SUCCESS_COLOR)
    } else {
        Style::default().fg(TEXT_COLOR)
    };
    
    let status_paragraph = Paragraph::new(status_text)
        .style(status_style)
        .block(Block::default().borders(Borders::ALL).title("Status"))
        .alignment(Alignment::Left);
    
    f.render_widget(status_paragraph, chunks[0]);
    
    // Right side: Keyboard shortcuts
    let shortcuts = get_current_shortcuts(app);
    let shortcuts_paragraph = Paragraph::new(shortcuts)
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::ALL).title("Keys"))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });
    
    f.render_widget(shortcuts_paragraph, chunks[1]);
}

fn get_current_shortcuts(app: &App) -> String {
    if app.search_mode != super::app::SearchMode::None {
        return "Type to filter  Enter Apply  Esc Cancel".to_string();
    }
    
    let vim_suffix = if app.settings.vim_mode { "  hjkl Nav" } else { "" };
    
    match app.mode {
        AppMode::VmList => {
            format!("↑↓ Select  Enter View  C Create  / Search  F Filter{}", vim_suffix)
        }
        AppMode::NodeList => {
            format!("↑↓ Select  Enter View  A Add  / Search  F Filter{}", vim_suffix)
        }
        AppMode::CreateVmForm | AppMode::CreateNodeForm => {
            "Tab Next  Shift+Tab Prev  Enter Submit  Esc Cancel".to_string()
        }
        AppMode::CreateClusterForm => {
            "1-2 Select Template  Esc Cancel".to_string()
        }
        AppMode::ClusterDiscovery => {
            "Enter Connect  R Refresh  Esc Close".to_string()
        }
        AppMode::ConfirmDialog => {
            "←→ Toggle  Enter Confirm  Esc Cancel".to_string()
        }
        _ => {
            format!("1-5 Tabs  R Refresh  P Perf  V Vim  Shift+C Discover{}", vim_suffix)
        }
    }
}

/// Get VM table title with filter info
fn get_vm_table_title(app: &App) -> String {
    let total = app.vms.len();
    let displayed = app.get_displayed_vms().len();
    
    let filter_info = match &app.vm_filter {
        super::app::VmFilter::All => {
            if app.quick_filter.is_empty() {
                String::new()
            } else {
                format!(" [filter: {}]", app.quick_filter)
            }
        }
        super::app::VmFilter::Running => " [running]".to_string(),
        super::app::VmFilter::Stopped => " [stopped]".to_string(),
        super::app::VmFilter::Failed => " [failed]".to_string(),
        super::app::VmFilter::ByNode(id) => format!(" [node: {}]", id),
        super::app::VmFilter::ByName(name) => format!(" [name: {}]", name),
    };
    
    if displayed == total {
        format!("🖥️ Virtual Machines ({} total){}", total, filter_info)
    } else {
        format!("🖥️ Virtual Machines ({}/{} shown){}", displayed, total, filter_info)
    }
}

/// Get node table title with filter info
fn get_node_table_title(app: &App) -> String {
    let total = app.nodes.len();
    let displayed = app.get_displayed_nodes().len();
    
    let filter_info = match &app.node_filter {
        super::app::NodeFilter::All => {
            if app.quick_filter.is_empty() {
                String::new()
            } else {
                format!(" [filter: {}]", app.quick_filter)
            }
        }
        super::app::NodeFilter::Healthy => " [healthy]".to_string(),
        super::app::NodeFilter::Warning => " [warning]".to_string(),
        super::app::NodeFilter::Critical => " [critical]".to_string(),
        super::app::NodeFilter::Leaders => " [leaders]".to_string(),
        super::app::NodeFilter::Followers => " [followers]".to_string(),
    };
    
    if displayed == total {
        format!("🔗 Cluster Nodes ({} total){}", total, filter_info)
    } else {
        format!("🔗 Cluster Nodes ({}/{} shown){}", displayed, total, filter_info)
    }
}

// Popup/Overlay rendering functions

fn render_create_vm_form(f: &mut Frame, app: &App) {
    let area = centered_rect(80, 24, f.size());
    f.render_widget(Clear, area);
    
    let block = Block::default()
        .title("🖥️ Create New VM")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY_COLOR));
    
    f.render_widget(block, area);
    
    let inner_area = area.inner(&Margin { vertical: 1, horizontal: 2 });
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Name field
            Constraint::Length(2), // vCPUs field
            Constraint::Length(2), // Memory field
            Constraint::Length(2), // Config Path field
            Constraint::Length(2), // Placement Strategy field
            Constraint::Length(2), // Node ID field
            Constraint::Length(2), // Auto Start field
            Constraint::Length(3), // Submit button
            Constraint::Min(0),    // Remaining space
        ])
        .split(inner_area);
    
    let form = &app.create_vm_form;
    
    // Name field
    let name_style = if form.current_field == CreateVmField::Name {
        Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_COLOR)
    };
    let name_field = Paragraph::new(form.name.as_str())
        .style(name_style)
        .block(Block::default().borders(Borders::ALL).title("VM Name *"));
    f.render_widget(name_field, chunks[0]);
    
    // vCPUs field
    let vcpus_style = if form.current_field == CreateVmField::Vcpus {
        Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_COLOR)
    };
    let vcpus_field = Paragraph::new(form.vcpus.as_str())
        .style(vcpus_style)
        .block(Block::default().borders(Borders::ALL).title("vCPUs *"));
    f.render_widget(vcpus_field, chunks[1]);
    
    // Memory field
    let memory_style = if form.current_field == CreateVmField::Memory {
        Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_COLOR)
    };
    let memory_field = Paragraph::new(form.memory.as_str())
        .style(memory_style)
        .block(Block::default().borders(Borders::ALL).title("Memory (MB) *"));
    f.render_widget(memory_field, chunks[2]);
    
    // Config Path field
    let config_style = if form.current_field == CreateVmField::ConfigPath {
        Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_COLOR)
    };
    let config_field = Paragraph::new(form.config_path.as_str())
        .style(config_style)
        .block(Block::default().borders(Borders::ALL).title("Config Path (optional)"));
    f.render_widget(config_field, chunks[3]);
    
    // Placement Strategy field
    let strategy_style = if form.current_field == CreateVmField::PlacementStrategy {
        Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_COLOR)
    };
    let strategy_text = format!("{:?}", form.placement_strategy);
    let strategy_field = Paragraph::new(strategy_text)
        .style(strategy_style)
        .block(Block::default().borders(Borders::ALL).title("Placement Strategy (↑↓ to change)"));
    f.render_widget(strategy_field, chunks[4]);
    
    // Node ID field
    let node_id_style = if form.current_field == CreateVmField::NodeId {
        Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_COLOR)
    };
    let node_id_display = form.node_id.map(|id| id.to_string()).unwrap_or_else(|| "".to_string());
    let node_id_field = Paragraph::new(node_id_display)
        .style(node_id_style)
        .block(Block::default().borders(Borders::ALL).title("Node ID (optional, leave empty for auto)"));
    f.render_widget(node_id_field, chunks[5]);
    
    // Auto Start field
    let auto_start_style = if form.current_field == CreateVmField::AutoStart {
        Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_COLOR)
    };
    let auto_start_text = if form.auto_start { "Yes" } else { "No" };
    let auto_start_field = Paragraph::new(auto_start_text)
        .style(auto_start_style)
        .block(Block::default().borders(Borders::ALL).title("Auto Start (↑↓ to toggle)"));
    f.render_widget(auto_start_field, chunks[6]);
    
    // Submit button
    let submit_button = Paragraph::new("[ Press Enter to Create VM ]")
        .style(Style::default().fg(SUCCESS_COLOR).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(submit_button, chunks[7]);
}

fn render_create_node_form(f: &mut Frame, app: &App) {
    let area = centered_rect(80, 20, f.size());
    f.render_widget(Clear, area);
    
    let block = Block::default()
        .title("🔗 Join Node to Cluster")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY_COLOR));
    
    f.render_widget(block, area);
    
    let inner_area = area.inner(&Margin { vertical: 1, horizontal: 2 });
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Node ID field
            Constraint::Length(2), // Bind Address field
            Constraint::Length(2), // Data Dir field
            Constraint::Length(2), // Peers field
            Constraint::Length(2), // VM Backend field
            Constraint::Length(2), // Daemon Mode field
            Constraint::Length(3), // Submit button
            Constraint::Min(0),    // Remaining space
        ])
        .split(inner_area);
    
    let form = &app.create_node_form;
    
    // Node ID field
    let id_style = if form.current_field == CreateNodeField::Id {
        Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_COLOR)
    };
    let id_field = Paragraph::new(form.id.as_str())
        .style(id_style)
        .block(Block::default().borders(Borders::ALL).title("Node ID *"));
    f.render_widget(id_field, chunks[0]);
    
    // Bind Address field
    let bind_style = if form.current_field == CreateNodeField::BindAddress {
        Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_COLOR)
    };
    let bind_field = Paragraph::new(form.bind_address.as_str())
        .style(bind_style)
        .block(Block::default().borders(Borders::ALL).title("Bind Address * (host:port, e.g. 127.0.0.1:7003)"));
    f.render_widget(bind_field, chunks[1]);
    
    // Data Dir field
    let data_dir_style = if form.current_field == CreateNodeField::DataDir {
        Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_COLOR)
    };
    let data_dir_field = Paragraph::new(form.data_dir.as_str())
        .style(data_dir_style)
        .block(Block::default().borders(Borders::ALL).title("Data Directory (optional)"));
    f.render_widget(data_dir_field, chunks[2]);
    
    // Peers field
    let peers_style = if form.current_field == CreateNodeField::Peers {
        Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_COLOR)
    };
    let peers_field = Paragraph::new(form.peers.as_str())
        .style(peers_style)
        .block(Block::default().borders(Borders::ALL).title("Peers (optional, comma-separated addresses)"));
    f.render_widget(peers_field, chunks[3]);
    
    // VM Backend field
    let backend_style = if form.current_field == CreateNodeField::VmBackend {
        Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_COLOR)
    };
    let backend_field = Paragraph::new(form.vm_backend.as_str())
        .style(backend_style)
        .block(Block::default().borders(Borders::ALL).title("VM Backend (microvm, mock, etc.)"));
    f.render_widget(backend_field, chunks[4]);
    
    // Daemon Mode field
    let daemon_style = if form.current_field == CreateNodeField::DaemonMode {
        Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_COLOR)
    };
    let daemon_text = if form.daemon_mode { "Yes" } else { "No" };
    let daemon_field = Paragraph::new(daemon_text)
        .style(daemon_style)
        .block(Block::default().borders(Borders::ALL).title("Daemon Mode (↑↓ to toggle)"));
    f.render_widget(daemon_field, chunks[5]);
    
    // Submit button
    let submit_button = Paragraph::new("[ Press Enter to Join Cluster ]")
        .style(Style::default().fg(SUCCESS_COLOR).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(submit_button, chunks[6]);
}

fn render_confirm_dialog(f: &mut Frame, app: &App) {
    if let Some(dialog) = &app.confirm_dialog {
        let area = centered_rect(50, 10, f.size());
        f.render_widget(Clear, area);
        
        let block = Block::default()
            .title(dialog.title.as_str())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(WARNING_COLOR));
        
        f.render_widget(block, area);
        
        let inner_area = area.inner(&Margin { vertical: 1, horizontal: 1 });
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(inner_area);
        
        // Message
        let message = Paragraph::new(dialog.message.as_str())
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
        f.render_widget(message, chunks[0]);
        
        // Buttons
        let button_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(chunks[1]);
        
        let no_style = if !dialog.selected {
            Style::default().bg(WARNING_COLOR).fg(Color::Black).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(WARNING_COLOR)
        };
        
        let yes_style = if dialog.selected {
            Style::default().bg(ERROR_COLOR).fg(Color::Black).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(ERROR_COLOR)
        };
        
        let no_button = Paragraph::new("No")
            .style(no_style)
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center);
        
        let yes_button = Paragraph::new("Yes")
            .style(yes_style)
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center);
        
        f.render_widget(no_button, button_chunks[0]);
        f.render_widget(yes_button, button_chunks[1]);
    }
}

fn render_log_viewer(f: &mut Frame, _app: &App) {
    let area = centered_rect(90, 80, f.size());
    f.render_widget(Clear, area);
    
    let block = Block::default()
        .title("📋 Log Viewer")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY_COLOR));
    
    f.render_widget(block, area);
    
    // TODO: Implement log viewing
    let content = Paragraph::new("Log Viewer\n\nPress Esc to close")
        .block(Block::default().borders(Borders::NONE))
        .alignment(Alignment::Center);
    
    f.render_widget(content, area.inner(&Margin { vertical: 1, horizontal: 1 }));
}

// Helper functions

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Render search/filter overlay (lazygit-inspired)
fn render_search_overlay(f: &mut Frame, app: &App) {
    // Small search bar at the bottom
    let size = f.size();
    let search_area = Rect {
        x: size.width / 4,
        y: size.height - 4,
        width: size.width / 2,
        height: 3,
    };
    
    f.render_widget(Clear, search_area);
    
    let search_title = match app.search_mode {
        super::app::SearchMode::VmSearch => "🔍 Search VMs",
        super::app::SearchMode::NodeSearch => "🔍 Search Nodes", 
        super::app::SearchMode::QuickFilter => "🔍 Quick Filter",
        _ => "🔍 Search",
    };
    
    let search_text = format!("{}: {}", search_title, app.quick_filter);
    let search_widget = Paragraph::new(search_text)
        .style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD))
        .block(Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(WARNING_COLOR)))
        .alignment(Alignment::Left);
    
    f.render_widget(search_widget, search_area);
}

/// Render cluster discovery dialog
fn render_cluster_discovery(f: &mut Frame, app: &super::app::App) {
    let area = centered_rect(80, 24, f.size());
    f.render_widget(Clear, area);
    
    let block = Block::default()
        .title("🔍 Cluster Discovery")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY_COLOR));
    
    f.render_widget(block, area);
    
    let inner_area = area.inner(&Margin { vertical: 1, horizontal: 2 });
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Progress bar
            Constraint::Min(0),    // Cluster list
            Constraint::Length(3), // Instructions
        ])
        .split(inner_area);
    
    // Progress bar
    let progress = if app.cluster_discovery_active {
        app.cluster_scan_progress as u16
    } else {
        100
    };
    
    let progress_bar = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("Scanning Progress"))
        .gauge_style(Style::default().fg(PRIMARY_COLOR))
        .percent(progress)
        .label(format!("{}%", progress));
    
    f.render_widget(progress_bar, chunks[0]);
    
    // Cluster list
    let clusters: Vec<ListItem> = app.discovered_clusters
        .iter()
        .map(|cluster| {
            let status_icon = match cluster.health_status {
                super::app::ClusterHealthStatus::Healthy => "🟢",
                super::app::ClusterHealthStatus::Degraded => "🟡",
                super::app::ClusterHealthStatus::Critical => "🔴",
                super::app::ClusterHealthStatus::Unknown => "⚪",
            };
            
            let discovery_icon = if cluster.auto_discovered { "🔍" } else { "🏗️" };
            
            let content = format!("{} {} {} - {} ({} nodes) {}",
                status_icon,
                discovery_icon,
                cluster.name,
                cluster.leader_address,
                cluster.node_count,
                if cluster.accessible { "✓" } else { "✗" }
            );
            
            ListItem::new(content)
                .style(Style::default().fg(if cluster.accessible { TEXT_COLOR } else { Color::Gray }))
        })
        .collect();
    
    let cluster_list = List::new(clusters)
        .block(Block::default()
            .borders(Borders::ALL)
            .title(format!("Discovered Clusters ({})", app.discovered_clusters.len())))
        .style(Style::default().fg(TEXT_COLOR));
    
    f.render_widget(cluster_list, chunks[1]);
    
    // Instructions
    let instructions = if app.cluster_discovery_active {
        "Scanning for clusters... Please wait."
    } else {
        "Enter: Connect | R: Refresh | Esc: Close"
    };
    
    let instruction_widget = Paragraph::new(instructions)
        .style(Style::default().fg(INFO_COLOR))
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);
    
    f.render_widget(instruction_widget, chunks[2]);
}

/// Render cluster creation form
fn render_create_cluster_form(f: &mut Frame, app: &super::app::App) {
    let area = centered_rect(70, 20, f.size());
    f.render_widget(Clear, area);
    
    let block = Block::default()
        .title("🏗️ Create New Cluster")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY_COLOR));
    
    f.render_widget(block, area);
    
    let inner_area = area.inner(&Margin { vertical: 1, horizontal: 2 });
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Title
            Constraint::Min(0),    // Template list
            Constraint::Length(3), // Instructions
        ])
        .split(inner_area);
    
    // Title
    let title = Paragraph::new("Select a cluster template to create:")
        .style(Style::default().fg(TEXT_COLOR))
        .alignment(Alignment::Center);
    f.render_widget(title, chunks[0]);
    
    // Template list
    let templates: Vec<ListItem> = app.cluster_templates
        .iter()
        .enumerate()
        .map(|(i, template)| {
            let content = format!("{}. {} - {} ({} nodes)",
                i + 1,
                template.name,
                template.description,
                template.node_count
            );
            
            ListItem::new(vec![
                Line::from(content),
                Line::from(format!("   Port: {}, Backend: {}", 
                    template.network_config.base_port,
                    template.node_template.default_vm_backend
                )),
            ])
        })
        .collect();
    
    let template_list = List::new(templates)
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(TEXT_COLOR));
    
    f.render_widget(template_list, chunks[1]);
    
    // Instructions
    let instructions = "1-2: Select Template | Esc: Cancel";
    let instruction_widget = Paragraph::new(instructions)
        .style(Style::default().fg(SUCCESS_COLOR).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);
    
    f.render_widget(instruction_widget, chunks[2]);
}

fn render_raft_debug(f: &mut Frame, area: Rect, app: &App) {
    if let Some(ref raft_info) = app.raft_debug_info {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(10), // Raft state overview
                Constraint::Min(0),     // Peer states
            ])
            .split(area);
        
        render_raft_state_detail(f, chunks[0], raft_info);
        render_peer_states(f, chunks[1], raft_info);
    } else {
        let content = Paragraph::new("No Raft debug information available.\\n\\nPress 's' to simulate debug data.")
            .block(Block::default()
                .borders(Borders::ALL)
                .title("⚙️ Raft Debug")
                .border_style(Style::default().fg(PRIMARY_COLOR)))
            .alignment(Alignment::Center);
        
        f.render_widget(content, area);
    }
}

fn render_raft_state_detail(f: &mut Frame, area: Rect, raft_info: &RaftDebugInfo) {
    let content = vec![
        Line::from(vec![
            Span::styled("Raft Node State: ", Style::default().fg(TEXT_COLOR).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:?}", raft_info.state), get_raft_state_style(&raft_info.state)),
        ]),
        Line::from(vec![
            Span::styled("Current Term: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(raft_info.current_term.to_string(), Style::default().fg(PRIMARY_COLOR)),
            Span::styled(" | Voted For: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(
                raft_info.voted_for.map_or("None".to_string(), |id| id.to_string()),
                Style::default().fg(INFO_COLOR)
            ),
        ]),
        Line::from(vec![
            Span::styled("Log: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(format!("Length={}", raft_info.log_length), Style::default().fg(INFO_COLOR)),
            Span::styled(" | Commit=", Style::default().fg(TEXT_COLOR)),
            Span::styled(raft_info.commit_index.to_string(), Style::default().fg(SUCCESS_COLOR)),
            Span::styled(" | Applied=", Style::default().fg(TEXT_COLOR)),
            Span::styled(raft_info.last_applied.to_string(), Style::default().fg(SUCCESS_COLOR)),
        ]),
        Line::from(vec![
            Span::styled("Election Timeout: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(format!("{:.0}ms", raft_info.election_timeout.as_millis()), 
                       Style::default().fg(WARNING_COLOR)),
        ]),
    ];
    
    let content = if let Some(ref snapshot) = raft_info.snapshot_metadata {
        let mut lines = content;
        lines.push(Line::from(vec![
            Span::styled("Snapshot: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(format!("Index={} Term={} Size={}KB", 
                snapshot.last_included_index, 
                snapshot.last_included_term, 
                snapshot.size_bytes / 1024
            ), Style::default().fg(SECONDARY_COLOR)),
        ]));
        lines
    } else {
        content
    };
    
    let paragraph = Paragraph::new(content)
        .block(Block::default()
            .borders(Borders::ALL)
            .title("⚙️ Raft State Detail")
            .border_style(Style::default().fg(PRIMARY_COLOR)))
        .wrap(Wrap { trim: true });
    
    f.render_widget(paragraph, area);
}

fn render_peer_states(f: &mut Frame, area: Rect, raft_info: &RaftDebugInfo) {
    let header = Row::new(vec![
        Cell::from("Peer ID").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("Next Index").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("Match Index").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("State").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("Last Activity").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("Reachable").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
    ]);
    
    let rows: Vec<Row> = raft_info.peer_states.iter().map(|peer| {
        let reachable_style = if peer.is_reachable {
            Style::default().fg(SUCCESS_COLOR)
        } else {
            Style::default().fg(ERROR_COLOR)
        };
        
        let last_activity = peer.last_activity
            .map(|instant| {
                let elapsed = instant.elapsed();
                if elapsed.as_secs() < 60 {
                    format!("{:.1}s ago", elapsed.as_secs_f64())
                } else {
                    format!("{:.0}m ago", elapsed.as_secs() / 60)
                }
            })
            .unwrap_or_else(|| "Never".to_string());
        
        Row::new(vec![
            Cell::from(peer.id.to_string()),
            Cell::from(peer.next_index.to_string()),
            Cell::from(peer.match_index.to_string()),
            Cell::from(format!("{:?}", peer.state)),
            Cell::from(last_activity),
            Cell::from(if peer.is_reachable { "✓" } else { "✗" }).style(reachable_style),
        ])
    }).collect();
    
    let table = Table::new(rows, [
        Constraint::Min(8),   // Peer ID
        Constraint::Min(12),  // Next Index
        Constraint::Min(12),  // Match Index
        Constraint::Min(10),  // State
        Constraint::Min(15),  // Last Activity
        Constraint::Min(10),  // Reachable
    ])
    .header(header)
    .block(Block::default()
        .borders(Borders::ALL)
        .title("🔗 Peer States")
        .border_style(Style::default().fg(PRIMARY_COLOR)));
    
    f.render_widget(table, area);
}

fn render_debug_metrics(f: &mut Frame, area: Rect, app: &App) {
    let metrics = &app.debug_metrics;
    let uptime = metrics.last_reset.elapsed();
    
    let content = vec![
        Line::from(vec![
            Span::styled("Debug Metrics - Uptime: ", Style::default().fg(TEXT_COLOR).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:.1}s", uptime.as_secs_f64()), Style::default().fg(INFO_COLOR)),
        ]),
        Line::from(""),
        Line::from("📨 Message Statistics:"),
        Line::from(vec![
            Span::styled("  Sent: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(metrics.messages_sent.to_string(), Style::default().fg(SUCCESS_COLOR)),
            Span::styled(" | Received: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(metrics.messages_received.to_string(), Style::default().fg(SUCCESS_COLOR)),
        ]),
        Line::from(""),
        Line::from("📄 Proposal Statistics:"),
        Line::from(vec![
            Span::styled("  Submitted: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(metrics.proposals_submitted.to_string(), Style::default().fg(PRIMARY_COLOR)),
            Span::styled(" | Committed: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(metrics.proposals_committed.to_string(), Style::default().fg(SUCCESS_COLOR)),
        ]),
        Line::from(vec![
            Span::styled("  Success Rate: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(
                if metrics.proposals_submitted > 0 {
                    format!("{:.1}%", 
                        (metrics.proposals_committed as f64 / metrics.proposals_submitted as f64) * 100.0)
                } else {
                    "N/A".to_string()
                },
                Style::default().fg(if metrics.proposals_committed == metrics.proposals_submitted {
                    SUCCESS_COLOR
                } else {
                    WARNING_COLOR
                })
            ),
        ]),
        Line::from(""),
        Line::from("🗳️ Election Statistics:"),
        Line::from(vec![
            Span::styled("  Elections Started: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(metrics.elections_started.to_string(), Style::default().fg(WARNING_COLOR)),
            Span::styled(" | Leadership Changes: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(metrics.leadership_changes.to_string(), Style::default().fg(WARNING_COLOR)),
        ]),
        Line::from(""),
        Line::from("💾 Storage Statistics:"),
        Line::from(vec![
            Span::styled("  Snapshots Created: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(metrics.snapshot_creations.to_string(), Style::default().fg(SECONDARY_COLOR)),
            Span::styled(" | Log Compactions: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(metrics.log_compactions.to_string(), Style::default().fg(SECONDARY_COLOR)),
        ]),
        Line::from(""),
        Line::from("🌐 Network Statistics:"),
        Line::from(vec![
            Span::styled("  Partitions Detected: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(metrics.network_partitions_detected.to_string(), 
                       if metrics.network_partitions_detected > 0 { ERROR_COLOR } else { SUCCESS_COLOR }),
        ]),
    ];
    
    let paragraph = Paragraph::new(content)
        .block(Block::default()
            .borders(Borders::ALL)
            .title("📈 Debug Metrics Detail")
            .border_style(Style::default().fg(PRIMARY_COLOR)))
        .wrap(Wrap { trim: true });
    
    f.render_widget(paragraph, area);
}

fn render_debug_logs(f: &mut Frame, area: Rect, app: &App) {
    let logs: Vec<ListItem> = app.debug_log_entries
        .iter()
        .take(20) // Show last 20 log entries
        .map(|entry| {
            let elapsed = entry.timestamp.elapsed();
            let time_str = if elapsed.as_secs() < 60 {
                format!("{:.1}s", elapsed.as_secs_f64())
            } else {
                format!("{:.0}m", elapsed.as_secs() / 60)
            };
            
            let level_icon = match entry.level {
                DebugLevel::Trace => "🔍",
                DebugLevel::Debug => "🐛",
                DebugLevel::Info => "ℹ️",
                DebugLevel::Warn => "⚠️",
                DebugLevel::Error => "❌",
            };
            
            let level_color = match entry.level {
                DebugLevel::Trace => Color::Gray,
                DebugLevel::Debug => Color::Cyan,
                DebugLevel::Info => Color::Blue,
                DebugLevel::Warn => Color::Yellow,
                DebugLevel::Error => Color::Red,
            };
            
            let content = format!("{} [{}] {} [{:?}]: {}", 
                level_icon, time_str, entry.component, entry.level, entry.message
            );
            
            ListItem::new(content)
                .style(Style::default().fg(level_color))
        })
        .collect();
    
    let logs_list = List::new(logs)
        .block(Block::default()
            .borders(Borders::ALL)
            .title(format!("📜 Debug Logs ({} entries)", app.debug_log_entries.len()))
            .border_style(Style::default().fg(PRIMARY_COLOR)))
        .style(Style::default().fg(TEXT_COLOR));
    
    f.render_widget(logs_list, area);
}

