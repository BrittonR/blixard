use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, List, ListItem, Paragraph, Wrap,
    },
    Frame,
};

use super::app::{App, AppMode, CreateVmForm, InputMode, VmInfo};
use blixard_core::types::VmStatus;

pub fn render(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // Header (expanded for cluster info)
            Constraint::Min(8),     // Main content
            Constraint::Length(8),  // Live log panel
            Constraint::Length(3),  // Footer
        ])
        .split(f.size());

    render_header(f, chunks[0], app);
    
    match app.mode {
        AppMode::VmList => render_vm_list(f, chunks[1], app),
        AppMode::VmDetails => render_vm_details(f, chunks[1], app),
        AppMode::VmCreate => render_vm_create(f, chunks[1], app),
        AppMode::VmLogs => render_vm_logs(f, chunks[1], app),
        AppMode::Help => render_help(f, chunks[1]),
        AppMode::SshSession => render_ssh_session(f, chunks[1], app),
        AppMode::RaftStatus => render_raft_status(f, chunks[1], app),
    }
    
    render_live_log_panel(f, chunks[2], app);
    render_footer(f, chunks[3], app);
}

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    // Split header vertically for two rows
    let header_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title and main status row
            Constraint::Length(2), // Cluster status row
        ])
        .split(area);

    // Top row: Split horizontally for title and status
    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(25),     // Title area
            Constraint::Length(50),  // Status/error messages
        ])
        .split(header_rows[0]);

    // Main title
    let title = Paragraph::new("🚀 Blixard VM Manager")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Blue)));
    
    f.render_widget(title, top_chunks[0]);

    // Status/error messages in top right
    if let Some(msg) = &app.status_message {
        let status = Paragraph::new(format!("✓ {}", msg))
            .style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Green)));
        f.render_widget(status, top_chunks[1]);
    } else if let Some(msg) = &app.error_message {
        let error = Paragraph::new(format!("✗ {}", msg))
            .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Red)));
        f.render_widget(error, top_chunks[1]);
    } else {
        // Show live log following status when no other messages
        let log_status = match &app.live_log_vm {
            Some(vm_name) => format!("📡 Following: {}", vm_name),
            None => "📡 Following: All VMs".to_string(),
        };
        let status = Paragraph::new(log_status)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Gray)));
        f.render_widget(status, top_chunks[1]);
    }

    // Bottom row: Split horizontally for cluster info
    let bottom_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33), // Node info
            Constraint::Percentage(33), // Cluster info 
            Constraint::Percentage(34), // Raft status
        ])
        .split(header_rows[1]);

    // Node information
    let node_info = format!("Node {}: {}", 
        app.cluster_info.current_node_id, 
        app.cluster_info.current_node_state
    );
    let node_color = match app.cluster_info.current_node_state.as_str() {
        "Leader" => Color::Green,
        "Follower" => Color::Blue,
        "Candidate" => Color::Yellow,
        _ => Color::Gray,
    };
    let node_widget = Paragraph::new(node_info)
        .style(Style::default().fg(node_color))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(node_color)));
    f.render_widget(node_widget, bottom_chunks[0]);

    // Cluster information  
    let cluster_info = format!("Cluster: {} nodes", app.cluster_info.node_count);
    let cluster_color = if app.cluster_info.node_count > 0 { Color::Green } else { Color::Red };
    let cluster_widget = Paragraph::new(cluster_info)
        .style(Style::default().fg(cluster_color))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(cluster_color)));
    f.render_widget(cluster_widget, bottom_chunks[1]);

    // Raft status
    let raft_info = if app.cluster_info.leader_id > 0 {
        format!("Leader: {} | Term: {}", 
            app.cluster_info.leader_id,
            app.cluster_info.term
        )
    } else {
        format!("No Leader | Term: {} | Press 'R' for details", app.cluster_info.term)
    };
    let raft_color = if app.cluster_info.leader_id > 0 { Color::Green } else { Color::Red };
    let raft_widget = Paragraph::new(raft_info)
        .style(Style::default().fg(raft_color))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(raft_color)));
    f.render_widget(raft_widget, bottom_chunks[2]);
}

