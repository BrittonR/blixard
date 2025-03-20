//// src/blixard.gleam

///
/// Main entry point for the Blixard orchestrator
import envoy
import gleam/dict
import gleam/erlang
import gleam/erlang/atom
import gleam/erlang/process
import gleam/float
import gleam/int
import gleam/io
import gleam/list
import gleam/option
import gleam/result
import gleam/string

import blixard/debug_test
import blixard/domain/types
import blixard/host_agent/agent
import blixard/host_agent/cli
import blixard/host_agent/types as agent_types
import blixard/host_agent/vm_manager
import blixard/orchestrator/core
import blixard/storage/khepri_store
import blixard/test_khepri

// Main function
pub fn main() {
  // Parse command-line arguments
  let args = erlang.start_arguments()

  // Check which mode to run
  case args {
    // Debug test mode
    ["--debug-ffi"] -> {
      debug_test.main()
    }

    // Khepri test mode
    ["--test-khepri"] -> {
      test_khepri.main()
    }

    // Combined test mode - create host agent and VM in one process
    ["--test-agent-create-vm", host_id, vm_name] -> {
      create_vm_directly(host_id, vm_name)
    }

    // Run as host agent with custom flake path
    ["--host-agent", host_id, flake_path] -> {
      run_host_agent(host_id, flake_path)
    }

    // Run as host agent with default flake path
    ["--host-agent", host_id] -> {
      run_host_agent(host_id, "./microvm_flakes")
    }

    // Send commands to a running host agent
    ["--agent-list-vms"] -> {
      case cli.list_vms() {
        Ok(vms) -> {
          io.println("VMs on host:")
          list.each(vms, fn(vm) {
            let status = case vm.is_running {
              True -> "Running"
              False -> "Stopped"
            }
            let outdated = case vm.is_outdated {
              True -> " (outdated)"
              False -> ""
            }
            io.println("- " <> vm.name <> ": " <> status <> outdated)
          })
        }
        Error(err) -> io.println("Error: " <> err)
      }
    }

    ["--agent-create-vm", vm_name] -> {
      // Get flake directory from environment or use microvm_flakes directory
      let flake_dir =
        envoy.get("BLIXARD_FLAKE_PATH")
        |> result.unwrap("./microvm_flakes")

      // Create a new VM record
      let vm =
        types.MicroVm(
          id: generate_uuid(),
          name: vm_name,
          description: option.Some("VM created from CLI"),
          vm_type: types.Persistent,
          resources: types.Resources(cpu_cores: 2, memory_mb: 2048, disk_gb: 20),
          state: types.Pending,
          host_id: option.None,
          storage_volumes: [],
          network_interfaces: [],
          tailscale_config: types.TailscaleConfig(
            enabled: True,
            auth_key: option.None,
            hostname: vm_name,
            tags: [],
            direct_client: True,
          ),
          nixos_config: types.NixosConfig(
            config_path: flake_dir,
            overrides: dict.new(),
            cache_url: option.None,
          ),
          labels: dict.new(),
          created_at: int.to_string(erlang.system_time(erlang.Second)),
          updated_at: int.to_string(erlang.system_time(erlang.Second)),
        )

      // Send the create command to the agent
      let reply_subject = process.new_subject()
      case cli.send_command(agent_types.CreateVM(vm, reply_subject)) {
        Ok(dynamic_result) -> {
          // In a real implementation, we'd handle the dynamic result properly
          io.println("VM created successfully")
        }
        Error(err) -> io.println("Error: " <> err)
      }
    }

    ["--agent-start-vm", vm_id] -> {
      case cli.start_vm(vm_id) {
        Ok(_) -> io.println("VM started successfully")
        Error(err) -> io.println("Error: " <> err)
      }
    }

    ["--agent-stop-vm", vm_id] -> {
      case cli.stop_vm(vm_id) {
        Ok(_) -> io.println("VM stopped successfully")
        Error(err) -> io.println("Error: " <> err)
      }
    }

    ["--agent-restart-vm", vm_id] -> {
      case cli.restart_vm(vm_id) {
        Ok(_) -> io.println("VM restarted successfully")
        Error(err) -> io.println("Error: " <> err)
      }
    }

    ["--agent-status"] -> {
      case cli.get_status() {
        Ok(status) -> {
          io.println("Host Status:")
          io.println("- Host ID: " <> status.host_id)
          io.println("- VM Count: " <> int.to_string(status.vm_count))
          io.println(
            "- Running VMs: " <> int.to_string(list.length(status.running_vms)),
          )
          io.println(
            "- CPU Cores Available: "
            <> int.to_string(status.available_resources.cpu_cores),
          )
          io.println(
            "- Memory Available: "
            <> int.to_string(status.available_resources.memory_mb)
            <> " MB",
          )
          io.println(
            "- Disk Available: "
            <> int.to_string(status.available_resources.disk_gb)
            <> " GB",
          )
          io.println(
            "- Load Average: "
            <> string.join(list.map(status.load_average, float.to_string), ", "),
          )
          io.println(
            "- Memory Usage: "
            <> float.to_string(status.memory_usage_percent)
            <> "%",
          )
          io.println(
            "- Disk Usage: "
            <> float.to_string(status.disk_usage_percent)
            <> "%",
          )
        }
        Error(err) -> io.println("Error: " <> err)
      }
    }

    ["--agent-metrics"] -> {
      case cli.get_metrics() {
        Ok(metrics) -> io.println(metrics)
        Error(err) -> io.println("Error: " <> err)
      }
    }

    // Check VM status manually (new command)
    // Check VM status manually (new command)
    ["--check-vms"] -> {
      io.println("Checking status of all VMs...")
      let store_result =
        khepri_store.start(["blixard@127.0.0.1"], "blixard_cluster")

      case store_result {
        Ok(store) -> {
          // Use the new watch_vms function to check all VMs
          let check_result = vm_manager.watch_vms(store)

          case check_result {
            Ok(_) -> io.println("VM status check completed successfully")
            Error(err) -> io.println("Error checking VM status: " <> err)
          }

          // Make sure to discard the result by using let _ assignment
          let _ = khepri_store.stop(store)
          // Return Nil to match other case branches
          Nil
        }
        Error(err) -> {
          io.println(
            "Failed to initialize Khepri store: " <> safe_debug_error(err),
          )
          // Return Nil to match other case branches
          Nil
        }
      }
    }

    // Normal mode
    _ -> {
      run_normal()
    }
  }
}

