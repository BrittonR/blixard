//// src/blixard/host_agent/agent.gleam

///
/// Host agent that uses shellout to interact with microvm.nix for VM management
import blixard/domain/types as domain_types
import blixard/host_agent/monitoring
import blixard/host_agent/state
import gleam/dynamic.{type Dynamic}

import blixard/host_agent/types.{
  type Command, type HostStatus, type Metrics, type MicroVMStatus, type State,
}
import blixard/host_agent/vm_manager
import blixard/storage/khepri_store.{type Khepri}
import gleam/erlang/process.{type Subject}
import gleam/io
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/otp/actor
import gleam/result
import gleam/string

// Start the host agent
pub fn start(
  host_id: domain_types.Uuid,
  store: Khepri,
) -> Result(Subject(Command), String) {
  // Retrieve host from the store
  let host_result = state.get_host(store, host_id)

  case host_result {
    Ok(host) -> {
      // Initialize state - minimal, not caching VM list
      let initial_state =
        types.State(
          host_id: host_id,
          store: store,
          metrics: types.Metrics(
            vm_start_count: 0,
            vm_stop_count: 0,
            vm_create_count: 0,
            vm_update_count: 0,
            vm_restart_count: 0,
            vm_error_count: 0,
            last_error: None,
          ),
        )

      // Start actor
      case actor.start(initial_state, handle_message) {
        Ok(actor) -> {
          // Register for VM state changes if Khepri supports watches
          // This is a placeholder for Khepri watch functionality
          let _ = state.watch_vms(store, actor)

          Ok(actor)
        }
        Error(err) -> Error("Failed to start actor: " <> debug_actor_error(err))
      }
    }

    Error(err) -> Error("Failed to retrieve host: " <> err)
  }
}

fn debug_actor_error(err: actor.StartError) -> String {
  "Actor start error: " <> string.inspect(err)
}

// Handle incoming messages
fn handle_message(message: Command, state: State) -> actor.Next(Command, State) {
  case message {
    types.StartVM(vm_id, reply_with) ->
      handle_start_vm(vm_id, reply_with, state)
    types.StopVM(vm_id, reply_with) -> handle_stop_vm(vm_id, reply_with, state)
    types.CreateVM(vm, reply_with) -> handle_create_vm(vm, reply_with, state)
    types.UpdateVM(vm_id, reply_with) ->
      handle_update_vm(vm_id, reply_with, state)
    types.RestartVM(vm_id, reply_with) ->
      handle_restart_vm(vm_id, reply_with, state)
    types.ListVMs(reply_with) -> handle_list_vms(reply_with, state)
    types.UpdateResources(resources, reply_with) ->
      handle_update_resources(resources, reply_with, state)
    types.SetSchedulable(schedulable, reply_with) ->
      handle_set_schedulable(schedulable, reply_with, state)
    types.GetStatus(reply_with) -> handle_get_status(reply_with, state)
    types.ExportPrometheusMetrics(reply_with) ->
      handle_export_prometheus_metrics(reply_with, state)
    types.KhepriUpdate(path, value, reply_with) ->
      handle_khepri_update(path, value, reply_with, state)
  }
}