fn render_footer(f: &mut Frame, area: Rect, app: &App) {
    let key_hints = match app.mode {
        AppMode::VmList => {
            vec![
                ("↑/↓", "Navigate"),
                ("Enter", "Details"),
                ("n", "New VM"),
                ("s", "Start"),
                ("x", "Stop"),
                ("c", "SSH"),
                ("l", "Logs"),
                ("f", "Follow VM"),
                ("F", "Follow All"),
                ("r", "Refresh"),
                ("R", "Raft Status"),
                ("?", "Help"),
                ("q", "Quit"),
            ]
        }
        AppMode::VmDetails => {
            vec![
                ("s", "Start"),
                ("x", "Stop"),
                ("c", "SSH"),
                ("l", "Logs"),
                ("Esc", "Back"),
                ("q", "Quit"),
            ]
        }
        AppMode::VmCreate => {
            vec![
                ("Tab", "Next Field"),
                ("Enter", "Submit"),
                ("Esc", "Cancel"),
            ]
        }
        AppMode::VmLogs => {
            vec![
                ("↑/↓", "Scroll"),
                ("PgUp/PgDn", "Page"),
                ("Home/End", "Top/Bottom"),
                ("Esc", "Back"),
            ]
        }
        AppMode::Help => {
            vec![("Any Key", "Back")]
        }
        AppMode::SshSession => {
            vec![
                ("Type", "Input"),
                ("Enter", "Send"),
                ("Ctrl+C", "Interrupt"),
                ("Ctrl+D", "EOF"),
                ("Esc", "Exit"),
            ]
        }
        AppMode::RaftStatus => {
            vec![
                ("r", "Refresh"),
                ("Any Key", "Back"),
            ]
        }
    };

    let spans: Vec<Span> = key_hints
        .iter()
        .flat_map(|(key, desc)| {
            vec![
                Span::styled(*key, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": "),
                Span::raw(*desc),
                Span::raw(" | "),
            ]
        })
        .collect();

    let footer_text = Line::from(spans);
    let footer = Paragraph::new(footer_text)
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);

    f.render_widget(footer, area);
}


fn render_vm_list(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(100)])
        .split(area);

    let vm_items: Vec<ListItem> = app
        .vms
        .iter()
        .map(|vm| {
            let status_color = match vm.status {
                VmStatus::Creating => Color::Yellow,
                VmStatus::Running => Color::Green,
                VmStatus::Starting => Color::Yellow,
                VmStatus::Stopping => Color::Yellow,
                VmStatus::Stopped => Color::Gray,
                VmStatus::Failed => Color::Red,
            };

            let status_indicator = match vm.status {
                VmStatus::Creating => "◔",
                VmStatus::Running => "●",
                VmStatus::Starting => "◐",
                VmStatus::Stopping => "◑",
                VmStatus::Stopped => "○",
                VmStatus::Failed => "✗",
            };

            let content = Line::from(vec![
                Span::styled(status_indicator, Style::default().fg(status_color)),
                Span::raw(" "),
                Span::styled(&vm.name, Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!(" ({}vcpu, {}MB)", vm.vcpus, vm.memory)),
                Span::raw(" on node "),
                Span::styled(vm.node_id.to_string(), Style::default().fg(Color::Cyan)),
            ]);

            ListItem::new(content)
        })
        .collect();

    let vm_list = List::new(vm_items)
        .block(
            Block::default()
                .title("VMs")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        )
        .highlight_style(Style::default().bg(Color::Blue).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");

    f.render_stateful_widget(vm_list, chunks[0], &mut app.vm_list_state.clone());

    // Show VM count in the title
    let title = format!("VMs ({})", app.vms.len());
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));
    f.render_widget(block, chunks[0]);
}

