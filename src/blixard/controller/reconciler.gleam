//// src/blixard/controller/reconciler.gleam

/// State reconciliation controller for microVMs
import blixard/config/json_reader.{type StateConfig, type VmStateConfig}
import blixard/domain/types
import blixard/host_agent/vm_manager
import blixard/storage/khepri_store.{type Khepri, type KhepriError}
import gleam/dict
import gleam/erlang
import gleam/erlang/process.{type Subject}
import gleam/int
import gleam/io
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/otp/actor
import gleam/result
import gleam/string
import shellout

/// Reconciler state
pub type ReconcilerState {
  ReconcilerState(
    store: Khepri,
    config_path: String,
    last_config: Option(StateConfig),
    subject: process.Subject(ReconcilerMessage),
  )
}

/// Reconciler messages
pub type ReconcilerMessage {
  Reconcile
  ReloadConfig
  Stop
}

/// Start the reconciler process
pub fn start(
  store: Khepri,
  config_path: String,
) -> Result(process.Subject(ReconcilerMessage), actor.StartError) {
  io.println("[RECONCILER] Starting state reconciler process...")
  io.println("[RECONCILER] Using config path: " <> config_path)

  // Test if config file exists
  case
    shellout.command(run: "test", with: ["-f", config_path], in: ".", opt: [])
  {
    Ok(_) -> io.println("[RECONCILER] Config file exists at: " <> config_path)
    Error(_) ->
      io.println(
        "[RECONCILER][WARNING] Config file not found at: " <> config_path,
      )
  }

  // Try to read the config file directly
  io.println("[RECONCILER] Trying to read config file contents...")
  case shellout.command(run: "cat", with: [config_path], in: ".", opt: []) {
    Ok(content) -> {
      io.println("[RECONCILER] Config file contents:")
      io.println(content)
    }
    Error(err) -> {
      let #(_, message) = err
      io.println("[RECONCILER][ERROR] Failed to read config file: " <> message)
    }
  }

  // Create a new subject for self-messaging
  let subject = process.new_subject()
  io.println("[RECONCILER] Created process subject for self-messaging")

  // Create the initial state
  let state =
    ReconcilerState(
      store: store,
      config_path: config_path,
      last_config: None,
      subject: subject,
    )
  io.println("[RECONCILER] Initial state created - no config loaded yet")

  // Start the actor
  io.println("[RECONCILER] Starting actor process...")
  let actor_result = actor.start(state, handle_message)

  case actor_result {
    Ok(pid) -> {
      // Initialize by sending first reconcile message
      io.println(
        "[RECONCILER] Actor started successfully with PID: "
        <> string.inspect(pid),
      )
      io.println(
        "[RECONCILER] Sending initial reconcile message to trigger first cycle",
      )
      process.send(subject, Reconcile)
      io.println("[RECONCILER] State reconciler initialization complete")
      Ok(subject)
    }
    Error(reason) as error -> {
      io.println(
        "[RECONCILER][ERROR] Failed to start reconciler actor: "
        <> string.inspect(reason),
      )
      error
    }
  }
}

/// Handle incoming messages
// In reconciler.gleam, update the handle_message function to be extremely simplified:

fn handle_message(
  message: ReconcilerMessage,
  state: ReconcilerState,
) -> actor.Next(ReconcilerMessage, ReconcilerState) {
  // Super simple debug logging
  io.println("[RECONCILER] **********************************")
  io.println("[RECONCILER] MESSAGE RECEIVED")

  case message {
    Reconcile -> {
      io.println("[RECONCILER] Message type: RECONCILE")
      // Skip normal handling for now, just log and continue
      let timer_ref =
        process.send_after(
          state.subject,
          60_000,
          // 60 seconds
          Reconcile,
        )
      io.println(
        "[RECONCILER] Scheduled next message with timer ref: "
        <> string.inspect(timer_ref),
      )
      io.println("[RECONCILER] **********************************")
      actor.continue(state)
    }
    ReloadConfig -> {
      io.println("[RECONCILER] Message type: RELOAD_CONFIG")
      io.println("[RECONCILER] **********************************")
      actor.continue(state)
    }
    Stop -> {
      io.println("[RECONCILER] Message type: STOP")
      io.println("[RECONCILER] **********************************")
      actor.Stop(process.Normal)
    }
  }
}