// Function to create a VM directly without using the agent process
fn create_vm_directly(host_id: String, vm_name: String) -> Nil {
  io.println("Starting both host agent and creating VM in one process...")

  // Start Khepri store
  io.println("Initializing Khepri store...")
  let store_result =
    khepri_store.start(["blixard@127.0.0.1"], "blixard_cluster")

  case store_result {
    Ok(store) -> {
      io.println("Khepri store initialized successfully!")

      // Set environment variable for VM manager to use
      envoy.set("BLIXARD_FLAKE_PATH", "./microvm_flakes")

      // Check if host exists, create if not
      io.println("Checking if host exists...")
      let host_result = khepri_store.get_host(store, host_id)

      case host_result {
        Ok(_) -> {
          io.println("Host found in Khepri store.")
          create_and_store_vm(store, host_id, vm_name)
        }
        Error(_) -> {
          io.println("Host not found, creating new host entry...")
          // Create a default host
          let new_host =
            types.Host(
              id: host_id,
              name: "Host " <> host_id,
              description: option.Some("Automatically created host"),
              control_ip: "127.0.0.1",
              connected: True,
              available_resources: types.Resources(
                cpu_cores: 4,
                memory_mb: 8192,
                disk_gb: 100,
              ),
              total_resources: types.Resources(
                cpu_cores: 4,
                memory_mb: 8192,
                disk_gb: 100,
              ),
              vm_ids: [],
              schedulable: True,
              tags: [],
              labels: dict.new(),
              created_at: int.to_string(erlang.system_time(erlang.Second)),
              updated_at: int.to_string(erlang.system_time(erlang.Second)),
            )

          // Store the host
          case khepri_store.put_host(store, new_host) {
            Ok(_) -> {
              io.println("Host created successfully!")
              create_and_store_vm(store, host_id, vm_name)
            }
            Error(err) -> {
              io.println("Failed to create host: " <> safe_debug_error(err))
            }
          }
        }
      }
    }
    Error(err) -> {
      io.println("Failed to initialize Khepri store: " <> safe_debug_error(err))
    }
  }
}

