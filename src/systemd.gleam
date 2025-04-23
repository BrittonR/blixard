// src/systemd.gleam
import gleam/io
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import simplifile

// Execute a shell command and return the output
pub fn shell_exec(command: String) -> Result(String, String) {
  io.println("Executing: " <> command)

  // Use simplifile to execute shell commands
  case simplifile.exec(["/bin/sh", "-c", command]) {
    Ok(output) -> Ok(output)
    Error(e) -> Error("Command failed: " <> string.inspect(e))
  }
}

// Start a systemd service
pub fn start_service(service: String) -> Result(Nil, String) {
  case shell_exec("systemctl start " <> service) {
    Ok(_) -> Ok(Nil)
    Error(e) -> Error(e)
  }
}

// Stop a systemd service
pub fn stop_service(service: String) -> Result(Nil, String) {
  case shell_exec("systemctl stop " <> service) {
    Ok(_) -> Ok(Nil)
    Error(e) -> Error(e)
  }
}

// Restart a systemd service
pub fn restart_service(service: String) -> Result(Nil, String) {
  case shell_exec("systemctl restart " <> service) {
    Ok(_) -> Ok(Nil)
    Error(e) -> Error(e)
  }
}

// Get service status
pub fn service_status(service: String) -> Result(String, String) {
  shell_exec("systemctl status " <> service <> " --no-pager")
}

// Check if service is active
pub fn is_active(service: String) -> Result(Bool, String) {
  case shell_exec("systemctl is-active " <> service) {
    Ok(output) -> Ok(string.trim(output) == "active")
    Error(_) -> Ok(False)
    // If command fails, service is not active
  }
}

// Check if service is enabled
pub fn is_enabled(service: String) -> Result(Bool, String) {
  case shell_exec("systemctl is-enabled " <> service) {
    Ok(output) -> Ok(string.trim(output) == "enabled")
    Error(_) -> Ok(False)
  }
}

// Get a list of all systemd services
pub fn list_services() -> Result(List(String), String) {
  case
    shell_exec(
      "systemctl list-units --type=service --no-legend | awk '{print $1}' | sed 's/.service$//'",
    )
  {
    Ok(output) -> {
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