/// Handle reconcile message
fn handle_reconcile(
  state: ReconcilerState,
) -> actor.Next(ReconcilerMessage, ReconcilerState) {
  io.println("[RECONCILER] DEBUG: handle_reconcile called")
  io.println(
    "[RECONCILER] ========== STARTING STATE RECONCILIATION CYCLE ==========",
  )
  io.println("[RECONCILER] Checking if config is already loaded...")

  // Load config if not already loaded
  let config_result = case state.last_config {
    Some(config) -> {
      io.println(
        "[RECONCILER] Using previously loaded config with "
        <> int.to_string(list.length(config.vms))
        <> " VMs",
      )
      Ok(config)
    }
    None -> {
      io.println(
        "[RECONCILER] No config loaded - loading from: " <> state.config_path,
      )
      // Try to parse the config file
      let result = json_reader.parse_config_file(state.config_path)
      io.println(
        "[RECONCILER] Config file parse result: " <> string.inspect(result),
      )
      result
    }
  }

  case config_result {
    Ok(config) -> {
      io.println("[RECONCILER] Config content:")
      io.println(
        "[RECONCILER] - VMs: " <> int.to_string(list.length(config.vms)),
      )
      io.println(
        "[RECONCILER] - Default flake path: " <> config.default_flake_path,
      )
      io.println(
        "[RECONCILER] - Reconcile interval: "
        <> int.to_string(config.reconcile_interval_sec),
      )

      // Log VM configs
      case list.length(config.vms) {
        0 -> io.println("[RECONCILER] WARNING: No VMs defined in config")
        _ -> {
          list.each(config.vms, fn(vm) {
            io.println("[RECONCILER] VM: " <> vm.name)
            io.println(
              "[RECONCILER]  - Desired state: "
              <> string.inspect(vm.desired_state),
            )
            case vm.host_id {
              Some(host) -> io.println("[RECONCILER]  - Host ID: " <> host)
              None -> io.println("[RECONCILER]  - Host ID: none")
            }
            case vm.resources {
              Some(res) ->
                io.println(
                  "[RECONCILER]  - Resources: CPU="
                  <> int.to_string(res.cpu_cores)
                  <> ", Memory="
                  <> int.to_string(res.memory_mb)
                  <> "MB, "
                  <> "Disk="
                  <> int.to_string(res.disk_gb)
                  <> "GB",
                )
              None -> io.println("[RECONCILER]  - Resources: default")
            }
          })
        }
      }

      // Reconcile each VM
      io.println(
        "[RECONCILER] Starting reconciliation for "
        <> int.to_string(list.length(config.vms))
        <> " VMs...",
      )

      list.each(config.vms, fn(vm_config) {
        io.println("[RECONCILER] ----------------------------------------")
        io.println("[RECONCILER] Reconciling VM: " <> vm_config.name)
        reconcile_vm(state.store, vm_config)
        io.println(
          "[RECONCILER] Completed reconciliation for VM: " <> vm_config.name,
        )
      })

      // Schedule next reconciliation
      io.println(
        "[RECONCILER] Scheduling next reconciliation in "
        <> int.to_string(config.reconcile_interval_sec)
        <> " seconds",
      )

      let timer_ref =
        process.send_after(
          state.subject,
          config.reconcile_interval_sec * 1000,
          Reconcile,
        )

      io.println(
        "[RECONCILER] Next reconciliation scheduled with timer ref: "
        <> string.inspect(timer_ref),
      )
      io.println(
        "[RECONCILER] ========== COMPLETED RECONCILIATION CYCLE ==========",
      )

      // Update state with loaded config
      actor.continue(ReconcilerState(..state, last_config: Some(config)))
    }
    Error(err) -> {
      io.println("[RECONCILER][ERROR] Failed to load configuration: " <> err)
      io.println("[RECONCILER] Will retry in 60 seconds")

      // Try again after a minute
      let timer_ref = process.send_after(state.subject, 60_000, Reconcile)
      io.println(
        "[RECONCILER] Retry scheduled with timer ref: "
        <> string.inspect(timer_ref),
      )
      io.println(
        "[RECONCILER] ========== RECONCILIATION CYCLE FAILED ==========",
      )

      actor.continue(state)
    }
  }
}

