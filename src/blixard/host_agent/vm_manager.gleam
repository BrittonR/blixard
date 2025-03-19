//// src/blixard/host_agent/vm_manager.gleam

///
/// VM management functions for interacting with microvm.nix and systemd
import blixard/host_agent/types.{type MicroVMStatus}
import envoy
import gleam/io
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import shellout

// Create a VM using nix run and the flake
pub fn create_vm(name: String, config_path: String) -> Result(Nil, String) {
  io.println("Creating VM " <> name <> " with flake directory: " <> config_path)

  // For creating the VM, we'll just build the flake output
  // This ensures the VM definition is built and cached
  shellout.command(
    run: "nix",
    with: [
      "build",
      config_path
        <> "#nixosConfigurations."
        <> name
        <> ".config.microvm.declaredRunner",
    ],
    in: ".",
    opt: [],
  )
  |> result.map(fn(_) { Nil })
  |> result.map_error(fn(err) {
    let #(_, message) = err
    "Failed to build VM: " <> message
  })
}

// Update a VM using nix build
pub fn update_vm(name: String) -> Result(Nil, String) {
  // Get flake directory from environment variable
  let flake_dir =
    envoy.get("BLIXARD_FLAKE_PATH")
    |> result.unwrap("./microvm_flakes")

  io.println("Updating VM " <> name <> " with flake directory: " <> flake_dir)

  // First rebuild the VM configuration
  shellout.command(
    run: "nix",
    with: [
      "build",
      "--update-input",
      "nixpkgs",
      flake_dir
        <> "#nixosConfigurations."
        <> name
        <> ".config.microvm.declaredRunner",
    ],
    in: ".",
    opt: [],
  )
  |> result.map(fn(_) { Nil })
  |> result.map_error(fn(err) {
    let #(_, message) = err
    "Failed to update VM: " <> message
  })
}

// Start a VM using nix run
pub fn start_vm(name: String) -> Result(Nil, String) {
  // Get flake directory from environment variable
  let flake_dir =
    envoy.get("BLIXARD_FLAKE_PATH")
    |> result.unwrap("./microvm_flakes")

  io.println("Starting VM " <> name <> " using nix run from " <> flake_dir)

  // Create a script to run the VM in the background
  let script_content =
    "#!/bin/sh\nnix run "
    <> flake_dir
    <> "#nixosConfigurations."
    <> name
    <> ".config.microvm.declaredRunner > /tmp/microvm-"
    <> name
    <> ".log 2>&1 &\necho $! > /tmp/microvm-"
    <> name
    <> ".pid\necho \"VM started with PID $(cat /tmp/microvm-"
    <> name
    <> ".pid)\""

  // Write the script to a temporary file
  let script_path = "/tmp/start-" <> name <> ".sh"
  let write_result =
    shellout.command(
      run: "bash",
      with: [
        "-c",
        "echo \""
          <> script_content
          <> "\" > "
          <> script_path
          <> " && chmod +x "
          <> script_path,
      ],
      in: ".",
      opt: [],
    )

  case write_result {
    Ok(_) -> {
      // Execute the script
      shellout.command(run: script_path, with: [], in: ".", opt: [])
      |> result.map(fn(_) { Nil })
      |> result.map_error(fn(err) {
        let #(_, message) = err
        "Failed to start VM: " <> message
      })
    }
    Error(err) -> {
      let #(_, message) = err
      Error("Failed to create start script: " <> message)
    }
  }
}

// Stop a VM using systemd (or find the process and kill it)
pub fn stop_vm(name: String) -> Result(Nil, String) {
  // For now, we'll try to find the VM process and kill it
  shellout.command(
    run: "pkill",
    with: ["-f", "microvm." <> name],
    in: ".",
    opt: [],
  )
  |> result.map(fn(_) { Nil })
  |> result.map_error(fn(err) {
    let #(_, message) = err
    "Failed to stop VM: " <> message
  })
}

// Restart a VM by stopping and starting
pub fn restart_vm(name: String) -> Result(Nil, String) {
  case stop_vm(name) {
    Ok(_) -> start_vm(name)
    Error(err) -> {
      // If stopping fails, try starting anyway
      io.println("Warning: Failed to stop VM: " <> err)
      start_vm(name)
    }
  }
}