fn render_vm_details(f: &mut Frame, area: Rect, app: &App) {
    if let Some(vm_index) = app.selected_vm {
        if let Some(vm) = app.vms.get(vm_index) {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(8),  // VM details
                    Constraint::Min(0),     // Resource usage (future)
                ])
                .split(area);

            render_vm_info(f, chunks[0], vm);
            render_vm_metrics(f, chunks[1], vm, &app.vm_process_info);
        }
    } else {
        let no_selection = Paragraph::new("No VM selected")
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center)
            .block(Block::default().title("VM Details").borders(Borders::ALL));
        f.render_widget(no_selection, area);
    }
}

fn render_vm_info(f: &mut Frame, area: Rect, vm: &VmInfo) {
    let status_color = match vm.status {
        VmStatus::Creating => Color::Yellow,
        VmStatus::Running => Color::Green,
        VmStatus::Starting => Color::Yellow,
        VmStatus::Stopping => Color::Yellow,
        VmStatus::Stopped => Color::Gray,
        VmStatus::Failed => Color::Red,
    };

    let info_text = vec![
        Line::from(vec![
            Span::raw("Name: "),
            Span::styled(&vm.name, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::raw("Status: "),
            Span::styled(format!("{:?}", vm.status), Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::raw("vCPUs: "),
            Span::styled(vm.vcpus.to_string(), Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::raw("Memory: "),
            Span::styled(format!("{} MB", vm.memory), Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::raw("Node ID: "),
            Span::styled(vm.node_id.to_string(), Style::default().fg(Color::Cyan)),
        ]),
    ];

    let info_paragraph = Paragraph::new(info_text)
        .block(Block::default().title("VM Information").borders(Borders::ALL))
        .wrap(Wrap { trim: true });

    f.render_widget(info_paragraph, area);
}

fn render_vm_metrics(f: &mut Frame, area: Rect, vm: &VmInfo, process_info: &Option<super::app::VmProcessInfo>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // System info
            Constraint::Length(3), // CPU usage
            Constraint::Length(3), // Memory usage
            Constraint::Min(0),    // Additional metrics
        ])
        .split(area);

    // System information
    let (service_status, uptime, pid_info) = if let Some(info) = process_info {
        (
            info.service_status.clone(),
            info.uptime.clone().unwrap_or_else(|| "Unknown".to_string()),
            info.pid.map(|p| format!("PID: {}", p)).unwrap_or_else(|| "No PID".to_string()),
        )
    } else {
        ("Unknown".to_string(), "Unknown".to_string(), "Loading...".to_string())
    };

    let system_info = vec![
        Line::from(vec![
            Span::raw("DB Status: "),
            Span::styled(
                format!("{:?}", vm.status),
                Style::default().fg(match vm.status {
                    VmStatus::Running => Color::Green,
                    VmStatus::Stopped => Color::Gray,
                    VmStatus::Failed => Color::Red,
                    VmStatus::Starting | VmStatus::Stopping => Color::Yellow,
                    _ => Color::Gray,
                })
            ),
            Span::raw(" | Service: "),
            Span::styled(
                service_status, 
                Style::default().fg(if process_info.as_ref().map(|i| &i.service_status) == Some(&"Running".to_string()) {
                    Color::Green
                } else {
                    Color::Red
                })
            ),
        ]),
        Line::from(vec![
            Span::raw("Uptime: "),
            Span::styled(uptime, Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::raw("Process: "),
            Span::styled(pid_info, Style::default().fg(Color::Cyan)),
        ]),
    ];

    let system_paragraph = Paragraph::new(system_info)
        .block(Block::default().title("System Info").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    f.render_widget(system_paragraph, chunks[0]);

    // CPU Usage
    let cpu_usage = process_info.as_ref().and_then(|i| i.cpu_usage).unwrap_or(0.0);
    let cpu_color = if cpu_usage > 80.0 {
        Color::Red
    } else if cpu_usage > 60.0 {
        Color::Yellow
    } else {
        Color::Green
    };

    let cpu_text = if process_info.is_some() {
        format!("CPU: {:.1}%", cpu_usage)
    } else {
        "CPU: Loading...".to_string()
    };
    let cpu_paragraph = Paragraph::new(cpu_text)
        .style(Style::default().fg(cpu_color))
        .block(Block::default().title("CPU Usage").borders(Borders::ALL))
        .alignment(Alignment::Center);
    f.render_widget(cpu_paragraph, chunks[1]);

    // Memory Usage
    let memory_mb = process_info.as_ref().and_then(|i| i.memory_usage).unwrap_or(0.0);
    let memory_color = if memory_mb > 512.0 {
        Color::Red
    } else if memory_mb > 256.0 {
        Color::Yellow
    } else {
        Color::Green
    };

    let memory_text = if process_info.is_some() {
        format!("Memory: {:.1} MB", memory_mb)
    } else {
        "Memory: Loading...".to_string()
    };
    let memory_paragraph = Paragraph::new(memory_text)
        .style(Style::default().fg(memory_color))
        .block(Block::default().title("Memory Usage").borders(Borders::ALL))
        .alignment(Alignment::Center);
    f.render_widget(memory_paragraph, chunks[2]);

    // Additional info
    let additional_info = if let Some(info) = process_info {
        // Check if DB status matches actual service status
        let db_running = matches!(vm.status, VmStatus::Running);
        let service_running = info.service_status == "Running";
        let sync_status = if db_running == service_running {
            if db_running {
                "✅ Synchronized (Both Running)"
            } else {
                "✅ Synchronized (Both Stopped)"
            }
        } else if db_running && !service_running {
            "⚠️  DB says Running but Service Stopped"
        } else {
            "⚠️  DB says Stopped but Service Running"
        };

        vec![
            Line::from(format!("🔧 Systemd service: blixard-vm-{}", vm.name)),
            Line::from(format!("🔄 State sync: {}", sync_status)),
            Line::from(format!("📊 Real-time monitoring: {}", 
                if info.pid.is_some() { "Active" } else { "Inactive" })),
            Line::from("💡 Press 'l' for logs, 's'/'x' to start/stop"),
        ]
    } else {
        vec![
            Line::from("🔧 Loading process information..."),
            Line::from("📊 Real-time metrics will appear here"),
            Line::from("💡 Press 'l' to view VM logs"),
        ]
    };

    let info_paragraph = Paragraph::new(additional_info)
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().title("Info").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    f.render_widget(info_paragraph, chunks[3]);
}

fn render_vm_create(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10), // Form
            Constraint::Min(0),     // Instructions
        ])
        .split(area);

    render_create_form(f, chunks[0], &app.create_form, app.input_mode == InputMode::Editing);
    render_create_instructions(f, chunks[1]);
}

fn render_create_form(f: &mut Frame, area: Rect, form: &CreateVmForm, editing: bool) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Name
            Constraint::Length(3), // vCPUs
            Constraint::Length(3), // Memory
        ])
        .split(area.inner(&Margin { vertical: 1, horizontal: 1 }));

    let fields = [
        ("Name", &form.name, 0),
        ("vCPUs", &form.vcpus, 1),
        ("Memory (MB)", &form.memory, 2),
    ];

    for (i, (label, value, field_index)) in fields.iter().enumerate() {
        let is_active = editing && form.current_field == *field_index;
        let border_style = if is_active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Blue)
        };

        let input = Paragraph::new(value.as_str())
            .style(Style::default().fg(Color::White))
            .block(
                Block::default()
                    .title(*label)
                    .borders(Borders::ALL)
                    .border_style(border_style),
            );

        f.render_widget(input, chunks[i]);

        // Show cursor for active field
        if is_active {
            let cursor_x = chunks[i].x + value.len() as u16 + 1;
            let cursor_y = chunks[i].y + 1;
            f.set_cursor(cursor_x.min(chunks[i].x + chunks[i].width - 2), cursor_y);
        }
    }

    let form_block = Block::default()
        .title("Create New VM")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    f.render_widget(form_block, area);
}

