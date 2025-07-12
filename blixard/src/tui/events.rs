use crossterm::event::{self, Event as CrosstermEvent, KeyEvent, MouseEvent};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Events that drive the TUI application
///
/// Represents all types of events that the TUI can process,
/// including user input, system ticks, and network updates.
/// This is the primary communication mechanism between the
/// event handler and the application state.
///
/// # Variants
///
/// * `Key(KeyEvent)` - Keyboard input from the user
/// * `Mouse(MouseEvent)` - Mouse interactions (clicks, scrolling)
/// * `Tick` - Regular timer event for data refresh and animations
/// * `LogLine(String)` - New log entry to display in debug views
/// * `Reconnect` - Trigger reconnection to the cluster
/// * `P2pPeersUpdate` - Updated list of P2P peers
/// * `P2pTransfersUpdate` - Updated list of P2P data transfers
/// * `P2pImagesUpdate` - Updated list of shared P2P images
#[derive(Debug, Clone)]
pub enum Event {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Tick,
    #[allow(dead_code)]
    LogLine(String),
    Reconnect,
    P2pPeersUpdate(Vec<crate::tui::app::P2pPeer>),
    P2pTransfersUpdate(Vec<crate::tui::app::P2pTransfer>),
    P2pImagesUpdate(Vec<crate::tui::app::P2pImage>),
}

/// Event handler that manages input and timing for the TUI
///
/// Coordinates between terminal input events (keyboard, mouse) and
/// application timing events (ticks for refresh). Provides a unified
/// event stream that the main application loop can process.
///
/// # Features
///
/// - Configurable tick rate for regular refresh events
/// - Non-blocking event polling with timeout handling
/// - Integration with crossterm for terminal input
/// - Channel-based event delivery to the application
///
/// # Usage
///
/// The event handler runs in the main event loop, delivering events
/// to the application for processing. Tick events drive regular data
/// refresh and UI updates.
pub struct EventHandler {
    #[allow(dead_code)]
    sender: mpsc::UnboundedSender<Event>,
    receiver: mpsc::UnboundedReceiver<Event>,
    last_tick: Instant,
    tick_rate: Duration,
}

impl EventHandler {
    /// Create a new event handler with the specified tick rate
    ///
    /// # Arguments
    ///
    /// * `tick_rate` - Milliseconds between tick events for regular updates
    ///
    /// # Examples
    ///
    /// ```rust
    /// use blixard::tui::events::EventHandler;
    ///
    /// // Create handler with 1 second ticks
    /// let handler = EventHandler::new(1000);
    /// ```
    pub fn new(tick_rate: u64) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();

        Self {
            sender,
            receiver,
            last_tick: Instant::now(),
            tick_rate: Duration::from_millis(tick_rate),
        }
    }

    /// Get the next event from the input stream
    ///
    /// Polls for keyboard/mouse input with a timeout based on the tick rate.
    /// If no input is received before the timeout, returns a Tick event
    /// to drive regular application updates.
    ///
    /// # Returns
    ///
    /// * `Ok(Event)` - The next event to process
    /// * `Err(BlixardError)` - Error reading input or polling
    ///
    /// # Behavior
    ///
    /// - Returns input events (Key, Mouse) immediately when available
    /// - Returns Tick events at regular intervals based on tick_rate
    /// - Handles terminal event polling errors gracefully
    pub async fn next(&mut self) -> crate::BlixardResult<Event> {
        let timeout = self
            .tick_rate
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
