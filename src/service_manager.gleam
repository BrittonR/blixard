// src/service_manager.gleam
import cluster_discovery
import gleam/dynamic
import gleam/erlang
import gleam/erlang/atom
import gleam/erlang/node
import gleam/erlang/process
import gleam/int
import gleam/io
import gleam/list
import gleam/option.{None, Some}
import gleam/result
import gleam/string
import khepri_store
import shellout
import systemd

// External function to start Erlang distribution
@external(erlang, "distribution_helper", "start_distribution")
fn start_distribution(
  node_name: String,
  hostname: String,
  cookie: String,
) -> Nil

pub fn main() -> Nil {
  // Get command-line arguments
  let args = erlang.start_arguments()

  // Process special setup commands first
  case args {
    ["--init-primary"] -> {
      start_primary_node()
      Nil
    }
    ["--init-secondary", primary_node] -> {
      start_secondary_node(primary_node)
      Nil
    }
    _ -> Nil
  }

  // Regular command processing - ensure Erlang distribution is started
  ensure_distribution()

  // Initialize Khepri (assumes it's already running in cluster mode)
  let _ = khepri_store.init()

  // Process commands
  case args {
    ["start", service] -> {
      let _ = handle_start(service)
      Nil
    }
    ["stop", service] -> {
      let _ = handle_stop(service)
      Nil
    }
    ["restart", service] -> {
      let _ = handle_restart(service)
      Nil
    }
    ["status", service] -> {
      let _ = handle_status(service)
      Nil
    }
    ["list"] -> {
      handle_list()
      Nil
    }
    ["list-cluster"] -> {
      handle_list_cluster()
      Nil
    }
    _ -> {
      print_usage()
      Nil
    }
  }
}

// Ensure we're running in distributed mode for CLI operations
fn ensure_distribution() -> Nil {
  let current_node = node.self()
  let node_name = atom.to_string(node.to_atom(current_node))

  // Check if already distributed
  case node_name == "nonode@nohost" {
    True -> {
      // Start distribution using our helper
      let name =
        "service_cli_" <> int.to_string(erlang.system_time(erlang.Millisecond))
      let hostname = "127.0.0.1"
      // Use IP address instead of "localhost"
      let cookie = "khepri_cookie"

      start_distribution(name, hostname, cookie)

      // Verify distribution started
      let new_node = node.self()
      let new_name = atom.to_string(node.to_atom(new_node))

      case new_name == "nonode@nohost" {
        True -> {
          io.println(
            "WARNING: Failed to start distribution, functionality will be limited",
          )
          Nil
        }
        False -> {
          io.println("Running as distributed node: " <> new_name)

          // Try to discover and connect to other nodes
          let _ = cluster_discovery.connect_to_all_nodes("khepri_node")
          Nil
        }
      }
    }
    False -> {
      io.println("Already running in distributed mode: " <> node_name)
      Nil
    }
  }
}

// Start a primary Khepri node
fn start_primary_node() -> Nil {
  io.println("Starting primary Khepri node...")

  // Start distribution
  ensure_distribution()

  // Initialize the Khepri cluster as primary
  let config =
    khepri_store.ClusterConfig(
      node_role: khepri_store.Primary,
      cookie: "khepri_cookie",
    )

  case khepri_store.init_cluster(config) {
    Ok(_) -> {
      io.println("Primary Khepri node started successfully")
      io.println("Waiting for connections...")

      // Keep the process running
      process.sleep_forever()
    }
    Error(err) -> {
      io.println_error("Failed to start primary node: " <> err)
      Nil
    }
  }
}

// Start a secondary Khepri node
fn start_secondary_node(primary_node: String) -> Nil {
  io.println("Starting secondary Khepri node...")
  io.println("Primary node: " <> primary_node)

  // Start distribution
  ensure_distribution()

  // Initialize the Khepri cluster as secondary
  let config =
    khepri_store.ClusterConfig(
      node_role: khepri_store.Secondary(primary_node),
      cookie: "khepri_cookie",
    )

  case khepri_store.init_cluster(config) {
    Ok(_) -> {
      io.println("Secondary Khepri node started successfully")
      io.println("Connected to primary node")

      // Keep the process running
      process.sleep_forever()
    }
    Error(err) -> {
      io.println_error("Failed to start secondary node: " <> err)
      Nil
    }
  }
}