fn render_create_instructions(f: &mut Frame, area: Rect) {
    let instructions = vec![
        Line::from("Use Tab/Shift+Tab to navigate between fields"),
        Line::from("Press Enter to move to next field or submit"),
        Line::from("Press Esc to cancel and return to VM list"),
    ];

    let instructions_paragraph = Paragraph::new(instructions)
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().title("Instructions").borders(Borders::ALL))
        .wrap(Wrap { trim: true });

    f.render_widget(instructions_paragraph, area);
}

fn render_vm_logs(f: &mut Frame, area: Rect, app: &App) {
    let log_lines: Vec<Line> = app
        .vm_logs
        .iter()
        .skip(app.log_scroll as usize)
        .map(|line| Line::from(line.as_str()))
        .collect();

    // Show follow status in title
    let title = if app.log_receiver.is_some() {
        "VM Logs 🔄 Following Live"
    } else {
        "VM Logs (Static)"
    };

    let title_style = if app.log_receiver.is_some() {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Blue)
    };

    let logs = Paragraph::new(log_lines)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(title_style),
        )
        .wrap(Wrap { trim: false })
        .scroll((app.log_scroll, 0));

    f.render_widget(logs, area);
}

fn render_live_log_panel(f: &mut Frame, area: Rect, app: &App) {
    // Get the last few lines to fit in the panel
    let visible_lines = area.height.saturating_sub(2) as usize; // Account for borders
    let log_lines: Vec<Line> = app
        .live_logs
        .iter()
        .rev() // Show most recent logs at bottom
        .take(visible_lines)
        .rev() // Reverse again to get correct order
        .map(|line| {
            // Color-code log lines based on content
            let style = if line.contains("error") || line.contains("ERROR") || line.contains("Failed") {
                Style::default().fg(Color::Red)
            } else if line.contains("warn") || line.contains("WARN") {
                Style::default().fg(Color::Yellow)
            } else if line.contains("info") || line.contains("INFO") || line.contains("Started") {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::White)
            };
            Line::from(Span::styled(line.clone(), style))
        })
        .collect();

    // Determine panel title based on what we're following
    let title = match &app.live_log_vm {
        Some(vm_name) => format!("🔄 Live Logs: {}", vm_name),
        None => "🔄 Live Logs: All VMs".to_string(),
    };

    let panel_style = if app.live_log_receiver.is_some() {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Gray)
    };

    let live_logs = Paragraph::new(log_lines)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(panel_style),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(live_logs, area);
}