/// Handle reload config message
fn handle_reload_config(
  state: ReconcilerState,
) -> actor.Next(ReconcilerMessage, ReconcilerState) {
  io.println("[RECONCILER] ========== HANDLING CONFIG RELOAD ==========")
  io.println("[RECONCILER] Forcing config reload by clearing cached config")

  // Force reload by clearing last_config
  let new_state = ReconcilerState(..state, last_config: None)

  // Trigger immediate reconciliation
  io.println("[RECONCILER] Triggering immediate reconciliation with new config")
  process.send(state.subject, Reconcile)
  io.println("[RECONCILER] Config reload process completed")

  actor.continue(new_state)
}

/// Reconcile a single VM's state
fn reconcile_vm(store: Khepri, vm_config: VmStateConfig) -> Nil {
  io.println(
    "[RECONCILER][VM:" <> vm_config.name <> "] Starting reconciliation",
  )
  io.println(
    "[RECONCILER][VM:"
    <> vm_config.name
    <> "] ========== DESIRED STATE ==========",
  )
  io.println(
    "[RECONCILER][VM:"
    <> vm_config.name
    <> "] VM State: "
    <> string.inspect(vm_config.desired_state),
  )

  // Log host information if present
  case vm_config.host_id {
    Some(host_id) -> {
      io.println("[RECONCILER][VM:" <> vm_config.name <> "] Host: " <> host_id)
    }
    None -> {
      io.println("[RECONCILER][VM:" <> vm_config.name <> "] Host: none")
    }
  }

  // Log resources if present
  case vm_config.resources {
    Some(res) -> {
      io.println(
        "[RECONCILER][VM:"
        <> vm_config.name
        <> "] Resources: CPU="
        <> int.to_string(res.cpu_cores)
        <> ", Memory="
        <> int.to_string(res.memory_mb)
        <> "MB, Disk="
        <> int.to_string(res.disk_gb)
        <> "GB",
      )
    }
    None -> {
      io.println(
        "[RECONCILER][VM:" <> vm_config.name <> "] Resources: Using defaults",
      )
    }
  }

  // Log flake path if present
  case vm_config.flake_path {
    Some(path) -> {
      io.println(
        "[RECONCILER][VM:" <> vm_config.name <> "] Flake Path: " <> path,
      )
    }
    None -> {
      io.println(
        "[RECONCILER][VM:" <> vm_config.name <> "] Flake Path: Using default",
      )
    }
  }

  io.println(
    "[RECONCILER][VM:"
    <> vm_config.name
    <> "] Auto-restart: "
    <> string.inspect(vm_config.auto_restart),
  )
  io.println(
    "[RECONCILER][VM:"
    <> vm_config.name
    <> "] ========== END DESIRED STATE ==========",
  )

  // Find VM by name in Khepri
  io.println(
    "[RECONCILER][VM:" <> vm_config.name <> "] Looking up VM in Khepri store...",
  )
  let find_result = find_vm_by_name(store, vm_config.name)

  case find_result {
    Ok(vm) -> {
      io.println(
        "[RECONCILER][VM:"
        <> vm_config.name
        <> "] VM found in Khepri with ID: "
        <> vm.id,
      )
      io.println(
        "[RECONCILER][VM:"
        <> vm_config.name
        <> "] ========== CURRENT STATE ==========",
      )
      io.println(
        "[RECONCILER][VM:"
        <> vm_config.name
        <> "] VM State: "
        <> string.inspect(vm.state),
      )

      case vm.host_id {
        Some(host) ->
          io.println("[RECONCILER][VM:" <> vm_config.name <> "] Host: " <> host)
        None ->
          io.println("[RECONCILER][VM:" <> vm_config.name <> "] Host: none")
      }

      let res = vm.resources
      io.println(
        "[RECONCILER][VM:"
        <> vm_config.name
        <> "] Resources: CPU="
        <> int.to_string(res.cpu_cores)
        <> ", Memory="
        <> int.to_string(res.memory_mb)
        <> "MB, Disk="
        <> int.to_string(res.disk_gb)
        <> "GB",
      )

      io.println(
        "[RECONCILER][VM:"
        <> vm_config.name
        <> "] Flake Path: "
        <> vm.nixos_config.config_path,
      )

      io.println(
        "[RECONCILER][VM:"
        <> vm_config.name
        <> "] ========== END CURRENT STATE ==========",
      )

      // VM exists, check if state matches
      io.println(
        "[RECONCILER][VM:" <> vm_config.name <> "] Reconciling existing VM...",
      )
      reconcile_existing_vm(store, vm, vm_config)
    }
    Error(err) -> {
      io.println(
        "[RECONCILER][VM:" <> vm_config.name <> "][ERROR] VM not found: " <> err,
      )

      // VM doesn't exist, create if desired
      case vm_config.desired_state == types.Stopped {
        False -> {
          io.println(
            "[RECONCILER][VM:"
            <> vm_config.name
            <> "] VM doesn't exist but desired state is not Stopped - creating VM",
          )
          create_new_vm(store, vm_config)
        }
        True -> {
          io.println(
            "[RECONCILER][VM:"
            <> vm_config.name
            <> "] VM doesn't exist and desired state is Stopped - nothing to do",
          )
        }
      }
    }
  }

  io.println(
    "[RECONCILER][VM:" <> vm_config.name <> "] Reconciliation complete",
  )
  Nil
}

