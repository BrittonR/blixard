//// src/blixard/host_agent/vm_manager.gleam

///
/// VM management functions for interacting with microvm.nix and systemd
import blixard/domain/types.{type ResourceState} as domain_types
import blixard/host_agent/types.{type MicroVMStatus}
import blixard/storage/khepri_store
import envoy
import gleam/erlang/process
import gleam/int
import gleam/io
import gleam/list
import gleam/option
import gleam/otp/actor
import gleam/result
import gleam/string
import shellout

// Path to the microvm script
pub const microvm_bin = "/run/current-system/sw/bin/microvm"

// Define message type for our VM watcher actor
pub type WatcherMessage {
  CheckVMs
  // Message to trigger VM status check
}

// Create a VM using microvm.nix with the flake path
pub fn create_vm(name: String, config_path: String) -> Result(Nil, String) {
  // Get the absolute path to the flake directory
  let abs_path_result =
    shellout.command(run: "realpath", with: [config_path], in: ".", opt: [])
    |> result.map(string.trim)

  case abs_path_result {
    Ok(abs_path) -> {
      // Form a proper flake reference with absolute path
      let flake_ref = abs_path

      io.println("Creating VM " <> name <> " with flake: " <> flake_ref)

      // Run the microvm create command with sudo
      shellout.command(
        run: "sudo",
        with: [microvm_bin, "-c", name, "-f", flake_ref],
        in: ".",
        opt: [],
      )
      |> result.map(fn(_) { Nil })
      |> result.map_error(fn(err) {
        let #(_, message) = err
        "Failed to create VM: " <> message
      })
    }
    Error(_) -> {
      // Fallback - try using the path as-is
      io.println(
        "Warning: Could not resolve absolute path. Using as-is: " <> config_path,
      )

      let flake_ref = config_path

      io.println("Creating VM " <> name <> " with flake: " <> flake_ref)

      shellout.command(
        run: "sudo",
        with: [microvm_bin, "-c", name, "-f", flake_ref],
        in: ".",
        opt: [],
      )
      |> result.map(fn(_) { Nil })
      |> result.map_error(fn(err) {
        let #(_, message) = err
        "Failed to create VM: " <> message
      })
    }
  }
}

// Update a VM using microvm.nix
pub fn update_vm(name: String) -> Result(Nil, String) {
  io.println("Updating VM " <> name)

  shellout.command(
    run: "sudo",
    with: [microvm_bin, "-u", name],
    in: ".",
    opt: [],
  )
  |> result.map(fn(_) { Nil })
  |> result.map_error(fn(err) {
    let #(_, message) = err
    "Failed to update VM: " <> message
  })
}

// Start a VM using systemd
pub fn start_vm(name: String) -> Result(Nil, String) {
  io.println("Starting VM " <> name <> " using systemd")

  shellout.command(
    run: "sudo",
    with: ["systemctl", "start", "microvm@" <> name <> ".service"],
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
  io.println("Stopping VM " <> name <> " using systemd")

  shellout.command(
    run: "sudo",
    with: ["systemctl", "stop", "microvm@" <> name <> ".service"],
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
  io.println("Restarting VM " <> name <> " using systemd")

  shellout.command(
    run: "sudo",
    with: ["systemctl", "restart", "microvm@" <> name <> ".service"],
    in: ".",
    opt: [],
  )
  |> result.map(fn(_) { Nil })
  |> result.map_error(fn(err) {
    let #(_, message) = err
    "Failed to restart VM: " <> message
  })
}

// Start an ephemeral VM with a timeout (for serverless workloads)
pub fn start_ephemeral_vm(name: String, timeout_sec: Int) -> Result(Nil, String) {
  // Start the VM
  let start_result = start_vm(name)

  case start_result {
    Ok(_) -> {
      // Set up a timer to stop it after the timeout
      io.println(
        "Setting up timeout of "
        <> int.to_string(timeout_sec)
        <> " seconds for VM "
        <> name,
      )

      shellout.command(
        run: "sudo",
        with: [
          "systemd-run",
          "--on-active=" <> int.to_string(timeout_sec),
          "systemctl",
          "stop",
          "microvm@" <> name <> ".service",
        ],
        in: ".",
        opt: [],
      )
      |> result.map(fn(_) { Nil })
      |> result.map_error(fn(err) {
        let #(_, message) = err
        "Failed to set up VM timeout: " <> message
      })
    }
    Error(err) -> Error(err)
  }
}

// List all VMs using microvm.nix
pub fn list_vms() -> Result(List(MicroVMStatus), String) {
  io.println("Listing VMs using microvm -l")

  shellout.command(run: "sudo", with: [microvm_bin, "-l"], in: ".", opt: [])
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
      option.Some(status) -> Ok(status)
      option.None -> Error(Nil)
    }
  })
}

