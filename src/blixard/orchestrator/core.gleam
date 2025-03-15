//// src/blixard/orchestrator/core.gleam

///
/// Orchestrator core that coordinates all components
import blixard/domain/types
import blixard/host_agent/agent.{type Command as HostCommand}
import blixard/scheduler/scheduler.{type SchedulingStrategy}
import blixard/storage/khepri_store.{type Khepri, type KhepriError}
import gleam/dict.{type Dict}
import gleam/dynamic
import gleam/erlang/process.{type Subject}
import gleam/io
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/otp/actor
import gleam/otp/supervisor
import gleam/result
import gleam/string

// Orchestrator commands
pub type Command {
  // VM management
  CreateVM(
    vm: types.MicroVm,
    reply_with: Subject(Result(types.MicroVm, String)),
  )
  DeleteVM(vm_id: types.Uuid, reply_with: Subject(Result(Nil, String)))
  StartVM(vm_id: types.Uuid, reply_with: Subject(Result(Nil, String)))
  StopVM(vm_id: types.Uuid, reply_with: Subject(Result(Nil, String)))
  PauseVM(vm_id: types.Uuid, reply_with: Subject(Result(Nil, String)))
  ResumeVM(vm_id: types.Uuid, reply_with: Subject(Result(Nil, String)))

  // Host management
  RegisterHost(
    host: types.Host,
    reply_with: Subject(Result(types.Host, String)),
  )
  DeregisterHost(host_id: types.Uuid, reply_with: Subject(Result(Nil, String)))

  // Monitoring
  GetSystemStatus(reply_with: Subject(Result(SystemStatus, String)))
}

// Orchestrator state
pub type State {
  State(
    store: Khepri,
    hosts: Dict(types.Uuid, Subject(HostCommand)),
    scheduling_strategy: SchedulingStrategy,
  )
}

// System status
pub type SystemStatus {
  SystemStatus(
    vm_count: Int,
    host_count: Int,
    vms_by_state: Dict(types.ResourceState, Int),
  )
}

// Start the orchestrator
pub fn start(store: Khepri) -> Result(Subject(Command), String) {
  // Initialize state
  let initial_state =
    State(
      store: store,
      hosts: dict.new(),
      scheduling_strategy: scheduler.MostAvailable,
    )

  // Start actor
  case actor.start(initial_state, handle_message) {
    Ok(actor) -> Ok(actor)
    Error(err) -> Error("Failed to start actor: " <> debug_actor_error(err))
  }
}

fn debug_actor_error(err: actor.StartError) -> String {
  "Actor start error"
}

// Handle incoming messages
fn handle_message(message: Command, state: State) -> actor.Next(Command, State) {
  case message {
    CreateVM(vm, reply_with) -> handle_create_vm(vm, reply_with, state)
    DeleteVM(vm_id, reply_with) -> handle_delete_vm(vm_id, reply_with, state)
    StartVM(vm_id, reply_with) -> handle_start_vm(vm_id, reply_with, state)
    StopVM(vm_id, reply_with) -> handle_stop_vm(vm_id, reply_with, state)
    PauseVM(vm_id, reply_with) -> handle_pause_vm(vm_id, reply_with, state)
    ResumeVM(vm_id, reply_with) -> handle_resume_vm(vm_id, reply_with, state)
    RegisterHost(host, reply_with) ->
      handle_register_host(host, reply_with, state)
    DeregisterHost(host_id, reply_with) ->
      handle_deregister_host(host_id, reply_with, state)
    GetSystemStatus(reply_with) -> handle_get_system_status(reply_with, state)
  }
}