// Handle start VM command
fn handle_start_vm(
  vm_id: domain_types.Uuid,
  reply_with: Subject(Result(Nil, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Get VM directly from Khepri
  let vm_result = state.get_vm(state.store, vm_id)

  case vm_result {
    Ok(vm) -> {
      // Verify VM belongs to this host
      case vm.host_id {
        Some(host_id) if host_id == state.host_id -> {
          // Start the VM
          let start_result = vm_manager.start_vm(vm.name)

          case start_result {
            Ok(_) -> {
              // Update VM state in Khepri
              let state_result =
                state.update_vm_state(state.store, vm_id, domain_types.Running)

              case state_result {
                Ok(_) -> {
                  // Update metrics only
                  let new_metrics =
                    types.Metrics(
                      ..state.metrics,
                      vm_start_count: state.metrics.vm_start_count + 1,
                    )

                  let new_state = types.State(..state, metrics: new_metrics)

                  io.println(
                    "Started VM " <> vm_id <> " (name: " <> vm.name <> ")",
                  )
                  process.send(reply_with, Ok(Nil))
                  actor.continue(new_state)
                }

                Error(err) -> {
                  process.send(reply_with, Error(err))
                  let new_metrics =
                    types.Metrics(
                      ..state.metrics,
                      vm_error_count: state.metrics.vm_error_count + 1,
                      last_error: Some(err),
                    )
                  actor.continue(types.State(..state, metrics: new_metrics))
                }
              }
            }

            Error(err) -> {
              process.send(reply_with, Error(err))
              let new_metrics =
                types.Metrics(
                  ..state.metrics,
                  vm_error_count: state.metrics.vm_error_count + 1,
                  last_error: Some(err),
                )
              actor.continue(types.State(..state, metrics: new_metrics))
            }
          }
        }

        _ -> {
          process.send(reply_with, Error("VM not assigned to this host"))
          actor.continue(state)
        }
      }
    }

    Error(err) -> {
      process.send(reply_with, Error(err))
      let new_metrics =
        types.Metrics(
          ..state.metrics,
          vm_error_count: state.metrics.vm_error_count + 1,
          last_error: Some(err),
        )
      actor.continue(types.State(..state, metrics: new_metrics))
    }
  }
}

// Handle stop VM command
fn handle_stop_vm(
  vm_id: domain_types.Uuid,
  reply_with: Subject(Result(Nil, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Get VM directly from Khepri
  let vm_result = state.get_vm(state.store, vm_id)

  case vm_result {
    Ok(vm) -> {
      // Verify VM belongs to this host
      case vm.host_id {
        Some(host_id) if host_id == state.host_id -> {
          // Stop the VM
          let stop_result = vm_manager.stop_vm(vm.name)

          case stop_result {
            Ok(_) -> {
              // Update VM state in Khepri
              let state_result =
                state.update_vm_state(state.store, vm_id, domain_types.Stopped)

              case state_result {
                Ok(_) -> {
                  // Update metrics
                  let new_metrics =
                    types.Metrics(
                      ..state.metrics,
                      vm_stop_count: state.metrics.vm_stop_count + 1,
                    )

                  let new_state = types.State(..state, metrics: new_metrics)

                  io.println(
                    "Stopped VM " <> vm_id <> " (name: " <> vm.name <> ")",
                  )
                  process.send(reply_with, Ok(Nil))
                  actor.continue(new_state)
                }

                Error(err) -> {
                  process.send(reply_with, Error(err))
                  let new_metrics =
                    types.Metrics(
                      ..state.metrics,
                      vm_error_count: state.metrics.vm_error_count + 1,
                      last_error: Some(err),
                    )
                  actor.continue(types.State(..state, metrics: new_metrics))
                }
              }
            }

            Error(err) -> {
              process.send(reply_with, Error(err))
              let new_metrics =
                types.Metrics(
                  ..state.metrics,
                  vm_error_count: state.metrics.vm_error_count + 1,
                  last_error: Some(err),
                )
              actor.continue(types.State(..state, metrics: new_metrics))
            }
          }
        }

        _ -> {
          process.send(reply_with, Error("VM not assigned to this host"))
          actor.continue(state)
        }
      }
    }

    Error(err) -> {
      process.send(reply_with, Error(err))
      let new_metrics =
        types.Metrics(
          ..state.metrics,
          vm_error_count: state.metrics.vm_error_count + 1,
          last_error: Some(err),
        )
      actor.continue(types.State(..state, metrics: new_metrics))
    }
  }
}

// Handle create VM command
fn handle_create_vm(
  vm: domain_types.MicroVm,
  reply_with: Subject(Result(Nil, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Get the NixOS flake path from the VM config
  let flake_path = vm.nixos_config.config_path

  // Create the VM
  let create_result = vm_manager.create_vm(vm.name, flake_path)

  case create_result {
    Ok(_) -> {
      // Update VM state in Khepri
      let state_result =
        state.update_vm_state(state.store, vm.id, domain_types.Stopped)

      case state_result {
        Ok(_) -> {
          // Update metrics
          let new_metrics =
            types.Metrics(
              ..state.metrics,
              vm_create_count: state.metrics.vm_create_count + 1,
            )

          let new_state = types.State(..state, metrics: new_metrics)

          io.println("Created VM " <> vm.id <> " (name: " <> vm.name <> ")")
          process.send(reply_with, Ok(Nil))
          actor.continue(new_state)
        }

        Error(err) -> {
          process.send(reply_with, Error(err))
          let new_metrics =
            types.Metrics(
              ..state.metrics,
              vm_error_count: state.metrics.vm_error_count + 1,
              last_error: Some(err),
            )
          actor.continue(types.State(..state, metrics: new_metrics))
        }
      }
    }

    Error(err) -> {
      process.send(reply_with, Error(err))
      let new_metrics =
        types.Metrics(
          ..state.metrics,
          vm_error_count: state.metrics.vm_error_count + 1,
          last_error: Some(err),
        )
      actor.continue(types.State(..state, metrics: new_metrics))
    }
  }
}

// Handle update VM command
fn handle_update_vm(
  vm_id: domain_types.Uuid,
  reply_with: Subject(Result(Nil, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Get VM directly from Khepri
  let vm_result = state.get_vm(state.store, vm_id)

  case vm_result {
    Ok(vm) -> {
      // Verify VM belongs to this host
      case vm.host_id {
        Some(host_id) if host_id == state.host_id -> {
          // Update the VM
          let update_result = vm_manager.update_vm(vm.name)

          case update_result {
            Ok(_) -> {
              // Update metrics
              let new_metrics =
                types.Metrics(
                  ..state.metrics,
                  vm_update_count: state.metrics.vm_update_count + 1,
                )

              let new_state = types.State(..state, metrics: new_metrics)

              io.println("Updated VM " <> vm_id <> " (name: " <> vm.name <> ")")
              process.send(reply_with, Ok(Nil))
              actor.continue(new_state)
            }

            Error(err) -> {
              process.send(reply_with, Error(err))
              let new_metrics =
                types.Metrics(
                  ..state.metrics,
                  vm_error_count: state.metrics.vm_error_count + 1,
                  last_error: Some(err),
                )
              actor.continue(types.State(..state, metrics: new_metrics))
            }
          }
        }

        _ -> {
          process.send(reply_with, Error("VM not assigned to this host"))
          actor.continue(state)
        }
      }
    }

    Error(err) -> {
      process.send(reply_with, Error(err))
      let new_metrics =
        types.Metrics(
          ..state.metrics,
          vm_error_count: state.metrics.vm_error_count + 1,
          last_error: Some(err),
        )
      actor.continue(types.State(..state, metrics: new_metrics))
    }
  }
}

// Handle restart VM command
fn handle_restart_vm(
  vm_id: domain_types.Uuid,
  reply_with: Subject(Result(Nil, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Get VM directly from Khepri
  let vm_result = state.get_vm(state.store, vm_id)

  case vm_result {
    Ok(vm) -> {
      // Verify VM belongs to this host
      case vm.host_id {
        Some(host_id) if host_id == state.host_id -> {
          // Restart the VM
          let restart_result = vm_manager.restart_vm(vm.name)

          case restart_result {
            Ok(_) -> {
              // Update VM state in Khepri
              let state_result =
                state.update_vm_state(state.store, vm_id, domain_types.Running)

              case state_result {
                Ok(_) -> {
                  // Update metrics
                  let new_metrics =
                    types.Metrics(
                      ..state.metrics,
                      vm_restart_count: state.metrics.vm_restart_count + 1,
                    )

                  let new_state = types.State(..state, metrics: new_metrics)

                  io.println(
                    "Restarted VM " <> vm_id <> " (name: " <> vm.name <> ")",
                  )
                  process.send(reply_with, Ok(Nil))
                  actor.continue(new_state)
                }

                Error(err) -> {
                  process.send(reply_with, Error(err))
                  let new_metrics =
                    types.Metrics(
                      ..state.metrics,
                      vm_error_count: state.metrics.vm_error_count + 1,
                      last_error: Some(err),
                    )
                  actor.continue(types.State(..state, metrics: new_metrics))
                }
              }
            }

            Error(err) -> {
              process.send(reply_with, Error(err))
              let new_metrics =
                types.Metrics(
                  ..state.metrics,
                  vm_error_count: state.metrics.vm_error_count + 1,
                  last_error: Some(err),
                )
              actor.continue(types.State(..state, metrics: new_metrics))
            }
          }
        }

        _ -> {
          process.send(reply_with, Error("VM not assigned to this host"))
          actor.continue(state)
        }
      }
    }

    Error(err) -> {
      process.send(reply_with, Error(err))
      let new_metrics =
        types.Metrics(
          ..state.metrics,
          vm_error_count: state.metrics.vm_error_count + 1,
          last_error: Some(err),
        )
      actor.continue(types.State(..state, metrics: new_metrics))
    }
  }
}

// Handle list VMs command
fn handle_list_vms(
  reply_with: Subject(Result(List(MicroVMStatus), String)),
  state: State,
) -> actor.Next(Command, State) {
  // List all VMs using microvm.nix
  let list_result = vm_manager.list_vms()

  case list_result {
    Ok(vm_statuses) -> {
      // For each status, try to find the VM ID from Khepri
      let vms_result = state.get_host_vms(state.store, state.host_id)

      case vms_result {
        Ok(vms) -> {
          // Enhance status with VM IDs from Khepri
          // This is a matching exercise - map from name to ID
          let enhanced_statuses =
            vm_statuses
            |> list.map(fn(status) {
              // Find a VM with matching name
              let matching_vm =
                list.find(vms, fn(vm) { vm.name == status.name })

              case matching_vm {
                Ok(vm) -> types.MicroVMStatus(..status, vm_id: Some(vm.id))
                Error(_) -> status
                // No match found, leave as is
              }
            })

          process.send(reply_with, Ok(enhanced_statuses))
          actor.continue(state)
        }

        Error(err) -> {
          // Still return what we got from microvm -l, but log the error
          io.println("Warning: Could not fetch VMs from Khepri: " <> err)
          process.send(reply_with, Ok(vm_statuses))
          actor.continue(state)
        }
      }
    }

    Error(err) -> {
      process.send(reply_with, Error(err))
      let new_metrics =
        types.Metrics(
          ..state.metrics,
          vm_error_count: state.metrics.vm_error_count + 1,
          last_error: Some(err),
        )
      actor.continue(types.State(..state, metrics: new_metrics))
    }
  }
}

// Handle get status command
fn handle_get_status(
  reply_with: Subject(Result(HostStatus, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Get host status with system metrics
  let status_result = monitoring.get_host_status(state.host_id, state.store)

  case status_result {
    Ok(status) -> {
      process.send(reply_with, Ok(status))
      actor.continue(state)
    }

    Error(err) -> {
      process.send(reply_with, Error(err))
      let new_metrics =
        types.Metrics(
          ..state.metrics,
          vm_error_count: state.metrics.vm_error_count + 1,
          last_error: Some(err),
        )
      actor.continue(types.State(..state, metrics: new_metrics))
    }
  }
}

// Handle export prometheus metrics command
fn handle_export_prometheus_metrics(
  reply_with: Subject(Result(String, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Generate Prometheus metrics
  let metrics_result =
    monitoring.generate_prometheus_metrics(
      state.host_id,
      state.store,
      state.metrics,
    )

  case metrics_result {
    Ok(metrics) -> {
      process.send(reply_with, Ok(metrics))
      actor.continue(state)
    }

    Error(err) -> {
      process.send(reply_with, Error(err))
      let new_metrics =
        types.Metrics(
          ..state.metrics,
          vm_error_count: state.metrics.vm_error_count + 1,
          last_error: Some(err),
        )
      actor.continue(types.State(..state, metrics: new_metrics))
    }
  }
}

// Handle update resources command
fn handle_update_resources(
  resources: domain_types.Resources,
  reply_with: Subject(Result(Nil, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Update host resources in Khepri
  let update_result =
    state.update_host_resources(state.store, state.host_id, resources)

  case update_result {
    Ok(_) -> {
      process.send(reply_with, Ok(Nil))
      actor.continue(state)
    }

    Error(err) -> {
      process.send(reply_with, Error(err))
      let new_metrics =
        types.Metrics(
          ..state.metrics,
          vm_error_count: state.metrics.vm_error_count + 1,
          last_error: Some(err),
        )
      actor.continue(types.State(..state, metrics: new_metrics))
    }
  }
}

// Handle set schedulable command
fn handle_set_schedulable(
  schedulable: Bool,
  reply_with: Subject(Result(Nil, String)),
  state: State,
) -> actor.Next(Command, State) {
  // Update host schedulable in Khepri
  let update_result =
    state.update_host_schedulable(state.store, state.host_id, schedulable)

  case update_result {
    Ok(_) -> {
      process.send(reply_with, Ok(Nil))
      actor.continue(state)
    }

    Error(err) -> {
      process.send(reply_with, Error(err))
      let new_metrics =
        types.Metrics(
          ..state.metrics,
          vm_error_count: state.metrics.vm_error_count + 1,
          last_error: Some(err),
        )
      actor.continue(types.State(..state, metrics: new_metrics))
    }
  }
}

// Handle Khepri update notification - placeholder for watch functionality
fn handle_khepri_update(
  path: List(String),
  value: Option(Dynamic),
  reply_with: Subject(Result(Nil, String)),
  state: State,
) -> actor.Next(Command, State) {
  // This is a placeholder for handling Khepri watch notifications
  // The actual implementation would depend on Khepri's watch API
  io.println("Received Khepri update for path: " <> string.join(path, "/"))

  process.send(reply_with, Ok(Nil))
  actor.continue(state)
}
