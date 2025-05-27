// src/systemd.gleam
//
// Interface to systemd service management
//
// This module provides functions for interacting with systemd services,
// including starting, stopping, restarting, and checking status.
// It uses shell commands via the shellout library to execute systemd commands.

import gleam/io
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shellout

/// Systemd mode - either system-wide or user services
pub type SystemdMode {
  System
  User
}

/// Execute a shell command and return the output
///
/// # Arguments
/// - `command`: The shell command to execute
///
/// # Returns
/// - `Ok(String)` with command output if execution succeeded
/// - `Error(String)` with error message if execution failed
pub fn shell_exec(command: String) -> Result(String, String) {
  io.println("Executing: " <> command)

  // Use shellout to execute shell commands
  case shellout.command(run: "sh", with: ["-c", command], in: ".", opt: []) {
    Ok(output) -> Ok(output)
    Error(#(_, message)) -> Error("Command failed: " <> message)
  }
}

/// Execute a shell command with mode support
pub fn shell_exec_with_mode(
  command: String,
  mode: SystemdMode,
) -> Result(String, String) {
  let final_command = case mode {
    User -> command <> " --user"
    System -> command
  }

  io.println("Executing: " <> final_command)

  case
    shellout.command(run: "sh", with: ["-c", final_command], in: ".", opt: [])
  {
    Ok(output) -> Ok(output)
    Error(#(_, message)) -> Error("Command failed: " <> message)
  }
}

/// Start a systemd service with mode
///
/// # Arguments
/// - `service`: The name of the service to start
/// - `mode`: SystemdMode (System or User)
///
/// # Returns
/// - `Ok(Nil)` if service start succeeded
/// - `Error(String)` with error message if service start failed
pub fn start_service_with_mode(
  service: String,
  mode: SystemdMode,
) -> Result(Nil, String) {
  case shell_exec_with_mode("systemctl start " <> service, mode) {
    Ok(_) -> Ok(Nil)
    Error(e) -> Error(e)
  }
}

/// Stop a systemd service with mode
///
/// # Arguments
/// - `service`: The name of the service to stop
/// - `mode`: SystemdMode (System or User)
///
/// # Returns
/// - `Ok(Nil)` if service stop succeeded
/// - `Error(String)` with error message if service stop failed
pub fn stop_service_with_mode(
  service: String,
  mode: SystemdMode,
) -> Result(Nil, String) {
  case shell_exec_with_mode("systemctl stop " <> service, mode) {
    Ok(_) -> Ok(Nil)
    Error(e) -> Error(e)
  }
}

/// Restart a systemd service with mode
///
/// # Arguments
/// - `service`: The name of the service to restart
/// - `mode`: SystemdMode (System or User)
///
/// # Returns
/// - `Ok(Nil)` if service restart succeeded
/// - `Error(String)` with error message if service restart failed
pub fn restart_service_with_mode(
  service: String,
  mode: SystemdMode,
) -> Result(Nil, String) {
  case shell_exec_with_mode("systemctl restart " <> service, mode) {
    Ok(_) -> Ok(Nil)
    Error(e) -> Error(e)
  }
}

/// Get detailed status of a systemd service with mode
///
/// # Arguments
/// - `service`: The name of the service to check status
/// - `mode`: SystemdMode (System or User)
///
/// # Returns
/// - `Ok(String)` with service status if request succeeded
/// - `Error(String)` with error message if request failed
pub fn service_status_with_mode(
  service: String,
  mode: SystemdMode,
) -> Result(String, String) {
  shell_exec_with_mode("systemctl status " <> service <> " --no-pager", mode)
}

/// Check if a service is active with mode
///
/// # Arguments
/// - `service`: The name of the service to check
/// - `mode`: SystemdMode (System or User)
///
/// # Returns
/// - `Ok(Bool)` with True if service is active, False if not active
/// - `Error(String)` with error message if check failed catastrophically
pub fn is_active_with_mode(
  service: String,
  mode: SystemdMode,
) -> Result(Bool, String) {
  case shell_exec_with_mode("systemctl is-active " <> service, mode) {
    Ok(output) -> Ok(string.trim(output) == "active")
    Error(_) -> Ok(False)
    // If command fails, service is not active
  }
}

/// Check if a service is enabled to start at boot with mode
///
/// # Arguments
/// - `service`: The name of the service to check
/// - `mode`: SystemdMode (System or User)
///
/// # Returns
/// - `Ok(Bool)` with True if service is enabled, False if not enabled
/// - `Error(String)` with error message if check failed catastrophically
pub fn is_enabled_with_mode(
  service: String,
  mode: SystemdMode,
) -> Result(Bool, String) {
  case shell_exec_with_mode("systemctl is-enabled " <> service, mode) {
    Ok(output) -> Ok(string.trim(output) == "enabled")
    Error(_) -> Ok(False)
    // If command fails, service is not enabled
  }
}

/// Get a list of all systemd services on the system with mode
///
/// # Arguments
/// - `mode`: SystemdMode (System or User)
///
/// # Returns
/// - `Ok(List(String))` with list of service names if request succeeded
/// - `Error(String)` with error message if request failed
pub fn list_services_with_mode(
  mode: SystemdMode,
) -> Result(List(String), String) {
  // The command extracts service names without the .service suffix
  let cmd =
    "systemctl list-units --type=service --no-legend --user | awk '{print $1}' | sed 's/.service$//'"
  case shell_exec_with_mode(cmd, mode) {
    Ok(output) -> {
      // Split the output by newlines and filter out empty lines
      let services =
        output
        |> string.trim
        |> string.split("\n")
        |> list.filter(fn(s) { !string.is_empty(s) })

      Ok(services)
    }
    Error(e) -> Error(e)
  }
}

// Keep the original functions for backward compatibility (system mode)
/// Start a systemd service
///
/// # Arguments
/// - `service`: The name of the service to start
///
/// # Returns
/// - `Ok(Nil)` if service start succeeded
/// - `Error(String)` with error message if service start failed
pub fn start_service(service: String) -> Result(Nil, String) {
  start_service_with_mode(service, System)
}

/// Stop a systemd service
///
/// # Arguments
/// - `service`: The name of the service to stop
///
/// # Returns
/// - `Ok(Nil)` if service stop succeeded
/// - `Error(String)` with error message if service stop failed
pub fn stop_service(service: String) -> Result(Nil, String) {
  stop_service_with_mode(service, System)
}

/// Restart a systemd service
///
/// # Arguments
/// - `service`: The name of the service to restart
///
/// # Returns
/// - `Ok(Nil)` if service restart succeeded
/// - `Error(String)` with error message if service restart failed
pub fn restart_service(service: String) -> Result(Nil, String) {
  restart_service_with_mode(service, System)
}

/// Get detailed status of a systemd service
///
/// # Arguments
/// - `service`: The name of the service to check status
///
/// # Returns
/// - `Ok(String)` with service status if request succeeded
/// - `Error(String)` with error message if request failed
pub fn service_status(service: String) -> Result(String, String) {
  service_status_with_mode(service, System)
}

/// Check if a service is active
///
/// # Arguments
/// - `service`: The name of the service to check
///
/// # Returns
/// - `Ok(Bool)` with True if service is active, False if not active
/// - `Error(String)` with error message if check failed catastrophically
pub fn is_active(service: String) -> Result(Bool, String) {
  is_active_with_mode(service, System)
}

/// Check if a service is enabled to start at boot
///
/// # Arguments
/// - `service`: The name of the service to check
///
/// # Returns
/// - `Ok(Bool)` with True if service is enabled, False if not enabled
/// - `Error(String)` with error message if check failed catastrophically
pub fn is_enabled(service: String) -> Result(Bool, String) {
  is_enabled_with_mode(service, System)
}

/// Get a list of all systemd services on the system
///
/// # Returns
/// - `Ok(List(String))` with list of service names if request succeeded
/// - `Error(String)` with error message if request failed
pub fn list_services() -> Result(List(String), String) {
  list_services_with_mode(System)
}
