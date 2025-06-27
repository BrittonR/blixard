//! P2P networking view for the TUI
//! 
//! Shows Iroh P2P status, connected peers, and shared resources

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Table, Row, Cell},
    Frame,
};
use super::app::App;

// Color constants
const PRIMARY_COLOR: Color = Color::Cyan;
const SUCCESS_COLOR: Color = Color::Green;
const WARNING_COLOR: Color = Color::Yellow;
const ERROR_COLOR: Color = Color::Red;
const INFO_COLOR: Color = Color::Magenta;
const TEXT_COLOR: Color = Color::White;

/// Render the P2P networking view
pub fn render_p2p_view(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),   // P2P status bar
            Constraint::Percentage(40), // Peer connections
            Constraint::Percentage(40), // Shared resources
            Constraint::Min(0),      // Transfer activity
        ])
        .split(area);

    render_p2p_status_bar(f, chunks[0], app);
    render_peer_connections(f, chunks[1], app);
    render_shared_resources(f, chunks[2], app);
    render_transfer_activity(f, chunks[3], app);
}

fn render_p2p_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(50),
            Constraint::Length(50),
        ])
        .split(area);
    
    let status_text = if app.p2p_enabled {
        vec![
            Span::styled("🌐 P2P: ", Style::default().fg(TEXT_COLOR)),
            Span::styled("ACTIVE", Style::default().fg(SUCCESS_COLOR).add_modifier(Modifier::BOLD)),
            Span::styled(" | Node ID: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(&app.p2p_node_id, Style::default().fg(PRIMARY_COLOR)),
            Span::styled(" | Peers: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(app.p2p_peer_count.to_string(), Style::default().fg(INFO_COLOR)),
            Span::styled(" | Shared Images: ", Style::default().fg(TEXT_COLOR)),
            Span::styled(app.p2p_shared_images.to_string(), Style::default().fg(INFO_COLOR)),
        ]
    } else {
        vec![
            Span::styled("🌐 P2P: ", Style::default().fg(TEXT_COLOR)),
            Span::styled("DISABLED", Style::default().fg(Color::Gray)),
            Span::styled(" - Press 'p' to enable P2P networking", Style::default().fg(Color::Gray)),
        ]
    };

    let status_bar = Paragraph::new(Line::from(status_text))
        .block(Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(PRIMARY_COLOR)));

    f.render_widget(status_bar, chunks[0]);
    
    // Keyboard shortcuts
    let shortcuts = vec![
        Span::styled("p", Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Span::raw(":toggle "),
        Span::styled("c", Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Span::raw(":connect "),
        Span::styled("r", Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Span::raw(":refresh "),
        Span::styled("u", Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Span::raw(":upload "),
        Span::styled("d", Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Span::raw(":download "),
        Span::styled("q", Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Span::raw(":queue "),
    ];
    
    let shortcuts_widget = Paragraph::new(Line::from(shortcuts))
        .alignment(Alignment::Right)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(PRIMARY_COLOR)));
    
    f.render_widget(shortcuts_widget, chunks[1]);
}

fn render_peer_connections(f: &mut Frame, area: Rect, app: &App) {
    let header = Row::new(vec![
        Cell::from("Peer ID").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("Address").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("Status").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("Latency").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("Shared").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
    ]);

    let rows: Vec<Row> = app.p2p_peers.iter().map(|peer| {
        let status_color = match peer.status.as_str() {
            "connected" => SUCCESS_COLOR,
            "connecting" => WARNING_COLOR,
            "disconnected" => ERROR_COLOR,
            _ => TEXT_COLOR,
        };

        Row::new(vec![
            Cell::from(peer.node_id.chars().take(8).collect::<String>()),
            Cell::from(peer.address.as_str()),
            Cell::from(peer.status.as_str()).style(Style::default().fg(status_color)),
            Cell::from(format!("{}ms", peer.latency_ms)),
            Cell::from(format!("{} images", peer.shared_images)),
        ])
    }).collect();

    let table = Table::new(rows, [
        Constraint::Min(10),   // Peer ID
        Constraint::Min(20),   // Address
        Constraint::Min(12),   // Status
        Constraint::Min(10),   // Latency
        Constraint::Min(12),   // Shared
    ])
    .header(header)
    .block(Block::default()
        .borders(Borders::ALL)
        .title("🔗 Connected Peers")
        .border_style(Style::default().fg(PRIMARY_COLOR)));

    f.render_widget(table, area);
}

fn render_shared_resources(f: &mut Frame, area: Rect, app: &App) {
    let header = Row::new(vec![
        Cell::from("Image").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("Version").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("Size").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("Peers").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
        Cell::from("Status").style(Style::default().fg(PRIMARY_COLOR).add_modifier(Modifier::BOLD)),
    ]);

    let rows: Vec<Row> = app.p2p_images.iter().map(|image| {
        let status_color = if image.is_cached {
            SUCCESS_COLOR
        } else if image.is_downloading {
            WARNING_COLOR
        } else {
            TEXT_COLOR
        };

        let status = if image.is_cached {
            "Cached"
        } else if image.is_downloading {
            "Downloading..."
        } else {
            "Available"
        };

        Row::new(vec![
            Cell::from(image.name.as_str()),
            Cell::from(image.version.as_str()),
            Cell::from(format_bytes(image.size)),
            Cell::from(image.available_peers.to_string()),
            Cell::from(status).style(Style::default().fg(status_color)),
        ])
    }).collect();

    let table = Table::new(rows, [
        Constraint::Min(20),   // Image
        Constraint::Min(10),   // Version
        Constraint::Min(10),   // Size
        Constraint::Min(8),    // Peers
        Constraint::Min(15),   // Status
    ])
    .header(header)
    .block(Block::default()
        .borders(Borders::ALL)
        .title("💿 Shared VM Images")
        .border_style(Style::default().fg(PRIMARY_COLOR)));

    f.render_widget(table, area);
}

fn render_transfer_activity(f: &mut Frame, area: Rect, app: &App) {
    let _chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);

    // Active transfers
    let transfers: Vec<ListItem> = app.p2p_transfers.iter().map(|transfer| {
        let progress = (transfer.bytes_transferred as f64 / transfer.total_bytes as f64 * 100.0) as u16;
        let speed = format_bytes_per_sec(transfer.speed_bps);
        
        let icon = if transfer.is_upload { "⬆️" } else { "⬇️" };
        let direction = if transfer.is_upload { "Uploading" } else { "Downloading" };
        
        let text = format!(
            "{} {} {} to {} - {}% ({}) - {}",
            icon,
            direction,
            transfer.resource_name,
            transfer.peer_id.chars().take(8).collect::<String>(),
            progress,
            format_bytes(transfer.bytes_transferred),
            speed
        );
        
        ListItem::new(text).style(Style::default().fg(
            if transfer.is_upload { INFO_COLOR } else { SUCCESS_COLOR }
        ))
    }).collect();

    let list = List::new(transfers)
        .block(Block::default()
            .borders(Borders::ALL)
            .title("📊 Transfer Activity")
            .border_style(Style::default().fg(PRIMARY_COLOR)));

    f.render_widget(list, area);
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn format_bytes_per_sec(bps: u64) -> String {
    if bps < 1024 {
        format!("{} B/s", bps)
    } else if bps < 1024 * 1024 {
        format!("{:.1} KB/s", bps as f64 / 1024.0)
    } else {
        format!("{:.1} MB/s", bps as f64 / (1024.0 * 1024.0))
    }
}