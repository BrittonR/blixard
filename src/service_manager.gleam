// src/service_manager.gleam
//
// Main entry point for the service management system
// 
// This module handles the CLI dispatch logic, initializing the appropriate
// mode of operation (primary node, secondary node, or client operation)
// based on command line arguments.

import cli
import gleam/erlang
import gleam/io
import khepri_store
import node_manager
import service_handlers

/// Main function that parses command line arguments and dispatches
/// to the appropriate operation mode:
/// 
/// - Primary node initialization (--init-primary)
/// - Secondary node initialization (--init-secondary <primary_node>)
/// - Normal CLI command processing
pub fn main() -> Nil {
  // Get command-line arguments
  let args = erlang.start_arguments()

  // Process special setup commands first
  case args {
    // Initialize as a primary Khepri node
    ["--init-primary"] -> {
      node_manager.start_primary_node()
      Nil
    }
    // Initialize as a secondary Khepri node that connects to the primary
    ["--init-secondary", primary_node] -> {
      node_manager.start_secondary_node(primary_node)
      Nil
    }
    // Regular command processing path for CLI operations
    _ -> {
      // Ensure Erlang distribution is started for distributed operations
      node_manager.ensure_distribution()

      // Initialize Khepri (assumes it's already running in cluster mode)
      // This allows CLI commands to read/write to the distributed store
      let _ = khepri_store.init()

      // Process regular commands (start, stop, restart, etc.)
      cli.handle_command(args)
    }
  }
}