/// Reconcile an existing VM
fn reconcile_existing_vm(
  store: Khepri,
  vm: types.MicroVm,
  vm_config: VmStateConfig,
) -> Nil {
  io.println(
    "[RECONCILER][VM:" <> vm.name <> "] ========== STATE COMPARISON ==========",
  )

  // Compare VM state
  io.println(
    "[RECONCILER][VM:"
    <> vm.name
    <> "] STATE DIFF: Current="
    <> string.inspect(vm.state)
    <> ", Desired="
    <> string.inspect(vm_config.desired_state)
    <> case vm.state == vm_config.desired_state {
      True -> " [MATCH]"
      False -> " [MISMATCH]"
    },
  )

  // Compare host assignment
  let current_host_str = case vm.host_id {
    Some(id) -> "'" <> id <> "'"
    None -> "none"
  }

  let desired_host_str = case vm_config.host_id {
    Some(id) -> "'" <> id <> "'"
    None -> "none"
  }

  let host_comparison = case vm.host_id, vm_config.host_id {
    None, None -> " [MATCH]"
    Some(id1), Some(id2) ->
      case id1 == id2 {
        True -> " [MATCH]"
        False -> " [MISMATCH]"
      }
    _, _ -> " [MISMATCH]"
  }

  io.println(
    "[RECONCILER][VM:"
    <> vm.name
    <> "] HOST DIFF: Current="
    <> current_host_str
    <> ", Desired="
    <> desired_host_str
    <> host_comparison,
  )

  // Compare resources if specified in config
  case vm_config.resources {
    Some(desired_resources) -> {
      let current_resources = vm.resources

      // Compare CPU cores
      io.println(
        "[RECONCILER][VM:"
        <> vm.name
        <> "] CPU CORES DIFF: Current="
        <> int.to_string(current_resources.cpu_cores)
        <> ", Desired="
        <> int.to_string(desired_resources.cpu_cores)
        <> case current_resources.cpu_cores == desired_resources.cpu_cores {
          True -> " [MATCH]"
          False -> " [MISMATCH]"
        },
      )

      // Compare memory
      io.println(
        "[RECONCILER][VM:"
        <> vm.name
        <> "] MEMORY DIFF: Current="
        <> int.to_string(current_resources.memory_mb)
        <> "MB, Desired="
        <> int.to_string(desired_resources.memory_mb)
        <> "MB"
        <> case current_resources.memory_mb == desired_resources.memory_mb {
          True -> " [MATCH]"
          False -> " [MISMATCH]"
        },
      )

      // Compare disk
      io.println(
        "[RECONCILER][VM:"
        <> vm.name
        <> "] DISK DIFF: Current="
        <> int.to_string(current_resources.disk_gb)
        <> "GB, Desired="
        <> int.to_string(desired_resources.disk_gb)
        <> "GB"
        <> case current_resources.disk_gb == desired_resources.disk_gb {
          True -> " [MATCH]"
          False -> " [MISMATCH]"
        },
      )
    }
    None -> {
      io.println(
        "[RECONCILER][VM:"
        <> vm.name
        <> "] RESOURCES DIFF: No specific resources requested in config",
      )
    }
  }

  // Compare flake path
  let current_flake_path = vm.nixos_config.config_path
  let desired_flake_path = case vm_config.flake_path {
    Some(path) -> path
    None -> "./microvm_flakes"
    // Default path
  }

  io.println(
    "[RECONCILER][VM:"
    <> vm.name
    <> "] FLAKE PATH DIFF: Current="
    <> current_flake_path
    <> ", Desired="
    <> desired_flake_path
    <> case current_flake_path == desired_flake_path {
      True -> " [MATCH]"
      False -> " [MISMATCH]"
    },
  )

  // Compare auto-restart setting
  io.println(
    "[RECONCILER][VM:"
    <> vm.name
    <> "] AUTO-RESTART DIFF: Config Setting="
    <> string.inspect(vm_config.auto_restart),
  )

  io.println(
    "[RECONCILER][VM:"
    <> vm.name
    <> "] ========== END STATE COMPARISON ==========",
  )

  // Check if host assignment matches
  io.println("[RECONCILER][VM:" <> vm.name <> "] Checking host assignment...")
  reconcile_host_assignment(store, vm, vm_config)

  // Check if state matches
  io.println("[RECONCILER][VM:" <> vm.name <> "] Checking state...")
  case vm.state, vm_config.desired_state {
    current, desired if current == desired -> {
      io.println(
        "[RECONCILER][VM:"
        <> vm.name
        <> "] Current state matches desired state - no action needed",
      )
      Nil
    }

    // VM is stopped but should be running
    types.Stopped, types.Running -> {
      io.println(
        "[RECONCILER][VM:"
        <> vm.name
        <> "] VM is STOPPED but should be RUNNING - initiating start operation",
      )

      case vm_manager.start_vm(vm.name) {
        Ok(_) ->
          io.println(
            "[RECONCILER][VM:" <> vm.name <> "] Start command sent successfully",
          )
        Error(err) ->
          io.println(
            "[RECONCILER][VM:"
            <> vm.name
            <> "][ERROR] Failed to start VM: "
            <> err,
          )
      }
      Nil
    }

    // VM is running but should be stopped
    types.Running, types.Stopped -> {
      io.println(
        "[RECONCILER][VM:"
        <> vm.name
        <> "] VM is RUNNING but should be STOPPED - initiating stop operation",
      )

      case vm_manager.stop_vm(vm.name) {
        Ok(_) ->
          io.println(
            "[RECONCILER][VM:" <> vm.name <> "] Stop command sent successfully",
          )
        Error(err) ->
          io.println(
            "[RECONCILER][VM:"
            <> vm.name
            <> "][ERROR] Failed to stop VM: "
            <> err,
          )
      }
      Nil
    }

    // VM is in another state that needs correction
    _, desired -> {
      io.println(
        "[RECONCILER][VM:"
        <> vm.name
        <> "] VM is in state "
        <> string.inspect(vm.state)
        <> " but should be "
        <> string.inspect(desired)
        <> " - attempting correction",
      )

      case desired {
        types.Running -> {
          io.println(
            "[RECONCILER][VM:"
            <> vm.name
            <> "] Trying to ensure VM reaches RUNNING state",
          )

          case vm_manager.start_vm(vm.name) {
            Ok(_) ->
              io.println(
                "[RECONCILER][VM:"
                <> vm.name
                <> "] Start command sent successfully",
              )
            Error(err) ->
              io.println(
                "[RECONCILER][VM:"
                <> vm.name
                <> "][ERROR] Failed to start VM: "
                <> err,
              )
          }
        }
        types.Stopped -> {
          io.println(
            "[RECONCILER][VM:"
            <> vm.name
            <> "] Trying to ensure VM reaches STOPPED state",
          )

          case vm_manager.stop_vm(vm.name) {
            Ok(_) ->
              io.println(
                "[RECONCILER][VM:"
                <> vm.name
                <> "] Stop command sent successfully",
              )
            Error(err) ->
              io.println(
                "[RECONCILER][VM:"
                <> vm.name
                <> "][ERROR] Failed to stop VM: "
                <> err,
              )
          }
        }
        _ -> {
          io.println(
            "[RECONCILER][VM:"
            <> vm.name
            <> "][WARNING] State "
            <> string.inspect(desired)
            <> " requires manual intervention",
          )
          io.println(
            "[RECONCILER][VM:"
            <> vm.name
            <> "] Automatic transition to this state is not supported",
          )
        }
      }
      Nil
    }
  }
}