// List all VMs using nix flake show
pub fn list_vms() -> Result(List(MicroVMStatus), String) {
  // Get flake directory from environment variable
  let flake_dir =
    envoy.get("BLIXARD_FLAKE_PATH")
    |> result.unwrap("./microvm_flakes")

  io.println("Listing VMs from flake directory: " <> flake_dir)

  // Use nix flake show to list available VMs
  shellout.command(
    run: "nix",
    with: ["flake", "show", flake_dir],
    in: ".",
    opt: [],
  )
  |> result.map(fn(output) { parse_flake_show_output(output) })
  |> result.map_error(fn(err) {
    let #(_, message) = err
    "Failed to list VMs: " <> message
  })
}

// Parse the output of nix flake show
pub fn parse_flake_show_output(output: String) -> List(MicroVMStatus) {
  // Split the output into lines
  let lines = string.split(output, "\n")

  // Extract VM names from nixosConfigurations
  let vm_names =
    list.filter_map(lines, fn(line) {
      // Look for lines with nixosConfigurations
      case string.contains(line, "nixosConfigurations") {
        True -> {
          // Try to extract the VM name
          case string.split(line, "my-") {
            [_, rest] -> {
              // Extract the VM name from the line
              case string.split(rest, ":") {
                [name, _] -> Ok(string.trim(name))
                _ -> Error(Nil)
              }
            }
            _ -> Error(Nil)
          }
        }
        False -> Error(Nil)
      }
    })

  // For each VM name, check if it's running
  list.map(vm_names, fn(name) {
    // Check if VM is running (simplified approach)
    let is_running =
      get_vm_service_status("my-" <> name)
      |> result.unwrap(False)

    // Create status object
    types.MicroVMStatus(
      name: "my-" <> name,
      vm_id: None,
      is_running: is_running,
      is_outdated: False,
      // Simplified
      system_version: None,
      // Simplified
    )
  })
}

// Get the status of a VM process
pub fn get_vm_service_status(name: String) -> Result(Bool, String) {
  // Check if VM process is running
  let result =
    shellout.command(
      run: "pgrep",
      with: ["-f", "microvm." <> name],
      in: ".",
      opt: [],
    )

  case result {
    Ok(_) -> {
      // Process found, VM is running
      Ok(True)
    }
    Error(_) -> {
      // No process found, VM is not running
      Ok(False)
    }
  }
}
// //// src/blixard/host_agent/vm_manager.gleam

// ///
// /// VM management functions for interacting with microvm.nix and systemd
// import blixard/host_agent/types.{type MicroVMStatus}
// import envoy
// import gleam/io
// import gleam/list
// import gleam/option.{type Option, None, Some}
// import gleam/result
// import gleam/string
// import shellout

// // Path to the microvm script
// pub const microvm_bin = "/run/current-system/sw/bin/microvm"

// // Create a VM using microvm.nix with the flake path
// // Update vm_manager.create_vm function in src/blixard/host_agent/vm_manager.gleam

// // Create a VM using microvm.nix with the flake path
// pub fn create_vm(name: String, config_path: String) -> Result(Nil, String) {
//   // Get the absolute path to the flake directory
//   let abs_path_result =
//     shellout.command(run: "realpath", with: [config_path], in: ".", opt: [])
//     |> result.map(string.trim)

//   case abs_path_result {
//     Ok(abs_path) -> {
//       // Form a proper flake reference with absolute path
//       let flake_ref = abs_path

//       io.println("Creating VM " <> name <> " with flake: " <> flake_ref)

//       // Run the microvm create command
//       shellout.command(
//         run: microvm_bin,
//         with: ["-c", name, "-f", flake_ref],
//         in: ".",
//         opt: [],
//       )
//       |> result.map(fn(_) { Nil })
//       |> result.map_error(fn(err) {
//         let #(_, message) = err
//         "Failed to create VM: " <> message
//       })
//     }
//     Error(_) -> {
//       // Fallback - try using the path as-is
//       io.println(
//         "Warning: Could not resolve absolute path. Using as-is: " <> config_path,
//       )

//       // Try alternative flake reference format
//       let flake_ref = config_path

//       io.println("Creating VM " <> name <> " with flake: " <> flake_ref)

//       shellout.command(
//         run: microvm_bin,
//         with: ["-c", name, "-f", flake_ref],
//         in: ".",
//         opt: [],
//       )
//       |> result.map(fn(_) { Nil })
//       |> result.map_error(fn(err) {
//         let #(_, message) = err
//         "Failed to create VM: " <> message
//       })
//     }
//   }
// }