// Handler for starting a service
fn handle_start(service: String) -> Result(Nil, String) {
  io.println("Starting service: " <> service)

  case systemd.start_service(service) {
    Ok(_) -> {
      io.println("Service started successfully")

      // Update state in Khepri
      let _ = khepri_store.store_service_state(service, khepri_store.Running)

      // Verify service is running
      case systemd.is_active(service) {
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

// Handler for stopping a service
fn handle_stop(service: String) -> Result(Nil, String) {
  io.println("Stopping service: " <> service)

  case systemd.stop_service(service) {
    Ok(_) -> {
      io.println("Service stopped successfully")

      // Update state in Khepri
      let _ = khepri_store.store_service_state(service, khepri_store.Stopped)

      Ok(Nil)
    }
    Error(err) -> {
      io.println_error("Failed to stop service: " <> err)

      // Record failed state
      let _ = khepri_store.store_service_state(service, khepri_store.Failed)

      Error(err)
    }
  }
}

// Handler for restarting a service
fn handle_restart(service: String) -> Result(Nil, String) {
  io.println("Restarting service: " <> service)

  case systemd.restart_service(service) {
    Ok(_) -> {
      io.println("Service restarted successfully")

      // Update state in Khepri
      let _ = khepri_store.store_service_state(service, khepri_store.Running)

      // Verify service is running
      case systemd.is_active(service) {
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

// Handler for getting service status
fn handle_status(service: String) -> Result(Nil, String) {
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
  case systemd.service_status(service) {
    Ok(status) -> {
      io.println("Systemd status:")
      io.println(status)

      // Update state in Khepri based on actual status
      case systemd.is_active(service) {
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

// Handler for listing services
fn handle_list() -> Nil {
  io.println("Listing all managed services:")

  case khepri_store.list_services() {
    Ok(services) -> {
      case list.length(services) {
        0 -> io.println("No services are currently managed")
        _ -> {
          io.println("\nService Status from Khepri Database:")
          io.println("--------------------------------")
          list.each(services, fn(service) {
            let #(name, state) = service
            io.println("- " <> name <> ": " <> string.inspect(state))
          })
        }
      }
    }
    Error(err) -> {
      io.println_error("Failed to list services from Khepri: " <> err)
    }
  }

  // Also list systemd services
  case systemd.list_services() {
    Ok(system_services) -> {
      io.println("\nSystemd Services Available:")
      io.println("------------------------")
      list.each(list.take(system_services, 10), fn(service) {
        io.println("- " <> service)
      })

      case list.length(system_services) > 10 {
        True ->
          io.println(
            "... and "
            <> int.to_string(list.length(system_services) - 10)
            <> " more",
          )
        False -> Nil
      }
    }
    Error(err) -> {
      io.println_error("Failed to list systemd services: " <> err)
    }
  }
}

// Handler for listing cluster nodes
fn handle_list_cluster() -> Nil {
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

// Print usage information
fn print_usage() -> Nil {
  io.println("Service Manager CLI")
  io.println("==================")
  io.println("\nUsage:")
  io.println("  service_manager start <service>     - Start a systemd service")
  io.println("  service_manager stop <service>      - Stop a systemd service")
  io.println(
    "  service_manager restart <service>   - Restart a systemd service",
  )
  io.println("  service_manager status <service>    - Check service status")
  io.println(
    "  service_manager list                - List all managed services",
  )
  io.println("  service_manager list-cluster        - List all connected nodes")
  io.println("\nSpecial commands:")
  io.println(
    "  service_manager --init-primary           - Start primary Khepri node",
  )
  io.println(
    "  service_manager --init-secondary <node>  - Start secondary Khepri node",
  )
}