// Enhanced version of create_and_store_vm with better debug output
fn create_and_store_vm(
  store: khepri_store.Khepri,
  host_id: String,
  vm_name: String,
) -> Nil {
  // Create a VM record with proper flake reference
  let flake_path = "./microvm_flakes"

  io.println(
    "[VM LIFECYCLE] Creating VM definition with ID and storing in Khepri",
  )
  let vm_id = generate_uuid()
  io.println("  - Generated VM ID: " <> vm_id)
  io.println("  - VM Name: " <> vm_name)
  io.println("  - Host ID: " <> host_id)
  io.println("  - Initial State: Pending")
  io.println("  - Flake Path: " <> flake_path)

  let vm =
    types.MicroVm(
      id: vm_id,
      name: vm_name,
      description: option.Some("VM created from test command"),
      vm_type: types.Persistent,
      resources: types.Resources(cpu_cores: 2, memory_mb: 2048, disk_gb: 20),
      state: types.Pending,
      host_id: option.Some(host_id),
      // Set the host ID directly
      storage_volumes: [],
      network_interfaces: [],
      tailscale_config: types.TailscaleConfig(
        enabled: True,
        auth_key: option.None,
        hostname: vm_name,
        tags: [],
        direct_client: True,
      ),
      nixos_config: types.NixosConfig(
        // Reference the flake directory
        config_path: flake_path,
        overrides: dict.new(),
        cache_url: option.None,
      ),
      labels: dict.new(),
      created_at: int.to_string(erlang.system_time(erlang.Second)),
      updated_at: int.to_string(erlang.system_time(erlang.Second)),
    )

  // Store VM in Khepri
  io.println("[VM LIFECYCLE] Storing VM in Khepri store...")
  case khepri_store.put_vm(store, vm) {
    Ok(_) -> {
      io.println("[VM LIFECYCLE] VM stored successfully in Khepri!")

      // Update host object to include the VM ID
      io.println("[VM LIFECYCLE] Updating host to include VM ID...")
      let _ = khepri_store.assign_vm_to_host(store, vm.id, host_id)

      // Build the VM using microvm
      io.println("[VM LIFECYCLE] Building VM using microvm command...")
      case vm_manager.create_vm(vm.name, vm.nixos_config.config_path) {
        Ok(_) -> {
          io.println("[VM LIFECYCLE] VM built successfully!")

          // Update VM state in Khepri
          io.println("[VM LIFECYCLE] Updating VM state to Stopped in Khepri...")
          let state_result =
            khepri_store.update_vm_state(store, vm.id, types.Stopped)

          case state_result {
            Ok(_) ->
              io.println(
                "[VM LIFECYCLE] VM state updated successfully to Stopped",
              )
            Error(err) ->
              io.println(
                "[VM LIFECYCLE] Failed to update VM state: "
                <> safe_debug_error(err),
              )
          }

          // Automatically start the VM
          io.println("[VM LIFECYCLE] Starting VM using systemd...")
          case vm_manager.start_vm(vm.name) {
            Ok(_) -> {
              io.println("[VM LIFECYCLE] VM started successfully!")

              // Update state in Khepri
              io.println(
                "[VM LIFECYCLE] Updating VM state to Running in Khepri...",
              )
              let run_state_result =
                khepri_store.update_vm_state(store, vm.id, types.Running)

              case run_state_result {
                Ok(_) ->
                  io.println(
                    "[VM LIFECYCLE] VM state updated successfully to Running",
                  )
                Error(err) ->
                  io.println(
                    "[VM LIFECYCLE] Failed to update VM state: "
                    <> safe_debug_error(err),
                  )
              }

              // Start VM monitoring 
              io.println("[VM LIFECYCLE] Setting up VM monitoring...")
              let _ = vm_manager.monitor_vm(vm.name, vm.id, store)

              io.println("[VM LIFECYCLE] VM is now running. To stop it, use:")
              io.println("gleam run -m blixard -- --agent-stop-vm " <> vm.id)
              io.println("or")
              io.println(
                "sudo systemctl stop microvm@" <> vm.name <> ".service",
              )
            }
            Error(err) -> {
              io.println("[VM LIFECYCLE] Failed to start VM: " <> err)
              io.println("You can try starting it manually with:")
              io.println(
                "sudo systemctl start microvm@" <> vm.name <> ".service",
              )
            }
          }
        }
        Error(err) -> {
          io.println("[VM LIFECYCLE] Failed to build VM: " <> err)
        }
      }
    }
    Error(err) -> {
      io.println(
        "[VM LIFECYCLE] Failed to store VM in Khepri: " <> safe_debug_error(err),
      )
    }
  }
}