fn render_help(f: &mut Frame, area: Rect) {
    let help_text = vec![
        Line::from("🚀 Blixard VM Manager - Help").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Line::from(""),
        Line::from("VM List View:").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Line::from("  ↑/↓ or j/k     - Navigate VM list"),
        Line::from("  Enter          - View VM details"),
        Line::from("  n              - Create new VM"),
        Line::from("  s              - Start selected VM"),
        Line::from("  x              - Stop selected VM"),
        Line::from("  c              - SSH connect to selected VM"),
        Line::from("  l              - View VM logs"),
        Line::from("  f              - Follow selected VM in live panel"),
        Line::from("  F              - Follow all VMs in live panel"),
        Line::from("  r              - Refresh VM list"),
        Line::from("  R              - View detailed Raft status"),
        Line::from(""),
        Line::from("VM Details View:").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Line::from("  s              - Start VM"),
        Line::from("  x              - Stop VM"),
        Line::from("  c              - SSH connect to VM"),
        Line::from("  l              - View VM logs"),
        Line::from(""),
        Line::from("VM Creation:").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Line::from("  Tab/Shift+Tab  - Navigate fields"),
        Line::from("  Enter          - Next field/Submit"),
        Line::from(""),
        Line::from("Log Viewer:").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Line::from("  ↑/↓ or j/k     - Scroll logs"),
        Line::from("  Page Up/Down   - Scroll by page"),
        Line::from("  Home/End       - Go to top/bottom"),
        Line::from("  🔄 Follows live logs by default"),
        Line::from(""),
        Line::from("Live Log Panel:").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Line::from("  🔄 Always visible at bottom"),
        Line::from("  Shows real-time logs from VMs"),
        Line::from("  Color-coded: red=errors, yellow=warnings, green=info"),
        Line::from(""),
        Line::from("Cluster Status:").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Line::from("  Header shows real-time node and cluster information"),
        Line::from("  Node role: Leader (green), Follower (blue), Candidate (yellow)"),
        Line::from("  Cluster size and Raft term displayed"),
        Line::from("  Auto-refreshes every 7.5 seconds"),
        Line::from(""),
        Line::from("Raft Status View:").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Line::from("  Shows detailed Raft consensus state"),
        Line::from("  Displays all cluster nodes and their roles"),
        Line::from("  Real-time term, leader, and node status"),
        Line::from("  Press 'r' to refresh, any key to exit"),
        Line::from(""),
        Line::from("SSH Connection:").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Line::from("  Press 'c' on running VM to open embedded SSH session"),
        Line::from("  Uses SSH key authentication for seamless access"),
        Line::from("  Interactive terminal with real-time I/O"),
        Line::from("  Supports Ctrl+C, Ctrl+D, and other terminal controls"),
        Line::from(""),
        Line::from("Global:").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Line::from("  Esc            - Go back/Cancel"),
        Line::from("  Ctrl+C or q    - Quit application"),
        Line::from("  ?              - Show this help"),
        Line::from("  Mouse          - Select and copy text (enabled)"),
        Line::from(""),
        Line::from("Press any key to return to VM list").style(Style::default().fg(Color::Green)),
    ];

    let help = Paragraph::new(help_text)
        .block(Block::default().title("Help").borders(Borders::ALL))
        .wrap(Wrap { trim: true });

    f.render_widget(help, area);
}

