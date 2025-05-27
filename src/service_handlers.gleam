// src/service_handlers.gleam
//
// Service management operation handlers
//
// This module contains handlers for the various service management operations,
// such as starting, stopping, restarting, and checking status of services.
// It interacts with both the systemd module for actual service control
// and the khepri_store module for storing service state.

import gleam/erlang/atom
import gleam/erlang/node
import gleam/int
import gleam/io
import gleam/list
import gleam/result
import gleam/string
import khepri_store
import systemd

/// Handler for starting a service with mode
///
/// # Arguments
/// - `service`: Name of the service to start
/// - `mode`: SystemdMode (System or User)
///
/// # Returns
/// - `Ok(Nil)` if the operation succeeded
/// - `Error(String)` with error message if the operation failed
///
/// # Effects
/// - Starts the systemd service
/// - Updates the service state in Khepri
pub fn handle_start_with_mode(
  service: String,
  mode: systemd.SystemdMode,
) -> Result(Nil, String) {
  io.println("Starting service: " <> service)

  case systemd.start_service_with_mode(service, mode) {
    Ok(_) -> {
      io.println("Service started successfully")

      // Update state in Khepri
      let _ = khepri_store.store_service_state(service, khepri_store.Running)

      // Verify service is running
      case systemd.is_active_with_mode(service, mode) {
        Ok(True) -> io.println("Verified: Service is running")
        _ -> io.println("Warning: Service may not be running properly")
      }

      Ok(Nil)
    }
    Error(err) -> {
      io.println_error("Failed to start service: " <> err)

      // Record failed state
      let _ = khepri_store.store_service_state(service, khepri_store.Failed)

      Error(err)
    }
  }
}

/// Handler for stopping a service with mode
///
/// # Arguments
/// - `service`: Name of the service to stop
/// - `mode`: SystemdMode (System or User)
///
/// # Returns
/// - `Ok(Nil)` if the operation succeeded
/// - `Error(String)` with error message if the operation failed
///
/// # Effects
/// - Stops the systemd service
/// - Updates the service state in Khepri OR removes it if requested
pub fn handle_stop_with_mode(
  service: String,
  mode: systemd.SystemdMode,
) -> Result(Nil, String) {
  io.println("Stopping service: " <> service)

  case systemd.stop_service_with_mode(service, mode) {
    Ok(_) -> {
      io.println("Service stopped successfully")

      // Check if this service is managed by our CLI
      case khepri_store.get_service_info(service) {
        Ok(_) -> {
          // Service is managed by CLI, ask if user wants to remove it from management
          io.println("This service is managed by Blixard.")
          io.println(
            "Choose: (1) Mark as stopped in Blixard, or (2) Remove from Blixard management",
          )
          io.println("Defaulting to option 1 (mark as stopped)...")

          // For now, just mark as stopped. Later we can add interactive choice.
          let _ =
            khepri_store.store_service_state(service, khepri_store.Stopped)

          io.println(
            "Service marked as stopped in Blixard. Use 'list' to see all managed services.",
          )
        }
        Error(_) -> {
          // Service not managed by CLI, just stop it
          io.println("Service stopped (not managed by Blixard)")
        }
      }

      Ok(Nil)
    }
    Error(err) -> {
      io.println_error("Failed to stop service: " <> err)

      // Record failed state if it's managed by us
      case khepri_store.get_service_info(service) {
        Ok(_) -> {
          let _ = khepri_store.store_service_state(service, khepri_store.Failed)
        }
        Error(_) -> Nil
      }

      Error(err)
    }
  }
}

/// Handler for removing a service from Blixard management
///
/// # Arguments
/// - `service`: Name of the service to remove from management
///
/// # Returns
/// - `Ok(Nil)` if the operation succeeded
/// - `Error(String)` with error message if the operation failed
pub fn handle_remove_from_management(service: String) -> Result(Nil, String) {
  io.println("Removing " <> service <> " from Blixard management...")

  case khepri_store.get_service_info(service) {
    Ok(_) -> {
      // Service exists in our management, remove it
      case khepri_store.remove_service_state(service) {
        Ok(_) -> {
          io.println("✅ " <> service <> " removed from Blixard management")
          io.println(
            "The service itself is still running - use systemctl to manage it directly",
          )
          Ok(Nil)
        }
        Error(err) -> Error("Failed to remove from management: " <> err)
      }
    }
    Error(_) -> {
      Error("Service " <> service <> " is not managed by Blixard")
    }
  }
}