/// Reconcile host assignment
fn reconcile_host_assignment(
  store: Khepri,
  vm: types.MicroVm,
  vm_config: VmStateConfig,
) -> Nil {
  io.println(
    "[RECONCILER][VM:"
    <> vm.name
    <> "] Checking host assignment configuration...",
  )

  // Log current and desired host assignments
  let current_host_str = case vm.host_id {
    Some(id) -> "'" <> id <> "'"
    None -> "none"
  }

  let desired_host_str = case vm_config.host_id {
    Some(id) -> "'" <> id <> "'"
    None -> "none"
  }

  io.println(
    "[RECONCILER][VM:"
    <> vm.name
    <> "] Current host: "
    <> current_host_str
    <> ", Desired host: "
    <> desired_host_str,
  )

  // Restructured fully exhaustive pattern matching
  case vm.host_id, vm_config.host_id {
    // Case 1: VM has no host but config specifies one
    None, Some(desired_host_id) -> {
      io.println(
        "[RECONCILER][VM:"
        <> vm.name
        <> "] VM has no host but should be assigned to host '"
        <> desired_host_id
        <> "' - assigning host",
      )

      case khepri_store.assign_vm_to_host(store, vm.id, desired_host_id) {
        Ok(_) ->
          io.println(
            "[RECONCILER][VM:"
            <> vm.name
            <> "] Host assignment successful to host '"
            <> desired_host_id
            <> "'",
          )
        Error(err) ->
          io.println(
            "[RECONCILER][VM:"
            <> vm.name
            <> "][ERROR] Failed to assign host: "
            <> khepri_store.debug_error(err),
          )
      }
      Nil
    }

    // Case 2: VM has no host and config doesn't specify one
    None, None -> {
      io.println(
        "[RECONCILER][VM:"
        <> vm.name
        <> "] No host assignment in VM or config - no action needed",
      )
      Nil
    }

    // Case 3: VM has a host but config doesn't specify one
    Some(host_id), None -> {
      io.println(
        "[RECONCILER][VM:"
        <> vm.name
        <> "][INFO] VM is assigned to host '"
        <> host_id
        <> "' but config doesn't specify a host - keeping current assignment",
      )
      Nil
    }

    // Case 4: VM has a host and config specifies a host (same or different)
    Some(current_host_id), Some(desired_host_id) -> {
      // Need to compare without guard expressions
      case current_host_id == desired_host_id {
        True -> {
          io.println(
            "[RECONCILER][VM:"
            <> vm.name
            <> "] VM is already on the correct host '"
            <> current_host_id
            <> "' - no action needed",
          )
        }
        False -> {
          io.println(
            "[RECONCILER][VM:"
            <> vm.name
            <> "][WARNING] VM is on host '"
            <> current_host_id
            <> "' but should be on host '"
            <> desired_host_id
            <> "'",
          )
          io.println(
            "[RECONCILER][VM:"
            <> vm.name
            <> "][WARNING] Migration between hosts is not yet implemented",
          )
          io.println(
            "[RECONCILER][VM:"
            <> vm.name
            <> "] Manual intervention required to migrate VM",
          )
        }
      }
      Nil
    }
  }
}