// // Update a VM using microvm.nix
// pub fn update_vm(name: String) -> Result(Nil, String) {
//   shellout.command(run: microvm_bin, with: ["-u", name], in: ".", opt: [])
//   |> result.map(fn(_) { Nil })
//   |> result.map_error(fn(err) {
//     let #(_, message) = err
//     "Failed to update VM: " <> message
//   })
// }

// // Start a VM using systemd
// pub fn start_vm(name: String) -> Result(Nil, String) {
//   shellout.command(
//     run: "systemctl",
//     with: ["start", "microvm@" <> name <> ".service"],
//     in: ".",
//     opt: [],
//   )
//   |> result.map(fn(_) { Nil })
//   |> result.map_error(fn(err) {
//     let #(_, message) = err
//     "Failed to start VM: " <> message
//   })
// }

// // Stop a VM using systemd
// pub fn stop_vm(name: String) -> Result(Nil, String) {
//   shellout.command(
//     run: "systemctl",
//     with: ["stop", "microvm@" <> name <> ".service"],
//     in: ".",
//     opt: [],
//   )
//   |> result.map(fn(_) { Nil })
//   |> result.map_error(fn(err) {
//     let #(_, message) = err
//     "Failed to stop VM: " <> message
//   })
// }

// // Restart a VM using systemd
// pub fn restart_vm(name: String) -> Result(Nil, String) {
//   shellout.command(
//     run: "systemctl",
//     with: ["restart", "microvm@" <> name <> ".service"],
//     in: ".",
//     opt: [],
//   )
//   |> result.map(fn(_) { Nil })
//   |> result.map_error(fn(err) {
//     let #(_, message) = err
//     "Failed to restart VM: " <> message
//   })
// }

// // List all VMs using microvm.nix
// pub fn list_vms() -> Result(List(MicroVMStatus), String) {
//   shellout.command(run: microvm_bin, with: ["-l"], in: ".", opt: [])
//   |> result.map(fn(output) { parse_microvm_list_output(output) })
//   |> result.map_error(fn(err) {
//     let #(_, message) = err
//     "Failed to list VMs: " <> message
//   })
// }

// // Parse the output of the microvm -l command
// pub fn parse_microvm_list_output(output: String) -> List(MicroVMStatus) {
//   // Split the output into lines
//   let lines = string.split(output, "\n")

//   // Process each line
//   lines
//   |> list.filter(fn(line) { !string.is_empty(line) })
//   |> list.filter_map(fn(line) {
//     case parse_microvm_list_line(line) {
//       Some(status) -> Ok(status)
//       None -> Error(Nil)
//     }
//   })
// }

// // Parse a single line from the microvm -l output
// pub fn parse_microvm_list_line(line: String) -> Option(types.MicroVMStatus) {
//   // Example line format: 
//   // "vm-name: current(abcdef), not booted: systemctl start microvm@vm-name.service"
//   // "vm-name: outdated(12345), rebuild(67890) and reboot: microvm -Ru vm-name"

//   let parts = string.split(line, ":")

//   case parts {
//     // Check if we have at least two parts (name and some status info)
//     [name_part, rest_part, ..more_parts] -> {
//       let name = string.trim(name_part)

//       // Reconstruct the rest of the status text
//       let rest_parts = [rest_part, ..more_parts]
//       let status_text = string.join(rest_parts, ":")

//       let is_running = !string.contains(status_text, "not booted")
//       let is_outdated =
//         string.contains(status_text, "outdated")
//         || string.contains(status_text, "stale")

//       // Try to extract the system version
//       let system_version = case string.split(status_text, "(") {
//         [_, version_part, ..] -> {
//           case string.split(version_part, ")") {
//             [version, ..] -> Some(version)
//             _ -> None
//           }
//         }
//         _ -> None
//       }

//       // For now, we don't have the VM ID from the microvm listing
//       Some(types.MicroVMStatus(
//         name,
//         None,
//         // vm_id
//         is_running,
//         is_outdated,
//         system_version,
//       ))
//     }
//     // If we only have one part, there's no colon in the line - it's invalid
//     _ -> None
//   }
// }

// // Get the status of a VM service using systemd
// pub fn get_vm_service_status(name: String) -> Result(Bool, String) {
//   let result =
//     shellout.command(
//       run: "systemctl",
//       with: ["is-active", "microvm@" <> name <> ".service"],
//       in: ".",
//       opt: [],
//     )

//   case result {
//     Ok(output) -> {
//       // If active, is-active returns "active"
//       Ok(string.trim(output) == "active")
//     }
//     Error(_) -> {
//       // If the service is not active, systemctl exits with non-zero
//       Ok(False)
//     }
//   }
// }