// Parse a single line from the microvm -l output
pub fn parse_microvm_list_line(
  line: String,
) -> option.Option(types.MicroVMStatus) {
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
            [version, ..] -> option.Some(version)
            _ -> option.None
          }
        }
        _ -> option.None
      }

      // For now, we don't have the VM ID from the microvm listing
      option.Some(types.MicroVMStatus(
        name,
        option.None,
        // vm_id
        is_running,
        is_outdated,
        system_version,
      ))
    }
    // If we only have one part, there's no colon in the line - it's invalid
    _ -> option.None
  }
}

// Get the status of a VM service using systemd
pub fn get_vm_service_status(name: String) -> Result(Bool, String) {
  let result =
    shellout.command(
      run: "sudo",
      with: ["systemctl", "is-active", "microvm@" <> name <> ".service"],
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

// Get the logs for a VM from journald
pub fn get_vm_logs(name: String, lines: Int) -> Result(String, String) {
  shellout.command(
    run: "sudo",
    with: [
      "journalctl",
      "-u",
      "microvm@" <> name <> ".service",
      "-n",
      int.to_string(lines),
      "--no-pager",
    ],
    in: ".",
    opt: [],
  )
  |> result.map_error(fn(err) {
    let #(_, message) = err
    "Failed to get VM logs: " <> message
  })
}

// Create and start a new ephemeral VM for serverless execution
pub fn run_serverless_vm(
  name: String,
  config_path: String,
  timeout_sec: Int,
) -> Result(Nil, String) {
  // First create the VM
  let create_result = create_vm(name, config_path)

  case create_result {
    Ok(_) -> {
      // Then start it with a timeout
      start_ephemeral_vm(name, timeout_sec)
    }
    Error(err) -> Error(err)
  }
}

// Delete a VM definition
pub fn delete_vm(name: String) -> Result(Nil, String) {
  io.println("Deleting VM " <> name)

  shellout.command(
    run: "sudo",
    with: [microvm_bin, "-d", name],
    in: ".",
    opt: [],
  )
  |> result.map(fn(_) { Nil })
  |> result.map_error(fn(err) {
    let #(_, message) = err
    "Failed to delete VM: " <> message
  })
}

// Monitor a VM and report status changes to Khepri
pub fn monitor_vm(
  name: String,
  vm_id: String,
  store: khepri_store.Khepri,
) -> Result(Nil, String) {
  io.println(
    "[VM MONITOR] Starting monitoring for VM "
    <> name
    <> " (ID: "
    <> vm_id
    <> ")",
  )

  // Check current status
  let status_result = get_vm_service_status(name)

  case status_result {
    Ok(is_running) -> {
      let state = case is_running {
        True -> {
          io.println("[VM MONITOR] VM " <> name <> " is currently running")
          domain_types.Running
          // Using domain_types instead of types
        }
        False -> {
          io.println("[VM MONITOR] VM " <> name <> " is currently stopped")
          domain_types.Stopped
          // Using domain_types instead of types
        }
      }

      // Update Khepri state
      io.println(
        "[VM MONITOR] Updating VM state in Khepri to: " <> string.inspect(state),
      )
      let update_result = khepri_store.update_vm_state(store, vm_id, state)

      case update_result {
        Ok(_) -> {
          io.println("[VM MONITOR] Successfully updated VM state in Khepri")
          Ok(Nil)
        }
        Error(err) -> {
          let error_msg =
            "Failed to update VM state in Khepri: " <> string.inspect(err)
          io.println("[VM MONITOR] " <> error_msg)
          Error(error_msg)
        }
      }
    }
    Error(err) -> {
      let error_msg = "Failed to get VM status: " <> err
      io.println("[VM MONITOR] " <> error_msg)
      Error(error_msg)
    }
  }
}

// Watch all VMs and update their status in Khepri
pub fn watch_vms(store: khepri_store.Khepri) -> Result(Nil, String) {
  io.println("[VM WATCHER] Starting VM status check")

  // Get all VMs from Khepri
  let vms_result = khepri_store.list_vms(store)

  case vms_result {
    Ok(vms) -> {
      io.println(
        "[VM WATCHER] Found "
        <> int.to_string(list.length(vms))
        <> " VMs in Khepri",
      )

      // Process each VM
      list.each(vms, fn(vm) {
        io.println(
          "[VM WATCHER] Checking VM " <> vm.name <> " (ID: " <> vm.id <> ")",
        )

        // Make sure both branches return the same type
        let _ = case vm.host_id {
          option.Some(host_id) -> {
            io.println(
              "[VM WATCHER] VM "
              <> vm.name
              <> " is assigned to host "
              <> host_id,
            )
            monitor_vm(vm.name, vm.id, store)
            // Returns Result(Nil, String)
          }
          option.None -> {
            io.println(
              "[VM WATCHER] VM " <> vm.name <> " is not assigned to any host",
            )
            Ok(Nil)
            // Match the Result(Nil, String) type
          }
        }
      })

      // After processing all VMs, return success
      Ok(Nil)
    }
    Error(err) -> {
      let error_msg = "Failed to list VMs from Khepri: " <> string.inspect(err)
      io.println("[VM WATCHER] " <> error_msg)
      Error(error_msg)
    }
  }
}

// Function to start the VM watcher process
pub type WatcherState {
  WatcherState(
    store: khepri_store.Khepri,
    subject: process.Subject(WatcherMessage),
  )
}

pub fn start_vm_watcher(
  store: khepri_store.Khepri,
) -> Result(process.Subject(WatcherMessage), actor.StartError) {
  // Create a new subject for self-messaging
  let subject = process.new_subject()

  // Create the initial state that includes both the store and the subject
  let state = WatcherState(store: store, subject: subject)

  // Create an actor with this state
  let actor_result =
    actor.start(state, fn(message: WatcherMessage, state: WatcherState) {
      case message {
        CheckVMs -> {
          io.println("[VM WATCHER] Running scheduled VM status check...")

          // Check all VMs
          let check_result = watch_vms(state.store)

          case check_result {
            Ok(_) ->
              io.println("[VM WATCHER] VM status check completed successfully")
            Error(err) ->
              io.println("[VM WATCHER] VM status check failed: " <> err)
          }

          // Schedule next check after 30 seconds using our subject
          let _ = process.send_after(state.subject, 30_000, CheckVMs)

          // Continue with same state
          actor.continue(state)
        }
      }
    })

  case actor_result {
    Ok(_) -> {
      // Send initial message to start the check cycle
      process.send(subject, CheckVMs)
      io.println("[VM WATCHER] VM watcher process started successfully")
      Ok(subject)
    }
    Error(reason) as error -> {
      io.println(
        "[VM WATCHER] Failed to start VM watcher process: "
        <> string.inspect(reason),
      )
      error
    }
  }
}
