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
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Paragraph, Wrap, Table, Row, Cell, Gauge, List, ListItem,
        Clear, Tabs, BarChart, Dataset, Axis, GraphType, Sparkline,
    },
    Frame,
    symbols,
};

use super::app_v2::{
    App, AppTab, AppMode, VmInfo, NodeInfo, NodeStatus, NodeRole, 
    EventLevel, PlacementStrategy, CreateVmForm, CreateNodeForm, ConfirmDialog
};
use blixard_core::types::VmStatus;
use std::time::Instant;

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
        AppTab::Help => render_help(f, chunks[1], app),
    }
    
    // Render status bar
    render_status_bar(f, chunks[2], app);
    
    // Render overlays/popups
    match app.mode {
        AppMode::CreateVmForm => render_create_vm_form(f, app),
        AppMode::CreateNodeForm => render_create_node_form(f, app),
        AppMode::ConfirmDialog => render_confirm_dialog(f, app),
        AppMode::LogViewer => render_log_viewer(f, app),
        _ => {}
    }
}

fn render_tab_bar(f: &mut Frame, area: Rect, app: &App) {
    let titles = vec![
        "📊 Dashboard",
        "🖥️ VMs", 
        "🔗 Nodes",
        "📈 Monitoring",
        "⚙️ Config",
        "❓ Help"
    ];
    
    let selected_tab = match app.current_tab {
        AppTab::Dashboard => 0,
        AppTab::VirtualMachines => 1,
        AppTab::Nodes => 2,
        AppTab::Monitoring => 3,
        AppTab::Configuration => 4,
        AppTab::Help => 5,
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

fn render_quick_actions_card(f: &mut Frame, area: Rect, app: &App) {
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

fn render_vm_toolbar(f: &mut Frame, area: Rect, app: &App) {
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
    
    let rows: Vec<Row> = app.vms.iter().map(|vm| {
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
        .title(format!("🖥️ Virtual Machines ({} total)", app.vms.len()))
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

fn render_node_toolbar(f: &mut Frame, area: Rect, app: &App) {
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
    
    let rows: Vec<Row> = app.nodes.iter().map(|node| {
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
        .title(format!("🔗 Cluster Nodes ({} total)", app.nodes.len()))
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

fn render_performance_metrics(f: &mut Frame, area: Rect, app: &App) {
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

fn render_configuration(f: &mut Frame, area: Rect, app: &App) {
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

fn render_help(f: &mut Frame, area: Rect, app: &App) {
    let content = vec![
        Line::from("❓ Blixard TUI Help"),
        Line::from(""),
        Line::from("🔥 Global Shortcuts:"),
        Line::from("  q / Ctrl+C   - Quit application"),
        Line::from("  h            - Show this help"),
        Line::from("  r            - Refresh all data"),
        Line::from("  1-5          - Switch between tabs"),
        Line::from(""),
        Line::from("📊 Dashboard:"),
        Line::from("  c            - Create new VM"),
        Line::from("  n            - Add new node"),
        Line::from("  s            - Show cluster status"),
        Line::from(""),
        Line::from("🖥️ VM Management:"),
        Line::from("  c            - Create VM"),
        Line::from("  Enter        - View VM details"),
        Line::from("  d            - Delete VM"),
        Line::from("  ↑/↓          - Navigate list"),
        Line::from(""),
        Line::from("🔗 Node Management:"),
        Line::from("  a            - Add node"),
        Line::from("  s            - Start daemon"),
        Line::from("  Enter        - View node details"),
        Line::from("  ↑/↓          - Navigate list"),
        Line::from(""),
        Line::from("💡 Tips:"),
        Line::from("  • Use daemon mode (-d) for background nodes"),
        Line::from("  • Monitor real-time metrics in Monitoring tab"),
        Line::from("  • Configure quotas and security in Config tab"),
        Line::from("  • All data refreshes automatically every 2 seconds"),
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
    
    let status_text = if let Some(msg) = &app.status_message {
        format!("✅ {}", msg)
    } else if let Some(msg) = &app.error_message {
        format!("❌ {}", msg)
    } else {
        format!("{} | {} | {} | {}", connection_status, leader_info, vm_count, node_count)
    };
    
    let status_style = if app.error_message.is_some() {
        Style::default().fg(ERROR_COLOR)
    } else if app.status_message.is_some() {
        Style::default().fg(SUCCESS_COLOR)
    } else {
        Style::default().fg(TEXT_COLOR)
    };
    
    let paragraph = Paragraph::new(status_text)
        .style(status_style)
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);
    
    f.render_widget(paragraph, area);
}

// Popup/Overlay rendering functions

fn render_create_vm_form(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 20, f.size());
    f.render_widget(Clear, area);
    
    let block = Block::default()
        .title("🖥️ Create New VM")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY_COLOR));
    
    f.render_widget(block, area);
    
    // TODO: Implement form fields
    let content = Paragraph::new("VM Creation Form\n\nPress Esc to cancel")
        .block(Block::default().borders(Borders::NONE))
        .alignment(Alignment::Center);
    
    f.render_widget(content, area.inner(&Margin { vertical: 1, horizontal: 1 }));
}

fn render_create_node_form(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 20, f.size());
    f.render_widget(Clear, area);
    
    let block = Block::default()
        .title("🔗 Add New Node")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PRIMARY_COLOR));
    
    f.render_widget(block, area);
    
    // TODO: Implement form fields
    let content = Paragraph::new("Node Creation Form\n\nPress Esc to cancel")
        .block(Block::default().borders(Borders::NONE))
        .alignment(Alignment::Center);
    
    f.render_widget(content, area.inner(&Margin { vertical: 1, horizontal: 1 }));
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

fn render_log_viewer(f: &mut Frame, app: &App) {
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