//// src/blixard/host_agent/agent.gleam

///
/// Host agent that runs on each node to manage VMs
import blixard/domain/types
import blixard/storage/khepri_store.{type Khepri, type KhepriError}
import gleam/dict.{type Dict}
import gleam/erlang/process.{type Subject}
import gleam/io
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/otp/actor
import gleam/result
import gleam/string

// Commands that can be sent to the host agent
pub type Command {
  // VM lifecycle commands
  StartVM(vm_id: types.Uuid, reply_with: Subject(Result(Nil, String)))
  StopVM(vm_id: types.Uuid, reply_with: Subject(Result(Nil, String)))
  PauseVM(vm_id: types.Uuid, reply_with: Subject(Result(Nil, String)))
  ResumeVM(vm_id: types.Uuid, reply_with: Subject(Result(Nil, String)))

  // Host management commands
  UpdateResources(
    resources: types.Resources,
    reply_with: Subject(Result(Nil, String)),
  )
  SetSchedulable(schedulable: Bool, reply_with: Subject(Result(Nil, String)))

  // Monitoring
  GetStatus(reply_with: Subject(Result(HostStatus, String)))
}

// Host agent state
pub type State {
  State(
    host_id: types.Uuid,
    store: Khepri,
    vms: Dict(types.Uuid, VmProcess),
    resources: types.Resources,
    schedulable: Bool,
  )
}

// VM process information
pub type VmProcess {
  VmProcess(
    vm_id: types.Uuid,
    pid: Option(process.Pid),
    state: types.ResourceState,
  )
}

// Host status information
pub type HostStatus {
  HostStatus(
    host_id: types.Uuid,
    vm_count: Int,
    running_vms: List(types.Uuid),
    available_resources: types.Resources,
    schedulable: Bool,
  )
}

// Start the host agent
pub fn start(
  host_id: types.Uuid,
  store: Khepri,
) -> Result(Subject(Command), String) {
  // Retrieve host from the store
  let host_result = khepri_store.get_host(store, host_id)

  case host_result {
    Ok(host) -> {
      // Initialize state
      let initial_state =
        State(
          host_id: host_id,
          store: store,
          vms: dict.new(),
          resources: host.available_resources,
          schedulable: host.schedulable,
        )

      // Start actor
      case actor.start(initial_state, handle_message) {
        Ok(actor) -> Ok(actor)
        Error(err) -> Error("Failed to start actor: " <> debug_actor_error(err))
      }
    }

    Error(err) -> Error("Failed to retrieve host: " <> debug_error(err))
  }
}

fn debug_actor_error(err: actor.StartError) -> String {
  "Actor start error"
}

// Handle incoming messages
fn handle_message(message: Command, state: State) -> actor.Next(Command, State) {
  case message {
    StartVM(vm_id, reply_with) -> handle_start_vm(vm_id, reply_with, state)
    StopVM(vm_id, reply_with) -> handle_stop_vm(vm_id, reply_with, state)
    PauseVM(vm_id, reply_with) -> handle_pause_vm(vm_id, reply_with, state)
    ResumeVM(vm_id, reply_with) -> handle_resume_vm(vm_id, reply_with, state)
    UpdateResources(resources, reply_with) ->
      handle_update_resources(resources, reply_with, state)
    SetSchedulable(schedulable, reply_with) ->
      handle_set_schedulable(schedulable, reply_with, state)
    GetStatus(reply_with) -> handle_get_status(reply_with, state)
  }
}

