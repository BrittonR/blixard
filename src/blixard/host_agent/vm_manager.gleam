//// src/blixard/host_agent/vm_manager.gleam

///
/// VM management functions for interacting with microvm.nix and systemd
import blixard/host_agent/types.{type MicroVMStatus}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shellout

// Path to the microvm script
pub const microvm_bin = "/run/current-system/sw/bin/microvm"

// Create a VM using microvm.nix
pub fn create_vm(name: String, flake_path: String) -> Result(Nil, String) {
  shellout.command(
    run: microvm_bin,
    with: ["-c", name, "-f", flake_path],
    in: ".",
    opt: [],
  )
  |> result.map(fn(_) { Nil })
  |> result.map_error(fn(err) {
    let #(_, message) = err
    "Failed to create VM: " <> message
  })
}

// Update a VM using microvm.nix
pub fn update_vm(name: String) -> Result(Nil, String) {
  shellout.command(run: microvm_bin, with: ["-u", name], in: ".", opt: [])
  |> result.map(fn(_) { Nil })
  |> result.map_error(fn(err) {
    let #(_, message) = err
    "Failed to update VM: " <> message
  })
}

// Start a VM using systemd
pub fn start_vm(name: String) -> Result(Nil, String) {
  shellout.command(
    run: "systemctl",
    with: ["start", "microvm@" <> name <> ".service"],
    in: ".",
    opt: [],
  )
  |> result.map(fn(_) { Nil })
  |> result.map_error(fn(err) {
    let #(_, message) = err
    "Failed to start VM: " <> message
  })
}

// Stop a VM using systemd
pub fn stop_vm(name: String) -> Result(Nil, String) {
  shellout.command(
    run: "systemctl",
    with: ["stop", "microvm@" <> name <> ".service"],
    in: ".",
    opt: [],
  )
  |> result.map(fn(_) { Nil })
  |> result.map_error(fn(err) {
    let #(_, message) = err
    "Failed to stop VM: " <> message
  })
}

// Restart a VM using systemd
pub fn restart_vm(name: String) -> Result(Nil, String) {
  shellout.command(
    run: "systemctl",
    with: ["restart", "microvm@" <> name <> ".service"],
    in: ".",
    opt: [],
  )
  |> result.map(fn(_) { Nil })
  |> result.map_error(fn(err) {
    let #(_, message) = err
    "Failed to restart VM: " <> message
  })
}

// List all VMs using microvm.nix
pub fn list_vms() -> Result(List(MicroVMStatus), String) {
  shellout.command(run: microvm_bin, with: ["-l"], in: ".", opt: [])
  |> result.map(fn(output) { parse_microvm_list_output(output) })
  |> result.map_error(fn(err) {
    let #(_, message) = err
    "Failed to list VMs: " <> message
  })
}

// Parse the output of the microvm -l command
pub fn parse_microvm_list_output(output: String) -> List(MicroVMStatus) {
  // Split the output into lines
  let lines = string.split(output, "\n")

  // Process each line
  lines
  |> list.filter(fn(line) { !string.is_empty(line) })
  |> list.filter_map(fn(line) {
    case parse_microvm_list_line(line) {
      Some(status) -> Ok(status)
      None -> Error(Nil)
    }
  })
}

// Parse a single line from the microvm -l output
pub fn parse_microvm_list_line(line: String) -> Option(types.MicroVMStatus) {
  // Example line format: 
  // "vm-name: current(abcdef), not booted: systemctl start microvm@vm-name.service"
  // "vm-name: outdated(12345), rebuild(67890) and reboot: microvm -Ru vm-name"

  let parts = string.split(line, ":")

  case parts {
    // Check if we have at least two parts (name and some status info)
    [name_part, rest_part, ..more_parts] -> {
      let name = string.trim(name_part)

      // Reconstruct the rest of the status text
      let rest_parts = [rest_part, ..more_parts]
      let status_text = string.join(rest_parts, ":")

      let is_running = !string.contains(status_text, "not booted")
      let is_outdated =
        string.contains(status_text, "outdated")
        || string.contains(status_text, "stale")

      // Try to extract the system version
      let system_version = case string.split(status_text, "(") {
        [_, version_part, ..] -> {
          case string.split(version_part, ")") {
            [version, ..] -> Some(version)
            _ -> None
          }
        }
        _ -> None
      }

      // For now, we don't have the VM ID from the microvm listing
      Some(types.MicroVMStatus(
        name,
        None,
        // vm_id
        is_running,
        is_outdated,
        system_version,
      ))
    }
    // If we only have one part, there's no colon in the line - it's invalid
    _ -> None
  }
}

// Get the status of a VM service using systemd
pub fn get_vm_service_status(name: String) -> Result(Bool, String) {
  let result =
    shellout.command(
      run: "systemctl",
      with: ["is-active", "microvm@" <> name <> ".service"],
      in: ".",
      opt: [],
    )

  case result {
    Ok(output) -> {
      // If active, is-active returns "active"
      Ok(string.trim(output) == "active")
    }
    Error(_) -> {
      // If the service is not active, systemctl exits with non-zero
      Ok(False)
    }
  }
}