fn run_host_agent(host_id: String, flake_path: String) -> Nil {
  io.println("Starting Blixard Host Agent for host ID: " <> host_id)
  io.println("Using flake directory: " <> flake_path)

  // Start Khepri store
  io.println("Initializing Khepri store...")
  let store_result =
    khepri_store.start(["blixard@127.0.0.1"], "blixard_cluster")

  case store_result {
    Ok(store) -> {
      io.println("Khepri store initialized successfully!")

      // Set environment variable for VM manager to use
      envoy.set("BLIXARD_FLAKE_PATH", flake_path)

      // Check if host exists, create if not
      io.println("Checking if host exists...")
      let host_result = khepri_store.get_host(store, host_id)

      case host_result {
        Ok(_) -> {
          io.println("Host found in Khepri store.")
          start_agent(host_id, store)
        }
        Error(_) -> {
          io.println("Host not found, creating new host entry...")
          // Create a default host
          let new_host =
            types.Host(
              id: host_id,
              name: "Host " <> host_id,
              description: option.Some("Automatically created host"),
              control_ip: "127.0.0.1",
              connected: True,
              available_resources: types.Resources(
                cpu_cores: 4,
                memory_mb: 8192,
                disk_gb: 100,
              ),
              total_resources: types.Resources(
                cpu_cores: 4,
                memory_mb: 8192,
                disk_gb: 100,
              ),
              vm_ids: [],
              schedulable: True,
              tags: [],
              labels: dict.new(),
              created_at: int.to_string(erlang.system_time(erlang.Second)),
              updated_at: int.to_string(erlang.system_time(erlang.Second)),
            )

          // Store the host
          case khepri_store.put_host(store, new_host) {
            Ok(_) -> {
              io.println("Host created successfully!")
              start_agent(host_id, store)
            }
            Error(err) -> {
              io.println("Failed to create host: " <> safe_debug_error(err))
            }
          }
        }
      }
    }
    Error(err) -> {
      io.println("Failed to initialize Khepri store: " <> safe_debug_error(err))
    }
  }
}

