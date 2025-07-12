//! Terminal User Interface (TUI) for Blixard cluster management
//!
//! This module provides a comprehensive text-based user interface for managing
//! Blixard clusters, virtual machines, and nodes. The TUI offers real-time
//! monitoring, interactive management, and diagnostic capabilities.
//!
//! # Architecture
//!
//! The TUI is organized into several key modules:
//!
//! - **State Management** (`state/`) - Application state and data models
//! - **Type Definitions** (`types/`) - Data structures for VMs, nodes, and UI
//! - **Forms** (`forms/`) - Interactive forms for resource creation and configuration
//! - **Events** (`events`) - Event handling and input processing
//! - **Client** (`vm_client`) - Network client for cluster communication
//! - **UI Rendering** (`ui`, `app`) - Terminal rendering and layout
//!
//! # Features
//!
//! - **Real-time Monitoring** - Live cluster, VM, and node status updates
//! - **Interactive Management** - Create, start, stop, and migrate VMs
//! - **Cluster Operations** - Join/leave nodes, view consensus status
//! - **P2P Networking** - Monitor peer connections and image sharing
//! - **Debug Tools** - Raft diagnostics, logs, and system metrics
//! - **Performance Modes** - Configurable refresh rates and resource usage
//!
//! # Quick Start
//!
//! ```rust
//! use blixard::tui::{Event, VmClient};
//! use blixard::tui::state::app_state::App;
//!
//! # async fn example() -> blixard::BlixardResult<()> {
//! // Create application state
//! let mut app = App::new();
//!
//! // Connect to cluster (internal method)
//! // app.try_connect().await;
//!
//! // Process events in main loop
//! // loop {
//! //     let event = event_handler.next().await?;
//! //     app.handle_event(event).await?;
//! // }
//! # Ok(())
//! # }
//! ```

pub mod cluster_config;
pub mod events;
pub mod vm_client;

// TUI state management
pub mod state;
pub mod types;
pub mod forms;

// TUI implementation
pub mod app;
pub mod p2p_view;
pub mod ui;

pub use events::Event;
pub use vm_client::VmClient;