/// Create a new VM based on config
fn create_new_vm(store: Khepri, vm_config: VmStateConfig) -> Nil {
  io.println(
    "[RECONCILER][VM:" <> vm_config.name <> "] ===== CREATING NEW VM =====",
  )
  io.println(
    "[RECONCILER][VM:"
    <> vm_config.name
    <> "] Desired state: "
    <> string.inspect(vm_config.desired_state),
  )

  // Get flake path
  let flake_path = case vm_config.flake_path {
    Some(path) -> {
      io.println(
        "[RECONCILER][VM:"
        <> vm_config.name
        <> "] Using specified flake path: "
        <> path,
      )
      path
    }
    None -> {
      let default_path = "./microvm_flakes"
      io.println(
        "[RECONCILER][VM:"
        <> vm_config.name
        <> "] No flake path specified - using default: "
        <> default_path,
      )
      default_path
    }
  }

  // Resources to use
  let resources = case vm_config.resources {
    Some(res) -> {
      io.println(
        "[RECONCILER][VM:"
        <> vm_config.name
        <> "] Using specified resources - CPU cores: "
        <> int.to_string(res.cpu_cores)
        <> ", Memory: "
        <> int.to_string(res.memory_mb)
        <> "MB, "
        <> "Disk: "
        <> int.to_string(res.disk_gb)
        <> "GB",
      )
      res
    }
    None -> {
      let default_res =
        types.Resources(cpu_cores: 2, memory_mb: 2048, disk_gb: 20)
      io.println(
        "[RECONCILER][VM:"
        <> vm_config.name
        <> "] No resources specified - using defaults - CPU cores: 2, Memory: 2048MB, Disk: 20GB",
      )
      default_res
    }
  }

  // Generate a unique ID for the VM
  let vm_id = generate_uuid()
  io.println(
    "[RECONCILER][VM:" <> vm_config.name <> "] Generated VM ID: " <> vm_id,
  )

  // Create a VM record
  io.println("[RECONCILER][VM:" <> vm_config.name <> "] Creating VM record")

  let vm =
    types.MicroVm(
      id: vm_id,
      name: vm_config.name,
      description: Some("Created by state reconciler"),
      vm_type: types.Persistent,
      resources: resources,
      state: types.Pending,
      host_id: vm_config.host_id,
      storage_volumes: [],
      network_interfaces: [],
      tailscale_config: types.TailscaleConfig(
        enabled: True,
        auth_key: None,
        hostname: vm_config.name,
        tags: [],
        direct_client: True,
      ),
      nixos_config: types.NixosConfig(
        config_path: flake_path,
        overrides: dict.new(),
        cache_url: None,
      ),
      labels: dict.new(),
      created_at: int.to_string(erlang.system_time(erlang.Second)),
      updated_at: int.to_string(erlang.system_time(erlang.Second)),
    )

  // Store the VM in Khepri
  io.println(
    "[RECONCILER][VM:"
    <> vm_config.name
    <> "] Storing VM record in Khepri database...",
  )

  case khepri_store.put_vm(store, vm) {
    Ok(_) -> {
      io.println(
        "[RECONCILER][VM:"
        <> vm_config.name
        <> "] VM record stored successfully in Khepri",
      )

      // Create the VM using microvm.nix
      io.println(
        "[RECONCILER][VM:"
        <> vm_config.name
        <> "] Creating VM using flake at: "
        <> flake_path,
      )

      case vm_manager.create_vm(vm.name, flake_path) {
        Ok(_) -> {
          io.println(
            "[RECONCILER][VM:" <> vm_config.name <> "] VM created successfully",
          )

          // If desired state is Running, start it
          case vm_config.desired_state == types.Running {
            True -> {
              io.println(
                "[RECONCILER][VM:"
                <> vm_config.name
                <> "] Desired state is RUNNING - starting VM...",
              )

              case vm_manager.start_vm(vm.name) {
                Ok(_) ->
                  io.println(
                    "[RECONCILER][VM:"
                    <> vm_config.name
                    <> "] VM start command sent successfully",
                  )
                Error(err) ->
                  io.println(
                    "[RECONCILER][VM:"
                    <> vm_config.name
                    <> "][ERROR] Failed to start VM: "
                    <> err,
                  )
              }
            }
            False -> {
              io.println(
                "[RECONCILER][VM:"
                <> vm_config.name
                <> "] Desired state is not RUNNING - VM will remain in initial state",
              )
            }
          }
        }
        Error(err) -> {
          io.println(
            "[RECONCILER][VM:"
            <> vm_config.name
            <> "][ERROR] Failed to create VM: "
            <> err,
          )
        }
      }
    }
    Error(err) -> {
      io.println(
        "[RECONCILER][VM:"
        <> vm_config.name
        <> "][ERROR] Failed to store VM in Khepri: "
        <> khepri_store.debug_error(err),
      )
    }
  }

  io.println(
    "[RECONCILER][VM:" <> vm_config.name <> "] VM creation process completed",
  )
  Nil
}

