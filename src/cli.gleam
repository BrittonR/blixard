// src/cli.gleam
import gleam/io
import service_handlers

// Process CLI commands
pub fn handle_command(args: List(String)) -> Nil {
  case args {
    ["start", service] -> {
      let _ = service_handlers.handle_start(service)
      Nil
    }
    ["stop", service] -> {
      let _ = service_handlers.handle_stop(service)
      Nil
    }
    ["restart", service] -> {
      let _ = service_handlers.handle_restart(service)
      Nil
    }
    ["status", service] -> {
      let _ = service_handlers.handle_status(service)
      Nil
    }
    ["list"] -> {
      service_handlers.handle_list()
      Nil
    }
    ["list-cluster"] -> {
      service_handlers.handle_list_cluster()
      Nil
    }
    _ -> {
      print_usage()
      Nil
    }
  }
}

// Print usage information
pub fn print_usage() -> Nil {
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
