pub mod events;
pub mod vm_client;

// Modern TUI implementation  
pub mod app_v2;
pub mod ui_v2;

pub use events::Event;
pub use vm_client::VmClient;

use crate::BlixardResult;
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io;

/// Initialize terminal for TUI
pub fn init_terminal() -> BlixardResult<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode().map_err(|e| crate::BlixardError::Internal { 
        message: format!("Failed to enable raw mode: {}", e) 
    })?;
    execute!(io::stdout(), EnterAlternateScreen).map_err(|e| crate::BlixardError::Internal { 
        message: format!("Failed to enter alternate screen: {}", e) 
    })?;
    
    let backend = CrosstermBackend::new(io::stdout());
    let terminal = Terminal::new(backend).map_err(|e| crate::BlixardError::Internal { 
        message: format!("Failed to create terminal: {}", e) 
    })?;
    
    Ok(terminal)
}

/// Restore terminal to normal state
pub fn restore_terminal() -> BlixardResult<()> {
    disable_raw_mode().map_err(|e| crate::BlixardError::Internal { 
        message: format!("Failed to disable raw mode: {}", e) 
    })?;
    execute!(io::stdout(), LeaveAlternateScreen).map_err(|e| crate::BlixardError::Internal { 
        message: format!("Failed to leave alternate screen: {}", e) 
    })?;
    
    Ok(())
}