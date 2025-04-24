// src/service_handlers.gleam
import gleam/erlang/atom
import gleam/erlang/node
import gleam/int
import gleam/io
import gleam/list
import gleam/result
import gleam/string
import khepri_store
import systemd

// Handler for starting a service
pub fn handle_start(service: String) -> Result(Nil, String) {
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
pub fn handle_stop(service: String) -> Result(Nil, String) {
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
pub fn handle_restart(service: String) -> Result(Nil, String) {
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
pub fn handle_status(service: String) -> Result(Nil, String) {
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
pub fn handle_list() -> Nil {
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