fn render_raft_status(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),  // Raft Overview
            Constraint::Min(8),     // Node Details
        ])
        .split(area);

    // Raft Overview
    let raft_overview = vec![
        Line::from(vec![
            Span::styled("🗳️ Raft Cluster Status", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Current Term: "),
            Span::styled(app.cluster_info.term.to_string(), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(" | Leader: "),
            Span::styled(
                if app.cluster_info.leader_id > 0 { 
                    format!("Node {}", app.cluster_info.leader_id) 
                } else { 
                    "No Leader".to_string() 
                },
                Style::default().fg(if app.cluster_info.leader_id > 0 { Color::Green } else { Color::Red }).add_modifier(Modifier::BOLD)
            ),
        ]),
        Line::from(vec![
            Span::raw("Cluster Size: "),
            Span::styled(
                format!("{} nodes", app.cluster_info.node_count),
                Style::default().fg(if app.cluster_info.node_count > 0 { Color::Green } else { Color::Red }).add_modifier(Modifier::BOLD)
            ),
            Span::raw(" | Current Node: "),
            Span::styled(
                format!("Node {} ({})", app.cluster_info.current_node_id, app.cluster_info.current_node_state),
                Style::default().fg(match app.cluster_info.current_node_state.as_str() {
                    "Leader" => Color::Green,
                    "Follower" => Color::Blue,
                    "Candidate" => Color::Yellow,
                    _ => Color::Gray,
                }).add_modifier(Modifier::BOLD)
            ),
        ]),
    ];

    let overview_widget = Paragraph::new(raft_overview)
        .block(
            Block::default()
                .title("Raft Overview")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue))
        );
    f.render_widget(overview_widget, chunks[0]);

    // Node Details
    let node_lines: Vec<Line> = if app.cluster_info.nodes.is_empty() {
        vec![
            Line::from("No node information available"),
            Line::from(""),
            Line::from("This could indicate:"),
            Line::from("• The cluster is not initialized"),
            Line::from("• Connection to the cluster failed"),
            Line::from("• The node is not yet part of a cluster"),
        ]
    } else {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("Node Details:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
        ];
        
        for node in &app.cluster_info.nodes {
            let state_color = match node.state.as_str() {
                "Leader" => Color::Green,
                "Follower" => Color::Blue,
                "Candidate" => Color::Yellow,
                _ => Color::Gray,
            };

            let prefix = if node.id == app.cluster_info.current_node_id { "▶ " } else { "  " };
            
            lines.push(Line::from(vec![
                Span::raw(prefix),
                Span::styled(format!("Node {}", node.id), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(" @ "),
                Span::styled(&node.address, Style::default().fg(Color::White)),
                Span::raw(" - "),
                Span::styled(&node.state, Style::default().fg(state_color).add_modifier(Modifier::BOLD)),
                if node.id == app.cluster_info.leader_id {
                    Span::styled(" 👑", Style::default().fg(Color::Yellow))
                } else {
                    Span::raw("")
                },
            ]));
        }
        
        lines.push(Line::from(""));
        lines.push(Line::from("Legend: ▶ Current Node | 👑 Leader"));
        lines.push(Line::from("Press 'r' to refresh, any other key to go back"));
        
        lines
    };

    let nodes_widget = Paragraph::new(node_lines)
        .block(
            Block::default()
                .title("Cluster Nodes")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green))
        )
        .wrap(Wrap { trim: true });
    f.render_widget(nodes_widget, chunks[1]);
}

