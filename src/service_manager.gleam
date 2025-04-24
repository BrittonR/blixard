// src/service_manager.gleam
import cli
import gleam/erlang
import gleam/io
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import khepri_store
import node_manager
import service_handlers

/// Main function that parses command line arguments and dispatches
/// to the appropriate operation mode
pub fn main() -> Nil {
  // Get command-line arguments and remove the program name
  let args = erlang.start_arguments()

  io.println("Processing arguments: " <> string.inspect(args))

  // Process args
  case args {
    // Join cluster with auto-discovery
    ["--join-cluster"] -> {
      node_manager.start_cluster_node(None)
      Nil
    }
    // Join cluster with specified seed node
    ["--join-cluster", seed_node] -> {
      node_manager.start_cluster_node(Some(seed_node))
      Nil
    }
    // Stop any running cluster nodes
    ["--stop-cluster"] -> {
      node_manager.stop_cluster()
      Nil
    }
    // Backward compatibility with old commands
    ["--init-primary"] -> {
      node_manager.start_cluster_node(None)
      Nil
    }
    ["--init-secondary", primary_node] -> {
      node_manager.start_cluster_node(Some(primary_node))
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