// Helper function to start the agent with VM monitoring
fn start_agent(host_id: String, store: khepri_store.Khepri) -> Nil {
  // Start the host agent
  io.println("Starting host agent...")
  case agent.start(host_id, store) {
    Ok(agent_process) -> {
      io.println("Host agent started successfully!")

      // We'll skip actual process registration for now as it requires
      // a different approach in Gleam/Erlang
      io.println(
        "Host agent running as process ID: " <> string.inspect(agent_process),
      )

      // Start the VM watcher for automatic monitoring
      io.println("Starting VM status monitoring...")
      let watcher_result = vm_manager.start_vm_watcher(store)

      case watcher_result {
        Ok(_) -> {
          io.println("VM monitoring started successfully!")
          io.println("VMs will be checked every 30 seconds")
        }
        Error(err) -> {
          io.println(
            "Warning: Failed to start VM monitoring: " <> string.inspect(err),
          )
          io.println("VM state changes will not be automatically detected")
        }
      }

      io.println("Agent is ready to receive commands.")

      // Keep the application running indefinitely
      process.sleep_forever()
    }
    Error(err) -> {
      io.println("Failed to start host agent: " <> err)
    }
  }
}

fn run_normal() -> Nil {
  io.println("Starting Blixard - NixOS microVM orchestrator")

  // Start Khepri store
  io.println("Initializing Khepri store...")
  let store_result =
    khepri_store.start(["blixard@127.0.0.1"], "blixard_cluster")

  case store_result {
    Ok(store) -> {
      io.println("Khepri store initialized successfully!")

      // For now, just log that we're running
      io.println("Blixard initialized! This is a placeholder implementation.")
      io.println("\nOptions:")
      io.println("  --test-khepri                 : Run Khepri store tests")
      io.println("  --debug-ffi                   : Run FFI debugging tests")
      io.println(
        "  --host-agent <host_id> [flake]: Run a host agent (flake defaults to ./microvm_flakes)",
      )
      io.println(
        "  --test-agent-create-vm <host_id> <vm_name>: Create VM directly without agent process",
      )
      io.println("\nHost Agent Commands (for a running agent):")
      io.println("  --agent-list-vms              : List all VMs on the host")
      io.println(
        "  --agent-create-vm <name>      : Create a new VM from the flake",
      )
      io.println("  --agent-start-vm <id>         : Start a VM")
      io.println("  --agent-stop-vm <id>          : Stop a VM")
      io.println("  --agent-restart-vm <id>       : Restart a VM")
      io.println("  --agent-status                : Show host status")
      io.println("  --agent-metrics               : Show Prometheus metrics")
      io.println(
        "  --check-vms                   : Run a manual VM status check",
      )

      // Start the VM watcher for automatic monitoring
      io.println("\nStarting VM status monitoring...")
      let watcher_result = vm_manager.start_vm_watcher(store)

      case watcher_result {
        Ok(_) -> {
          io.println("VM monitoring started successfully!")
          io.println("VMs will be checked every 30 seconds")
        }
        Error(err) -> {
          io.println(
            "Warning: Failed to start VM monitoring: " <> string.inspect(err),
          )
          io.println("VM state changes will not be automatically detected")
        }
      }

      // Keep the application running indefinitely
      process.sleep_forever()
    }

    Error(err) -> {
      io.println("Failed to initialize Khepri store: " <> safe_debug_error(err))
    }
  }
}

/// Helper function to debug KhepriError with safer handling
fn safe_debug_error(error: khepri_store.KhepriError) -> String {
  let error_str = case error {
    khepri_store.ConnectionError(msg) ->
      "Connection error: " <> string.inspect(msg)
    khepri_store.ConsensusError(msg) ->
      "Consensus error: " <> string.inspect(msg)
    khepri_store.StorageError(msg) -> "Storage error: " <> string.inspect(msg)
    khepri_store.NotFound -> "Resource not found"
    khepri_store.InvalidData(msg) -> "Invalid data: " <> string.inspect(msg)
    _ -> "Unknown error: " <> string.inspect(error)
  }

  io.println("[KHEPRI ERROR] " <> error_str)
  error_str
}

// Helper function to generate a UUID
fn generate_uuid() -> String {
  // For simplicity, use timestamp + sequential number
  let timestamp = int.to_string(erlang.system_time(erlang.Second))
  // We don't have a good random function available, so use current time microseconds as "random"
  let pseudo_random =
    int.to_string(erlang.system_time(erlang.Microsecond) % 100_000)
  timestamp <> "-" <> pseudo_random
}