fn render_ssh_session(f: &mut Frame, area: Rect, app: &App) {
    if let Some(session) = &app.ssh_session {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),  // Connection info header
                Constraint::Min(5),     // Terminal output
                Constraint::Length(3),  // Input line
            ])
            .split(area);

        // Connection info header
        let connection_info = vec![
            Line::from(vec![
                Span::styled("🔌 SSH Session: ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled(&session.vm_name, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(" | "),
                Span::styled(format!("{}@{}:{}", session.username, session.host, session.port), Style::default().fg(Color::Green)),
            ]),
            Line::from(vec![
                Span::raw("Status: "),
                Span::styled(&session.connection_status, 
                    if session.is_connected { 
                        Style::default().fg(Color::Green) 
                    } else { 
                        Style::default().fg(Color::Yellow) 
                    }
                ),
            ]),
        ];

        let info_widget = Paragraph::new(connection_info)
            .block(
                Block::default()
                    .title("SSH Connection")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Blue))
            );
        f.render_widget(info_widget, chunks[0]);

        // Terminal output area
        let output_lines: Vec<Line> = session
            .output_lines
            .iter()
            .map(|line| {
                // Color-code output based on content
                let style = if line.starts_with("stderr:") || line.contains("error") || line.contains("ERROR") {
                    Style::default().fg(Color::Red)
                } else if line.starts_with("$ ") {
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                } else if line.starts_with("✓") {
                    Style::default().fg(Color::Green)
                } else if line.starts_with("✗") || line.starts_with("Failed") {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(Span::styled(line.clone(), style))
            })
            .collect();

        let terminal_widget = Paragraph::new(output_lines)
            .block(
                Block::default()
                    .title("Terminal Output")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Green))
            )
            .wrap(Wrap { trim: false })
            .scroll((
                (session.output_lines.len().saturating_sub(chunks[1].height.saturating_sub(2) as usize)) as u16,
                0
            ));
        f.render_widget(terminal_widget, chunks[1]);

        // Input line
        let input_text = vec![
            Line::from(vec![
                Span::raw("$ "),
                Span::styled(&session.input_buffer, Style::default().fg(Color::White)),
                Span::styled("█", Style::default().fg(Color::White)), // Cursor
            ]),
        ];

        let input_widget = Paragraph::new(input_text)
            .block(
                Block::default()
                    .title("Input (Enter to send, Ctrl+C to interrupt, Esc to exit)")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow))
            );
        f.render_widget(input_widget, chunks[2]);
    } else {
        // No SSH session active
        let no_session = Paragraph::new("No SSH session active")
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center)
            .block(Block::default().title("SSH Session").borders(Borders::ALL));
        f.render_widget(no_session, area);
    }
}

/// Helper function to create a centered rectangle
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