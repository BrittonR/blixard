//! Demo of P2P functionality in the TUI
//!
//! This example demonstrates:
//! 1. Starting the TUI with P2P tab
//! 2. Enabling P2P networking
//! 3. Viewing P2P status and transfers

use blixard::tui::app::{App, AppTab};
use blixard::tui::Event;
use blixard::BlixardResult;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt().with_env_filter("info").init();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and switch to P2P tab
    let mut app = App::new().await?;
    app.switch_tab(AppTab::P2P);

    // Create event channel
    let (tx, mut rx) = mpsc::channel(100);

    // Spawn input handler
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        loop {
            if event::poll(Duration::from_millis(100)).unwrap() {
                if let Ok(CEvent::Key(key)) = event::read() {
                    tx_clone.send(Event::Key(key)).await.unwrap();
                }
            }
        }
    });

    // Spawn tick handler
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            tx.send(Event::Tick).await.unwrap();
        }
    });

    // Main loop
    loop {
        // Draw UI
        terminal.draw(|f| blixard::tui::ui::render(f, &app))?;

        // Handle events
        if let Ok(event) = rx.try_recv() {
            app.handle_event(event).await?;

            if app.should_quit {
                break;
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