/// Handler for restarting a service with mode
///
/// # Arguments
/// - `service`: Name of the service to restart
/// - `mode`: SystemdMode (System or User)
///
/// # Returns
/// - `Ok(Nil)` if the operation succeeded
/// - `Error(String)` with error message if the operation failed
///
/// # Effects
/// - Restarts the systemd service
/// - Updates the service state in Khepri
pub fn handle_restart_with_mode(
  service: String,
  mode: systemd.SystemdMode,
) -> Result(Nil, String) {
  io.println("Restarting service: " <> service)

  case systemd.restart_service_with_mode(service, mode) {
    Ok(_) -> {
      io.println("Service restarted successfully")

      // Update state in Khepri
      let _ = khepri_store.store_service_state(service, khepri_store.Running)

      // Verify service is running
      case systemd.is_active_with_mode(service, mode) {
        Ok(True) -> io.println("Verified: Service is running")
        _ -> io.println("Warning: Service may not be running properly")
      }

      Ok(Nil)
    }
    Error(err) -> {
      io.println_error("Failed to restart service: " <> err)

      // Record failed state
      let _ = khepri_store.store_service_state(service, khepri_store.Failed)

      Error(err)
    }
  }
}

/// Handler for getting service status with mode
///
/// # Arguments
/// - `service`: Name of the service to check
/// - `mode`: SystemdMode (System or User)
///
/// # Returns
/// - `Ok(Nil)` if the status check succeeded
/// - `Error(String)` with error message if the status check failed
///
/// # Effects
/// - Retrieves the service status from systemd
/// - Updates the service state in Khepri based on current status
pub fn handle_status_with_mode(
  service: String,
  mode: systemd.SystemdMode,
) -> Result(Nil, String) {
  io.println("Checking status of service: " <> service)

  // Get current state from Khepri
  case khepri_store.get_service_state(service) {
    Ok(state) -> {
      io.println("Service state in Khepri: " <> string.inspect(state))
    }
    Error(_) -> {
      io.println("Service state not found in Khepri")
    }
  }

  // Get actual systemd status
  case systemd.service_status_with_mode(service, mode) {
    Ok(status) -> {
      io.println("Systemd status:")
      io.println(status)

      // Update state in Khepri based on actual status
      case systemd.is_active_with_mode(service, mode) {
        Ok(True) -> {
          let _ =
            khepri_store.store_service_state(service, khepri_store.Running)
        }
        Ok(False) -> {
          let _ =
            khepri_store.store_service_state(service, khepri_store.Stopped)
        }
        Error(_) -> {
          let _ =
            khepri_store.store_service_state(service, khepri_store.Unknown)
        }
      }

      Ok(Nil)
    }
    Error(err) -> {
      io.println_error("Failed to get service status: " <> err)
      Error(err)
    }
  }
}

// Keep original functions for backward compatibility (system mode)
/// Handler for starting a service
///
/// # Arguments
/// - `service`: Name of the service to start
///
/// # Returns
/// - `Ok(Nil)` if the operation succeeded
/// - `Error(String)` with error message if the operation failed
///
/// # Effects
/// - Starts the systemd service
/// - Updates the service state in Khepri
pub fn handle_start(service: String) -> Result(Nil, String) {
  handle_start_with_mode(service, systemd.System)
}

/// Handler for stopping a service
///
/// # Arguments
/// - `service`: Name of the service to stop
///
/// # Returns
/// - `Ok(Nil)` if the operation succeeded
/// - `Error(String)` with error message if the operation failed
///
/// # Effects
/// - Stops the systemd service
/// - Updates the service state in Khepri
pub fn handle_stop(service: String) -> Result(Nil, String) {
  handle_stop_with_mode(service, systemd.System)
}

/// Handler for restarting a service
///
/// # Arguments
/// - `service`: Name of the service to restart
///
/// # Returns
/// - `Ok(Nil)` if the operation succeeded
/// - `Error(String)` with error message if the operation failed
///
/// # Effects
/// - Restarts the systemd service
/// - Updates the service state in Khepri
pub fn handle_restart(service: String) -> Result(Nil, String) {
  handle_restart_with_mode(service, systemd.System)
}

/// Handler for getting service status
///
/// # Arguments
/// - `service`: Name of the service to check
///
/// # Returns
/// - `Ok(Nil)` if the status check succeeded
/// - `Error(String)` with error message if the status check failed
///
/// # Effects
/// - Retrieves the service status from systemd
/// - Updates the service state in Khepri based on current status
pub fn handle_status(service: String) -> Result(Nil, String) {
  handle_status_with_mode(service, systemd.System)
}

/// Format timestamp for display
fn format_timestamp(timestamp: String) -> String {
  case timestamp {
    "unknown" -> "unknown"
    _ -> {
      // Try to parse as milliseconds and format nicely
      case int.parse(timestamp) {
        Ok(millis) -> {
          // Simple formatting - just show last few digits for readability
          let seconds = millis / 1000
          let display_seconds = seconds % 100_000
          // Last 5 digits
          "..." <> int.to_string(display_seconds)
        }
        Error(_) -> timestamp
      }
    }
  }
}

