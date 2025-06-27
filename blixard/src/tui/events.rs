use crossterm::event::{self, Event as CrosstermEvent, KeyEvent, MouseEvent};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum Event {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Tick,
    #[allow(dead_code)]
    LogLine(String),
    P2pPeersUpdate(Vec<crate::tui::app::P2pPeer>),
    P2pTransfersUpdate(Vec<crate::tui::app::P2pTransfer>),
    P2pImagesUpdate(Vec<crate::tui::app::P2pImage>),
}

/// Event handler for the TUI
pub struct EventHandler {
    #[allow(dead_code)]
    sender: mpsc::UnboundedSender<Event>,
    receiver: mpsc::UnboundedReceiver<Event>,
    last_tick: Instant,
    tick_rate: Duration,
}

impl EventHandler {
    pub fn new(tick_rate: u64) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        
        Self {
            sender,
            receiver,
            last_tick: Instant::now(),
            tick_rate: Duration::from_millis(tick_rate),
        }
    }

    /// Get the next event, blocking until one is available
    pub async fn next(&mut self) -> crate::BlixardResult<Event> {
        let timeout = self.tick_rate
            .checked_sub(self.last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        // Check for crossterm events
        if event::poll(timeout).map_err(|e| crate::BlixardError::Internal {
            message: format!("Failed to poll events: {}", e),
        })? {
            match event::read().map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to read event: {}", e),
            })? {
                CrosstermEvent::Key(key) => return Ok(Event::Key(key)),
                CrosstermEvent::Mouse(mouse) => return Ok(Event::Mouse(mouse)),
                _ => {}
            }
        }

        // Check for tick events
        if self.last_tick.elapsed() >= self.tick_rate {
            self.last_tick = Instant::now();
            return Ok(Event::Tick);
        }

        // Check for custom events (like log lines)
        if let Ok(event) = self.receiver.try_recv() {
            return Ok(event);
        }

        // If no events, wait a short time and return tick
        tokio::time::sleep(Duration::from_millis(50)).await;
        Ok(Event::Tick)
    }

    /// Get a sender for custom events
    #[allow(dead_code)]
    pub fn sender(&self) -> mpsc::UnboundedSender<Event> {
        self.sender.clone()
    }
}