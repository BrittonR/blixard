//! State management for the TUI application
//!
//! This module organizes the application state into focused modules
//! for better maintainability and separation of concerns.

pub mod app_state;
pub mod vm_state;
pub mod node_state;
pub mod monitoring_state;
pub mod debug_state;
pub mod ui_state;

// Re-export the main App struct
pub use app_state::App;