/// Format service state for display
fn format_service_state(state: khepri_store.ServiceState) -> String {
  case state {
    khepri_store.Running -> "🟢 RUNNING"
    khepri_store.Stopped -> "🔴 STOPPED"
    khepri_store.Failed -> "❌ FAILED"
    khepri_store.Unknown -> "❓ UNKNOWN"
    khepri_store.Custom(msg) -> "📝 " <> msg
  }
}

/// Handler for listing all managed services with enhanced display and cluster info
///
/// # Effects
/// - Retrieves and displays all CLI-managed services from Khepri with metadata
/// - Shows a summary of available systemd services
pub fn handle_list() -> Nil {
  io.println("CLI-Managed Services (via Blixard)")
  io.println("==================================")

  // Show cluster connection info first
  let connected_nodes = node.visible()
  case list.length(connected_nodes) > 0 {
    True -> {
      io.println(
        "🌐 Connected to cluster with "
        <> int.to_string(list.length(connected_nodes))
        <> " other nodes:",
      )
      list.each(connected_nodes, fn(node_name) {
        io.println("   • " <> atom.to_string(node.to_atom(node_name)))
      })
      io.println("")
    }
    False -> {
      io.println("⚠️  Running in standalone mode (no cluster nodes detected)")
      io.println("")
    }
  }

  // Debug: Show all paths in Khepri
  io.println("🔍 Debugging Khepri paths:")
  khepri_store.debug_print_paths()

  // List CLI-managed services from Khepri with full metadata
  case khepri_store.list_cli_services() {
    Ok(services) -> {
      case list.length(services) {
        0 -> {
          io.println("No services are currently managed by the CLI")
          io.println("")
          io.println(
            "Start a service with: gleam run -m service_manager -- start [--user] <service-name>",
          )
        }
        count -> {
          io.println(
            "Found " <> int.to_string(count) <> " CLI-managed services:",
          )
          io.println("")

          // Group services by node
          let nodes = list.map(services, fn(s) { s.node }) |> list.unique()

          list.each(nodes, fn(node_name) {
            let node_services =
              list.filter(services, fn(s) { s.node == node_name })

            case list.length(node_services) {
              0 -> Nil
              node_count -> {
                io.println(
                  "📍 Node: "
                  <> node_name
                  <> " ("
                  <> int.to_string(node_count)
                  <> " services)",
                )

                list.each(node_services, fn(service_info) {
                  let status = format_service_state(service_info.state)
                  let time = format_timestamp(service_info.last_updated)
                  io.println(
                    "   • "
                    <> service_info.name
                    <> " - "
                    <> status
                    <> " (updated: "
                    <> time
                    <> ")",
                  )
                })
                io.println("")
              }
            }
          })

          io.println(
            "💡 Use 'stop <service>' to stop and optionally remove from management",
          )
          io.println(
            "💡 Use 'remove <service>' to remove from Blixard management without stopping",
          )
        }
      }
    }
    Error(err) -> {
      io.println_error("Failed to list CLI-managed services: " <> err)
      io.println("")
    }
  }

  // Show a compact summary of systemd services
  io.println("Available Systemd Services (sample)")
  io.println("===================================")

  case systemd.list_services() {
    Ok(system_services) -> {
      let count = list.length(system_services)
      io.println("Found " <> int.to_string(count) <> " total systemd services")

      // Show first 5 as examples
      let sample_services = list.take(system_services, 5)
      case list.length(sample_services) > 0 {
        True -> {
          io.println("Examples:")
          list.each(sample_services, fn(service) {
            io.println("   • " <> service)
          })

          case count > 5 {
            True ->
              io.println("   ... and " <> int.to_string(count - 5) <> " more")
            False -> Nil
          }
        }
        False -> Nil
      }
    }
    Error(err) -> {
      io.println_error("Failed to list systemd services: " <> err)
    }
  }

  io.println("")
  io.println("💡 Use 'status <service>' to check a specific service")
  io.println("💡 Use 'start [--user] <service>' to manage a service via Blixard")
}

/// Handler for listing cluster nodes
///
/// # Effects
/// - Retrieves and displays all connected nodes in the cluster
pub fn handle_list_cluster() -> Nil {
  io.println("Connected Erlang nodes:")

  let connected_nodes = node.visible()
  case list.length(connected_nodes) {
    0 -> io.println("No connected nodes (standalone mode)")
    _ -> {
      list.each(connected_nodes, fn(node_name) {
        io.println("- " <> atom.to_string(node.to_atom(node_name)))
      })
    }
  }
}