/// Helper function to find a VM by name
fn find_vm_by_name(store: Khepri, name: String) -> Result(types.MicroVm, String) {
  io.println(
    "[RECONCILER][VM:" <> name <> "] Listing all VMs from Khepri store...",
  )

  // Get all VMs
  let vms_result = khepri_store.list_vms(store)

  case vms_result {
    Ok(vms) -> {
      io.println(
        "[RECONCILER][VM:"
        <> name
        <> "] Found "
        <> int.to_string(list.length(vms))
        <> " VMs in Khepri",
      )

      // Find VM by name
      io.println("[RECONCILER][VM:" <> name <> "] Searching for VM by name...")
      let find_result = list.find(vms, fn(vm) { vm.name == name })

      case find_result {
        Ok(vm) -> {
          io.println(
            "[RECONCILER][VM:" <> name <> "] VM found with ID: " <> vm.id,
          )
          Ok(vm)
        }
        Error(_) -> {
          io.println(
            "[RECONCILER][VM:" <> name <> "] VM not found in Khepri store",
          )
          Error("VM with name '" <> name <> "' not found")
        }
      }
    }
    Error(err) -> {
      io.println(
        "[RECONCILER][VM:"
        <> name
        <> "][ERROR] Failed to list VMs from Khepri: "
        <> khepri_store.debug_error(err),
      )
      Error("Failed to list VMs: " <> khepri_store.debug_error(err))
    }
  }
}

/// Generate a UUID
fn generate_uuid() -> String {
  // For simplicity, use timestamp + sequential number
  let timestamp = int.to_string(erlang.system_time(erlang.Second))
  // Use current time microseconds as "random"
  let pseudo_random =
    int.to_string(erlang.system_time(erlang.Microsecond) % 100_000)

  let uuid = timestamp <> "-" <> pseudo_random
  io.println("[RECONCILER] Generated UUID: " <> uuid)
  uuid
}