// Handle start VM command
fn handle_start_vm(
  vm_id: types.Uuid,
  reply_with: Subject(Result(Nil, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Check if VM is already running
  case dict.get(state.vms, vm_id) {
    Ok(vm_process) -> {
      case vm_process.state {
        types.Running -> {
          process.send(reply_with, Error("VM is already running"))
          actor.continue(state)
        }

        _ -> start_vm_process(vm_id, reply_with, state)
      }
    }

    Error(_) -> {
      // VM is not in our local state, check if it's assigned to this host
      case khepri_store.get_vm(state.store, vm_id) {
        Ok(vm) -> {
          case vm.host_id {
            Some(id) -> {
              case id == state.host_id {
                True -> start_vm_process(vm_id, reply_with, state)
                False -> {
                  process.send(
                    reply_with,
                    Error("VM is not assigned to this host"),
                  )
                  actor.continue(state)
                }
              }
            }

            None -> {
              process.send(reply_with, Error("VM is not assigned to this host"))
              actor.continue(state)
            }
          }
        }

        Error(_) -> {
          process.send(reply_with, Error("VM not found"))
          actor.continue(state)
        }
      }
    }
  }
}

// Start a VM process
fn start_vm_process(
  vm_id: types.Uuid,
  reply_with: Subject(Result(Nil, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Implementation note: In a real system, this would involve calling into system APIs
  // to start the actual VM. For now, we'll simulate this process.

  // Update VM state in Khepri
  let state_result =
    khepri_store.update_vm_state(state.store, vm_id, types.Provisioning)

  case state_result {
    Ok(_) -> {
      // Simulate VM startup process
      io.println("Starting VM " <> vm_id <> " on host " <> state.host_id)

      // In a real implementation, we would:
      // 1. Pull NixOS config from cache
      // 2. Set up network with Tailscale
      // 3. Launch the microVM
      // For now, we'll just simulate a successful startup

      // Update VM state to Running
      case khepri_store.update_vm_state(state.store, vm_id, types.Running) {
        Ok(_) -> {
          // Create a simulated PID for the VM
          let new_vm_process =
            VmProcess(
              vm_id: vm_id,
              pid: Some(process.self()),
              // Using the agent's PID for simulation
              state: types.Running,
            )

          // Update local state
          let new_vms = dict.insert(state.vms, vm_id, new_vm_process)
          let new_state = State(..state, vms: new_vms)

          process.send(reply_with, Ok(Nil))
          actor.continue(new_state)
        }

        Error(err) -> {
          process.send(
            reply_with,
            Error("Failed to update VM state: " <> debug_error(err)),
          )
          actor.continue(state)
        }
      }
    }

    Error(err) -> {
      process.send(
        reply_with,
        Error("Failed to update VM state: " <> debug_error(err)),
      )
      actor.continue(state)
    }
  }
}

// Handle stop VM command
fn handle_stop_vm(
  vm_id: types.Uuid,
  reply_with: Subject(Result(Nil, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Check if VM is running
  case dict.get(state.vms, vm_id) {
    Ok(vm_process) -> {
      // Update VM state in Khepri
      let state_result =
        khepri_store.update_vm_state(state.store, vm_id, types.Stopping)

      case state_result {
        Ok(_) -> {
          // Simulate VM shutdown process
          io.println("Stopping VM " <> vm_id <> " on host " <> state.host_id)

          // Update VM state to Stopped
          case khepri_store.update_vm_state(state.store, vm_id, types.Stopped) {
            Ok(_) -> {
              // Remove VM from local state
              let new_vms = dict.delete(state.vms, vm_id)
              let new_state = State(..state, vms: new_vms)

              process.send(reply_with, Ok(Nil))
              actor.continue(new_state)
            }

            Error(err) -> {
              process.send(
                reply_with,
                Error("Failed to update VM state: " <> debug_error(err)),
              )
              actor.continue(state)
            }
          }
        }

        Error(err) -> {
          process.send(
            reply_with,
            Error("Failed to update VM state: " <> debug_error(err)),
          )
          actor.continue(state)
        }
      }
    }

    Error(_) -> {
      process.send(reply_with, Error("VM is not running on this host"))
      actor.continue(state)
    }
  }
}

// Handle pause VM command
fn handle_pause_vm(
  vm_id: types.Uuid,
  reply_with: Subject(Result(Nil, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Check if VM is running
  case dict.get(state.vms, vm_id) {
    Ok(vm_process) -> {
      case vm_process.state {
        types.Running -> {
          // Update VM state in Khepri
          let state_result =
            khepri_store.update_vm_state(state.store, vm_id, types.Paused)

          case state_result {
            Ok(_) -> {
              // Update local state
              let new_vm_process = VmProcess(..vm_process, state: types.Paused)
              let new_vms = dict.insert(state.vms, vm_id, new_vm_process)
              let new_state = State(..state, vms: new_vms)

              process.send(reply_with, Ok(Nil))
              actor.continue(new_state)
            }

            Error(err) -> {
              process.send(
                reply_with,
                Error("Failed to update VM state: " <> debug_error(err)),
              )
              actor.continue(state)
            }
          }
        }

        _ -> {
          process.send(reply_with, Error("VM is not in a running state"))
          actor.continue(state)
        }
      }
    }

    Error(_) -> {
      process.send(reply_with, Error("VM is not running on this host"))
      actor.continue(state)
    }
  }
}

// Handle resume VM command
fn handle_resume_vm(
  vm_id: types.Uuid,
  reply_with: Subject(Result(Nil, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Check if VM is paused
  case dict.get(state.vms, vm_id) {
    Ok(vm_process) -> {
      case vm_process.state {
        types.Paused -> {
          // Update VM state in Khepri
          let state_result =
            khepri_store.update_vm_state(state.store, vm_id, types.Running)

          case state_result {
            Ok(_) -> {
              // Update local state
              let new_vm_process = VmProcess(..vm_process, state: types.Running)
              let new_vms = dict.insert(state.vms, vm_id, new_vm_process)
              let new_state = State(..state, vms: new_vms)

              process.send(reply_with, Ok(Nil))
              actor.continue(new_state)
            }

            Error(err) -> {
              process.send(
                reply_with,
                Error("Failed to update VM state: " <> debug_error(err)),
              )
              actor.continue(state)
            }
          }
        }

        _ -> {
          process.send(reply_with, Error("VM is not in a paused state"))
          actor.continue(state)
        }
      }
    }

    Error(_) -> {
      process.send(reply_with, Error("VM is not running on this host"))
      actor.continue(state)
    }
  }
}

// Handle update resources command
fn handle_update_resources(
  resources: types.Resources,
  reply_with: Subject(Result(Nil, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Update host in Khepri
  let host_result = khepri_store.get_host(state.store, state.host_id)

  case host_result {
    Ok(host) -> {
      let updated_host = types.Host(..host, available_resources: resources)

      case khepri_store.put_host(state.store, updated_host) {
        Ok(_) -> {
          // Update local state
          let new_state = State(..state, resources: resources)

          process.send(reply_with, Ok(Nil))
          actor.continue(new_state)
        }

        Error(err) -> {
          process.send(
            reply_with,
            Error("Failed to update host: " <> debug_error(err)),
          )
          actor.continue(state)
        }
      }
    }

    Error(err) -> {
      process.send(
        reply_with,
        Error("Failed to retrieve host: " <> debug_error(err)),
      )
      actor.continue(state)
    }
  }
}

// Handle set schedulable command
fn handle_set_schedulable(
  schedulable: Bool,
  reply_with: Subject(Result(Nil, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Update host in Khepri
  let host_result = khepri_store.get_host(state.store, state.host_id)

  case host_result {
    Ok(host) -> {
      let updated_host = types.Host(..host, schedulable: schedulable)

      case khepri_store.put_host(state.store, updated_host) {
        Ok(_) -> {
          // Update local state
          let new_state = State(..state, schedulable: schedulable)

          process.send(reply_with, Ok(Nil))
          actor.continue(new_state)
        }

        Error(err) -> {
          process.send(
            reply_with,
            Error("Failed to update host: " <> debug_error(err)),
          )
          actor.continue(state)
        }
      }
    }

    Error(err) -> {
      process.send(
        reply_with,
        Error("Failed to retrieve host: " <> debug_error(err)),
      )
      actor.continue(state)
    }
  }
}

// Handle get status command
fn handle_get_status(
  reply_with: Subject(Result(HostStatus, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Get running VMs
  let running_vms =
    state.vms
    |> dict.filter(fn(_, vm_process) { vm_process.state == types.Running })
    |> dict.keys

  // Create status
  let status =
    HostStatus(
      host_id: state.host_id,
      vm_count: dict.size(state.vms),
      running_vms: running_vms,
      available_resources: state.resources,
      schedulable: state.schedulable,
    )

  process.send(reply_with, Ok(status))
  actor.continue(state)
}

/// Helper function to debug KhepriError
fn debug_error(error: KhepriError) -> String {
  case error {
    khepri_store.ConnectionError(msg) -> "Connection error: " <> msg
    khepri_store.ConsensusError(msg) -> "Consensus error: " <> msg
    khepri_store.StorageError(msg) -> "Storage error: " <> msg
    khepri_store.NotFound -> "Resource not found"
    khepri_store.InvalidData(msg) -> "Invalid data: " <> msg
  }
}
