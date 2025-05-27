// src/cli.gleam
//
// Command-line interface for the service management system
//
// This module handles parsing and dispatching CLI commands to the
// appropriate handlers, providing a user-friendly interface
// for managing systemd services across the cluster.

import gleam/io
import service_handlers
import systemd

/// Process CLI commands and dispatch to the appropriate service handler
///
/// # Arguments
/// - `args`: List of command-line arguments
///
/// # Command format
/// - `start [--user] <service>`: Start a service
/// - `stop [--user] <service>`: Stop a service
/// - `restart [--user] <service>`: Restart a service
/// - `status [--user] <service>`: Check service status
/// - `list`: List all managed services
/// - `list-cluster`: List all connected nodes
pub fn handle_command(args: List(String)) -> Nil {
  // Check for --user flag and extract mode
  let #(filtered_args, mode) = case args {
    ["start", "--user", service] -> #(["start", service], systemd.User)
    ["stop", "--user", service] -> #(["stop", service], systemd.User)
    ["restart", "--user", service] -> #(["restart", service], systemd.User)
    ["status", "--user", service] -> #(["status", service], systemd.User)
    _ -> #(args, systemd.System)
  }

  case filtered_args {
    // Start a service
    ["start", service] -> {
      let _ = service_handlers.handle_start_with_mode(service, mode)
      Nil
    }
    // Stop a service
    ["stop", service] -> {
      let _ = service_handlers.handle_stop_with_mode(service, mode)
      Nil
    }
    // Restart a service
    ["restart", service] -> {
      let _ = service_handlers.handle_restart_with_mode(service, mode)
      Nil
    }
    // Get service status
    ["status", service] -> {
      let _ = service_handlers.handle_status_with_mode(service, mode)
      Nil
    }
    // List all services in the cluster
    ["list"] -> {
      service_handlers.handle_list()
      Nil
    }
    // List all nodes in the cluster
    ["list-cluster"] -> {
      service_handlers.handle_list_cluster()
      Nil
    }
    // Default case - print usage information
    _ -> {
      print_usage()
      Nil
    }
  }
}

/// Print usage information for the CLI
pub fn print_usage() -> Nil {
  io.println("Service Manager CLI")
  io.println("==================")
  io.println("\nUsage:")
  io.println(
    "  service_manager start [--user] <service>     - Start a systemd service",
  )
  io.println(
    "  service_manager stop [--user] <service>      - Stop a systemd service",
  )
  io.println(
    "  service_manager restart [--user] <service>   - Restart a systemd service",
  )
  io.println(
    "  service_manager status [--user] <service>    - Check service status",
  )
  io.println(
    "  service_manager list                         - List all managed services",
  )
  io.println(
    "  service_manager list-cluster                 - List all connected nodes",
  )
  io.println("\nThe --user flag uses systemd user services (no root required)")
  io.println("\nSpecial commands:")
  io.println(
    "  service_manager --join-cluster           - Start node with auto-discovery",
  )
  io.println(
    "  service_manager --join-cluster <node>    - Start node and connect to specified node",
  )
  io.println(
    "  service_manager --stop-cluster           - Stop all running cluster nodes",
  )
}