// Handle create VM command
fn handle_create_vm(
  vm: types.MicroVm,
  reply_with: Subject(Result(types.MicroVm, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Validate VM
  case validate_vm(vm) {
    Ok(_) -> {
      // Set initial state to Pending
      let vm_with_state = types.MicroVm(..vm, state: types.Pending)

      // Store VM in Khepri
      case khepri_store.put_vm(state.store, vm_with_state) {
        Ok(_) -> {
          // Schedule VM on a host
          case
            scheduler.schedule_vm(state.store, vm.id, state.scheduling_strategy)
          {
            scheduler.Scheduled(vm_id, host_id) -> {
              // VM successfully scheduled
              process.send(reply_with, Ok(vm_with_state))
              actor.continue(state)
            }

            scheduler.NoSuitableHost(reason) -> {
              process.send(
                reply_with,
                Error("No suitable host found: " <> reason),
              )
              actor.continue(state)
            }

            scheduler.SchedulingError(error) -> {
              process.send(reply_with, Error("Scheduling error: " <> error))
              actor.continue(state)
            }
          }
        }

        Error(err) -> {
          process.send(
            reply_with,
            Error("Failed to store VM: " <> debug_error(err)),
          )
          actor.continue(state)
        }
      }
    }

    Error(reason) -> {
      process.send(reply_with, Error("Invalid VM configuration: " <> reason))
      actor.continue(state)
    }
  }
}

// Validate VM configuration
fn validate_vm(vm: types.MicroVm) -> Result(Nil, String) {
  // Check for required fields
  case vm.name, vm.resources {
    "", _ -> Error("VM name cannot be empty")
    _, resources if resources.cpu_cores <= 0 ->
      Error("CPU cores must be positive")
    _, resources if resources.memory_mb <= 0 -> Error("Memory must be positive")
    _, resources if resources.disk_gb <= 0 ->
      Error("Disk space must be positive")
    _, _ -> Ok(Nil)
  }
}

// Handle delete VM command
fn handle_delete_vm(
  vm_id: types.Uuid,
  reply_with: Subject(Result(Nil, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Check if VM exists
  case khepri_store.get_vm(state.store, vm_id) {
    Ok(vm) -> {
      // Check if VM is running
      case vm.state {
        types.Running | types.Paused -> {
          process.send(
            reply_with,
            Error("Cannot delete running VM. Stop it first."),
          )
          actor.continue(state)
        }

        _ -> {
          // Delete VM from Khepri
          case khepri_store.delete_vm(state.store, vm_id) {
            Ok(_) -> {
              process.send(reply_with, Ok(Nil))
              actor.continue(state)
            }

            Error(err) -> {
              process.send(
                reply_with,
                Error("Failed to delete VM: " <> debug_error(err)),
              )
              actor.continue(state)
            }
          }
        }
      }
    }

    Error(err) -> {
      process.send(reply_with, Error("VM not found: " <> debug_error(err)))
      actor.continue(state)
    }
  }
}

// Handle start VM command
fn handle_start_vm(
  vm_id: types.Uuid,
  reply_with: Subject(Result(Nil, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Check if VM exists
  case khepri_store.get_vm(state.store, vm_id) {
    Ok(vm) -> {
      case vm.host_id {
        Some(id) -> {
          // Find host agent
          case dict.get(state.hosts, id) {
            Ok(host_agent) -> {
              // Forward command to host agent
              let host_reply = process.new_subject()

              process.send(host_agent, agent.StartVM(vm_id, host_reply))

              // Wait for reply from host agent
              case process.receive(host_reply, 10_000) {
                Ok(result) -> {
                  process.send(reply_with, result)
                  actor.continue(state)
                }

                Error(_) -> {
                  process.send(
                    reply_with,
                    Error("Timeout waiting for host agent"),
                  )
                  actor.continue(state)
                }
              }
            }

            Error(_) -> {
              process.send(
                reply_with,
                Error("Host agent not found for host " <> id),
              )
              actor.continue(state)
            }
          }
        }

        None -> {
          process.send(reply_with, Error("VM not assigned to a host"))
          actor.continue(state)
        }
      }
    }

    Error(err) -> {
      process.send(reply_with, Error("VM not found: " <> debug_error(err)))
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
  // Check if VM exists
  case khepri_store.get_vm(state.store, vm_id) {
    Ok(vm) -> {
      case vm.host_id {
        Some(id) -> {
          // Find host agent
          case dict.get(state.hosts, id) {
            Ok(host_agent) -> {
              // Forward command to host agent
              let host_reply = process.new_subject()

              process.send(host_agent, agent.StopVM(vm_id, host_reply))

              // Wait for reply from host agent
              case process.receive(host_reply, 10_000) {
                Ok(result) -> {
                  process.send(reply_with, result)
                  actor.continue(state)
                }

                Error(_) -> {
                  process.send(
                    reply_with,
                    Error("Timeout waiting for host agent"),
                  )
                  actor.continue(state)
                }
              }
            }

            Error(_) -> {
              process.send(
                reply_with,
                Error("Host agent not found for host " <> id),
              )
              actor.continue(state)
            }
          }
        }

        None -> {
          process.send(reply_with, Error("VM not assigned to a host"))
          actor.continue(state)
        }
      }
    }

    Error(err) -> {
      process.send(reply_with, Error("VM not found: " <> debug_error(err)))
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
  // Check if VM exists
  case khepri_store.get_vm(state.store, vm_id) {
    Ok(vm) -> {
      case vm.host_id {
        Some(id) -> {
          // Find host agent
          case dict.get(state.hosts, id) {
            Ok(host_agent) -> {
              // Forward command to host agent
              let host_reply = process.new_subject()

              process.send(host_agent, agent.PauseVM(vm_id, host_reply))

              // Wait for reply from host agent
              case process.receive(host_reply, 10_000) {
                Ok(result) -> {
                  process.send(reply_with, result)
                  actor.continue(state)
                }

                Error(_) -> {
                  process.send(
                    reply_with,
                    Error("Timeout waiting for host agent"),
                  )
                  actor.continue(state)
                }
              }
            }

            Error(_) -> {
              process.send(
                reply_with,
                Error("Host agent not found for host " <> id),
              )
              actor.continue(state)
            }
          }
        }

        None -> {
          process.send(reply_with, Error("VM not assigned to a host"))
          actor.continue(state)
        }
      }
    }

    Error(err) -> {
      process.send(reply_with, Error("VM not found: " <> debug_error(err)))
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
  // Check if VM exists
  case khepri_store.get_vm(state.store, vm_id) {
    Ok(vm) -> {
      case vm.host_id {
        Some(id) -> {
          // Find host agent
          case dict.get(state.hosts, id) {
            Ok(host_agent) -> {
              // Forward command to host agent
              let host_reply = process.new_subject()

              process.send(host_agent, agent.ResumeVM(vm_id, host_reply))

              // Wait for reply from host agent
              case process.receive(host_reply, 10_000) {
                Ok(result) -> {
                  process.send(reply_with, result)
                  actor.continue(state)
                }

                Error(_) -> {
                  process.send(
                    reply_with,
                    Error("Timeout waiting for host agent"),
                  )
                  actor.continue(state)
                }
              }
            }

            Error(_) -> {
              process.send(
                reply_with,
                Error("Host agent not found for host " <> id),
              )
              actor.continue(state)
            }
          }
        }

        None -> {
          process.send(reply_with, Error("VM not assigned to a host"))
          actor.continue(state)
        }
      }
    }

    Error(err) -> {
      process.send(reply_with, Error("VM not found: " <> debug_error(err)))
      actor.continue(state)
    }
  }
}

// Handle register host command
fn handle_register_host(
  host: types.Host,
  reply_with: Subject(Result(types.Host, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Validate host
  case validate_host(host) {
    Ok(_) -> {
      // Store host in Khepri
      case khepri_store.put_host(state.store, host) {
        Ok(_) -> {
          // Start host agent
          case agent.start(host.id, state.store) {
            Ok(host_agent) -> {
              // Add host agent to state
              let new_hosts = dict.insert(state.hosts, host.id, host_agent)
              let new_state = State(..state, hosts: new_hosts)

              process.send(reply_with, Ok(host))
              actor.continue(new_state)
            }

            Error(reason) -> {
              process.send(
                reply_with,
                Error("Failed to start host agent: " <> reason),
              )
              actor.continue(state)
            }
          }
        }

        Error(err) -> {
          process.send(
            reply_with,
            Error("Failed to store host: " <> debug_error(err)),
          )
          actor.continue(state)
        }
      }
    }

    Error(reason) -> {
      process.send(reply_with, Error("Invalid host configuration: " <> reason))
      actor.continue(state)
    }
  }
}

// Validate host configuration
fn validate_host(host: types.Host) -> Result(Nil, String) {
  // Check for required fields
  case host.name, host.control_ip {
    "", _ -> Error("Host name cannot be empty")
    _, "" -> Error("Host control IP cannot be empty")
    _, _ -> Ok(Nil)
  }
}

// Handle deregister host command
fn handle_deregister_host(
  host_id: types.Uuid,
  reply_with: Subject(Result(Nil, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Check if host exists
  case khepri_store.get_host(state.store, host_id) {
    Ok(host) -> {
      // Check if host has running VMs
      case list.is_empty(host.vm_ids) {
        True -> {
          // Remove host from Khepri
          case khepri_store.delete_host(state.store, host_id) {
            Ok(_) -> {
              // Remove host agent from state
              let new_hosts = dict.delete(state.hosts, host_id)
              let new_state = State(..state, hosts: new_hosts)

              process.send(reply_with, Ok(Nil))
              actor.continue(new_state)
            }

            Error(err) -> {
              process.send(
                reply_with,
                Error("Failed to delete host: " <> debug_error(err)),
              )
              actor.continue(state)
            }
          }
        }

        False -> {
          process.send(
            reply_with,
            Error("Host has running VMs. Stop them first."),
          )
          actor.continue(state)
        }
      }
    }

    Error(err) -> {
      process.send(reply_with, Error("Host not found: " <> debug_error(err)))
      actor.continue(state)
    }
  }
}

// Handle get system status command
fn handle_get_system_status(
  reply_with: Subject(Result(SystemStatus, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Get all VMs
  let vms_result = khepri_store.list_vms(state.store)

  // Get all hosts
  let hosts_result = khepri_store.list_hosts(state.store)

  case vms_result, hosts_result {
    Ok(vms), Ok(hosts) -> {
      // Count VMs by state
      let grouped_vms =
        vms
        |> list.group(fn(vm) { vm.state })

      // Convert to list of tuples for dict.from_list
      let state_counts =
        grouped_vms
        |> dict.to_list
        |> list.map(fn(tuple) { #(tuple.0, list.length(tuple.1)) })

      // Create the dictionary from the list of tuples
      let vms_by_state = dict.from_list(state_counts)

      // Create status
      let status =
        SystemStatus(
          vm_count: list.length(vms),
          host_count: list.length(hosts),
          vms_by_state: vms_by_state,
        )

      process.send(reply_with, Ok(status))
      actor.continue(state)
    }

    Error(err), _ -> {
      process.send(
        reply_with,
        Error("Failed to retrieve VMs: " <> debug_error(err)),
      )
      actor.continue(state)
    }

    _, Error(err) -> {
      process.send(
        reply_with,
        Error("Failed to retrieve hosts: " <> debug_error(err)),
      )
      actor.continue(state)
    }
  }
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
