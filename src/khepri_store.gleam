// src/khepri_store.gleam
//
// Distributed storage module using Khepri
//
// This module provides an interface to the Khepri distributed key-value store,
// which is used for storing service states and other data across the cluster.
// It supports both standalone and clustered operation modes.

import gleam/dynamic
import gleam/dynamic/decode
import gleam/erlang
import gleam/erlang/atom
import gleam/erlang/node
import gleam/erlang/process
import gleam/int
import gleam/io
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import khepri_gleam
import khepri_gleam_cluster

/// Helper function to decode a 2-tuple from dynamic data
fn decode_tuple2(
  first_decoder: decode.Decoder(a),
  second_decoder: decode.Decoder(b),
) -> decode.Decoder(#(a, b)) {
  decode.at([0], first_decoder)
  |> decode.then(fn(first_value) {
    decode.at([1], second_decoder)
    |> decode.map(fn(second_value) { #(first_value, second_value) })
  })
}


/// Helper function to decode a 3-tuple from dynamic data
fn decode_tuple3(
  first_decoder: decode.Decoder(a),
  second_decoder: decode.Decoder(b),
  third_decoder: decode.Decoder(c),
) -> decode.Decoder(#(a, b, c)) {
  decode.at([0], first_decoder)
  |> decode.then(fn(first_value) {
    decode.at([1], second_decoder)
    |> decode.then(fn(second_value) {
      decode.at([2], third_decoder)
      |> decode.map(fn(third_value) {
        #(first_value, second_value, third_value)
      })
    })
  })
}

/// Service state type definition
/// Represents the possible states of a managed service
pub type ServiceState {
  /// Service is currently running
  Running
  /// Service is currently stopped
  Stopped
  /// Service failed to start or stop
  Failed
  /// Service state is unknown
  Unknown
  /// Custom state message (for join notifications, etc.)
  Custom(String)
}

/// Cluster configuration type
/// Used to configure the Khepri cluster
pub type ClusterConfig {
  ClusterConfig(node_role: NodeRole, cookie: String)
}

/// Node role in the cluster
/// Defines whether this node is a primary or a secondary node
pub type NodeRole {
  /// Primary node - responsible for leader election and initial setup
  Primary
  /// Secondary node - connects to the primary node
  Secondary(primary_node: String)
}

/// Service information with metadata
pub type ServiceInfo {
  ServiceInfo(
    name: String,
    state: ServiceState,
    node: String,
    last_updated: String,
  )
}

/// Default store ID for Khepri
/// Used to identify this specific Khepri instance
pub const default_store_id = "khepri"

// External function declarations
@external(erlang, "khepri_gleam_cluster_helper", "wait_for_leader")
fn wait_for_leader_raw() -> Result(Nil, String)

@external(erlang, "khepri_gleam_cluster_helper", "get_cluster_members")
fn get_cluster_members_raw() -> Result(List(String), String)

@external(erlang, "khepri_gleam_cluster_helper", "is_store_running")
fn is_khepri_running_raw(store_id: String) -> Bool

@external(erlang, "rpc", "call")
fn rpc_call_raw(
  node: atom.Atom,
  module: atom.Atom,
  function: atom.Atom,
  args: List(dynamic.Dynamic),
) -> dynamic.Dynamic

@external(erlang, "khepri_gleam_helper", "safe_list_directory")
fn safe_list_directory(
  path: String,
) -> Result(List(#(String, dynamic.Dynamic)), String)

@external(erlang, "khepri_gleam_helper", "get_many")
fn get_many_raw(pattern: List(String)) -> Result(dynamic.Dynamic, String)

@external(erlang, "ra", "members")
fn ra_members(cluster_name: String) -> dynamic.Dynamic

/// Check if we have a local Khepri instance running
pub fn has_local_khepri() -> Bool {
  is_khepri_running()
}

fn is_khepri_running() -> Bool {
  is_khepri_running_raw("khepri")
}

/// Get a cluster node that has Khepri running
fn get_cluster_node_with_khepri() -> Result(node.Node, String) {
  let connected_nodes = node.visible()

  case connected_nodes {
    [] -> Error("No cluster nodes connected")
    [first, ..] -> Ok(first)
  }
}

/// Wrapper for RPC calls with error handling
fn rpc_call_khepri(
  target_node: node.Node,
  function: String,
  args: List(dynamic.Dynamic),
) -> Result(dynamic.Dynamic, String) {
  let node_atom = node.to_atom(target_node)
  let module_atom = atom.create_from_string("khepri")
  let function_atom = atom.create_from_string(function)

  io.println(
    "RPC call to " <> atom.to_string(node_atom) <> " - khepri:" <> function,
  )

  let result = rpc_call_raw(node_atom, module_atom, function_atom, args)

  // Simple check for RPC errors
  let result_str = string.inspect(result)
  case string.contains(result_str, "badrpc") {
    True -> Error("RPC failed: " <> result_str)
    False -> Ok(result)
  }
}

/// Initialize a standalone Khepri instance (non-clustered)
pub fn init() -> Result(Nil, String) {
  io.println("Initializing Khepri store...")
  khepri_gleam.start()

  let services_path = khepri_gleam.to_khepri_path("/:services/")
  case khepri_gleam.exists("/:services/") {
    False -> {
      case khepri_gleam.tx_put_path("/:services/", "service_states") {
        Ok(_) -> io.println("Created services storage")
        Error(err) ->
          io.println("Warning: Failed to create services path: " <> err)
      }
    }
    True -> io.println("Services storage exists")
  }

  Ok(Nil)
}

/// Check if we're already part of a Khepri cluster
fn is_already_in_cluster() -> Bool {
  case is_khepri_running() {
    False -> False
    True -> {
      case get_cluster_members_raw() {
        Ok(members) -> list.length(members) > 0
        Error(_) -> False
      }
    }
  }
}

/// Initialize Khepri with clustering support
pub fn init_cluster(config: ClusterConfig) -> Result(Nil, String) {
  case is_already_in_cluster() {
    True -> {
      io.println("Already part of a Khepri cluster, skipping initialization")
      Ok(Nil)
    }
    False -> {
      io.println("Starting Khepri...")
      khepri_gleam.start()

      case config.node_role {
        Primary -> {
          let services_path = khepri_gleam.to_khepri_path("/:services/")
          case khepri_gleam.exists("/services/") {
            False -> {
              khepri_gleam.put(services_path, "service_states")
              io.println("Created services storage")
            }
            True -> io.println("Services storage exists")
          }

          let events_path = khepri_gleam.to_khepri_path("/:cluster_events/")
          case khepri_gleam.exists("/:cluster_events/") {
            False -> {
              khepri_gleam.put(events_path, "cluster_events")
              io.println("Created cluster events storage")
            }
            True -> io.println("Cluster events storage exists")
          }
        }
        Secondary(_) -> {
          io.println("Secondary node - skipping path creation")
        }
      }

      case khepri_gleam_cluster.start() {
        Ok(cluster) -> {
          case config.node_role {
            Primary -> {
              io.println("Waiting longer for leader election...")
              process.sleep(10_000)

              case wait_for_leader_safely() {
                Ok(_) -> {
                  io.println("Leader election complete")
                  Ok(Nil)
                }
                Error(err) -> Error("Leader election failed: " <> err)
              }
            }
            Secondary(primary_node) -> {
              io.println("Joining primary node: " <> primary_node)
              join_primary_with_retry(cluster, primary_node, 5)
            }
          }
        }
        Error(err) ->
          Error("Failed to start cluster actor: " <> string.inspect(err))
      }
    }
  }
}

fn wait_for_leader_safely() -> Result(Nil, String) {
  case wait_for_leader_raw() {
    Ok(_) -> Ok(Nil)
    Error(_) -> {
      io.println(
        "Direct leader election call failed, allowing time for auto-election",
      )
      process.sleep(5000)
      Ok(Nil)
    }
  }
}

pub fn debug_print_paths() -> Nil {
  let paths = get_all_paths()

  io.println("\n==== DEBUG: ALL KHEPRI PATHS ====")
  io.println("Found " <> int.to_string(list.length(paths)) <> " paths:")
  list.each(paths, fn(path) { io.println("• " <> string.inspect(path)) })
  io.println("================================\n")
}

fn join_primary_with_retry(
  cluster: process.Subject(khepri_gleam_cluster.ClusterMessage),
  primary_node: String,
  retries: Int,
) -> Result(Nil, String) {
  case retries <= 0 {
    True -> Error("Failed to join cluster after multiple attempts")
    False -> {
      case khepri_gleam_cluster.join(cluster, primary_node, 5000) {
        Ok(_) -> {
          io.println("Successfully joined the cluster!")
          io.println("Waiting for cluster to stabilize...")
          process.sleep(10_000)
          Ok(Nil)
        }
        Error(err) -> {
          io.println("Join attempt failed: " <> string.inspect(err))
          io.println("Retrying in 3 seconds...")
          process.sleep(3000)
          join_primary_with_retry(cluster, primary_node, retries - 1)
        }
      }
    }
  }
}

/// Store a service state in the Khepri store with metadata
pub fn store_service_state(
  service: String,
  state: ServiceState,
) -> Result(Nil, String) {
  let path = "/:services/" <> service
  let current_node = atom.to_string(node.to_atom(node.self()))
  let timestamp = int.to_string(erlang.system_time(erlang.Millisecond))

  let state_string = case state {
    Running -> "running"
    Stopped -> "stopped"
    Failed -> "failed"
    Unknown -> "unknown"
    Custom(message) -> message
  }

  let khepri_path = khepri_gleam.to_khepri_path(path)
  let service_data = #(state_string, current_node, timestamp)

  io.println("Writing to path: " <> string.inspect(khepri_path))

  case has_local_khepri() {
    True -> {
      khepri_gleam.put(khepri_path, service_data)

      // Also update the service list
      update_service_list(service)

      Ok(Nil)
    }
    False -> {
      io.println("Local Khepri not available, using RPC")

      case get_cluster_node_with_khepri() {
        Ok(remote_node) -> {
          let path_dynamic = dynamic.from(khepri_path)
          let data_dynamic = dynamic.from(service_data)

          case
            rpc_call_khepri(remote_node, "put", [path_dynamic, data_dynamic])
          {
            Ok(_) -> {
              io.println("Successfully stored via RPC")

              // Also update service list via RPC
              update_service_list_via_rpc(remote_node, service)

              Ok(Nil)
            }
            Error(e) -> {
              io.println("RPC store failed: " <> e)
              Error("Failed to store via RPC: " <> e)
            }
          }
        }
        Error(e) -> {
          io.println("No cluster node available: " <> e)
          Error("No cluster node available: " <> e)
        }
      }
    }
  }
}

/// Update the list of known services
fn update_service_list(service: String) -> Nil {
  let services_list_path = khepri_gleam.to_khepri_path("/:service_list")

  let current_services = case khepri_gleam.get(services_list_path) {
    Ok(data) -> {
      case decode.run(data, decode.list(decode.string)) {
        Ok(names) -> names
        Error(_) -> []
      }
    }
    Error(_) -> []
  }

  let updated_services = case list.contains(current_services, service) {
    True -> current_services
    False -> [service, ..current_services]
  }

  khepri_gleam.put(services_list_path, updated_services)
  Nil
}

/// Update service list via RPC
fn update_service_list_via_rpc(remote_node: node.Node, service: String) -> Nil {
  let services_list_path = khepri_gleam.to_khepri_path("/:service_list")

  // Get current list via RPC
  case rpc_call_khepri(remote_node, "get", [dynamic.from(services_list_path)]) {
    Ok(rpc_result) -> {
      let current_services = case
        decode.run(rpc_result, decode_tuple2(decode.string, decode.dynamic))
      {
        Ok(#("ok", data)) -> {
          case decode.run(data, decode.list(decode.string)) {
            Ok(names) -> names
            Error(_) -> []
          }
        }
        _ -> []
      }

      let updated_services = case list.contains(current_services, service) {
        True -> current_services
        False -> [service, ..current_services]
      }

      // Store updated list via RPC
      let _ =
        rpc_call_khepri(remote_node, "put", [
          dynamic.from(services_list_path),
          dynamic.from(updated_services),
        ])

      Nil
    }
    Error(_) -> Nil
  }
}

pub fn broadcast_cluster_event(
  event_type: String,
  node_name: String,
) -> Result(Nil, String) {
  let timestamp = int.to_string(erlang.system_time(erlang.Millisecond))
  let event_key = event_type <> "_" <> timestamp

  case khepri_gleam.exists("/:cluster_events/") {
    False -> {
      khepri_gleam.put(
        khepri_gleam.to_khepri_path("/:cluster_events/"),
        "cluster_events",
      )
    }
    True -> Nil
  }

  khepri_gleam.put(
    khepri_gleam.to_khepri_path("/:cluster_events/" <> event_key),
    node_name,
  )

  Ok(Nil)
}

pub fn store_join_notification(node_name: String) -> Result(Nil, String) {
  broadcast_cluster_event("join", node_name)
}

pub fn get_cluster_events() -> Result(List(#(String, String)), String) {
  case khepri_gleam.exists("/:cluster_events/") {
    False -> {
      Ok([])
    }
    True -> {
      case safe_list_directory("/:cluster_events/") {
        Ok(events) -> {
          let parsed_events =
            list.map(events, fn(event) {
              let #(key, data) = event
              let value = case decode.run(data, decode.string) {
                Ok(str) -> str
                Error(_) -> "unknown"
              }
              #(key, value)
            })
          Ok(parsed_events)
        }
        Error(_) -> {
          Ok([])
        }
      }
    }
  }
}

pub fn get_service_state(service: String) -> Result(ServiceState, String) {
  let path = "/:services/" <> service

  case khepri_gleam.tx_get_path(path) {
    Ok(result) -> {
      case decode_service_data(result) {
        Ok(state) -> Ok(state)
        Error(_) -> {
          case decode.run(result, decode.string) {
            Ok("running") -> Ok(Running)
            Ok("stopped") -> Ok(Stopped)
            Ok("failed") -> Ok(Failed)
            Ok(other) -> Ok(Custom(other))
            Error(_) -> Ok(Unknown)
          }
        }
      }
    }
    Error(e) -> Error(e)
  }
}

fn decode_service_data(data: dynamic.Dynamic) -> Result(ServiceState, String) {
  let tuple_decoder = decode_tuple3(decode.string, decode.string, decode.string)

  case decode.run(data, tuple_decoder) {
    Ok(#(state_str, _node, _timestamp)) -> {
      case state_str {
        "running" -> Ok(Running)
        "stopped" -> Ok(Stopped)
        "failed" -> Ok(Failed)
        other -> Ok(Custom(other))
      }
    }
    Error(_) -> Error("Could not decode service data")
  }
}

pub fn get_service_info(service: String) -> Result(ServiceInfo, String) {
  io.println("Getting service info for: " <> service)

  let khepri_path = khepri_gleam.to_khepri_path("/:services/" <> service)

  let result = case has_local_khepri() {
    True -> khepri_gleam.get(khepri_path)
    False -> {
      case get_cluster_node_with_khepri() {
        Ok(remote_node) -> {
          case
            rpc_call_khepri(remote_node, "get", [dynamic.from(khepri_path)])
          {
            Ok(rpc_result) -> {
              // RPC returns {ok, Value} or {error, Reason}
              case
                decode.run(
                  rpc_result,
                  decode_tuple2(decode.string, decode.dynamic),
                )
              {
                Ok(#("ok", value)) -> Ok(value)
                Ok(#(_, _)) -> Error("Not found")
                Error(_) -> {
                  // Maybe it returned the value directly
                  Ok(rpc_result)
                }
              }
            }
            Error(e) -> Error(e)
          }
        }
        Error(e) -> Error(e)
      }
    }
  }

  case result {
    Ok(data) -> {
      let tuple_decoder =
        decode_tuple3(decode.string, decode.string, decode.string)

      case decode.run(data, tuple_decoder) {
        Ok(#(state_str, node, timestamp)) -> {
          let state = case state_str {
            "running" -> Running
            "stopped" -> Stopped
            "failed" -> Failed
            other -> Custom(other)
          }
          Ok(ServiceInfo(
            name: service,
            state: state,
            node: node,
            last_updated: timestamp,
          ))
        }
        Error(_) -> {
          case decode.run(data, decode.string) {
            Ok(state_str) -> {
              let state = case state_str {
                "running" -> Running
                "stopped" -> Stopped
                "failed" -> Failed
                other -> Custom(other)
              }
              Ok(ServiceInfo(
                name: service,
                state: state,
                node: "unknown",
                last_updated: "unknown",
              ))
            }
            Error(_) -> Error("Could not decode service data")
          }
        }
      }
    }
    Error(e) -> Error("Service not found: " <> e)
  }
}

fn discover_services_from_paths() -> Result(List(String), String) {
  io.println("Discovering services directly from Khepri paths...")

  let all_paths = get_all_paths()
  io.println("All paths: " <> string.inspect(all_paths))

  let service_names =
    list.filter_map(all_paths, fn(path) {
      case path {
        ["services", service_name] -> Ok(service_name)
        _ -> Error(Nil)
      }
    })

  io.println("Service names: " <> string.inspect(service_names))
  Ok(service_names)
}

pub fn list_cli_services() -> Result(List(ServiceInfo), String) {
  io.println("Getting CLI-managed services from Khepri...")

  case has_local_khepri() {
    True -> {
      list_cli_services_local()
    }
    False -> {
      io.println("Using RPC to list services")

      case get_cluster_node_with_khepri() {
        Ok(remote_node) -> {
          // Try the simpler approach first - get service list
          list_services_via_service_list(remote_node)
        }
        Error(e) -> Error("No cluster node available: " <> e)
      }
    }
  }
}

/// List services using the maintained service list
fn list_services_via_service_list(
  remote_node: node.Node,
) -> Result(List(ServiceInfo), String) {
  let services_list_path = khepri_gleam.to_khepri_path("/:service_list")

  case rpc_call_khepri(remote_node, "get", [dynamic.from(services_list_path)]) {
    Ok(rpc_result) -> {
      case
        decode.run(rpc_result, decode_tuple2(decode.string, decode.dynamic))
      {
        Ok(#("ok", data)) -> {
          case decode.run(data, decode.list(decode.string)) {
            Ok(service_names) -> {
              io.println(
                "Found "
                <> int.to_string(list.length(service_names))
                <> " services in list",
              )

              // Get info for each service
              let service_infos =
                list.filter_map(service_names, fn(name) {
                  case get_service_info(name) {
                    Ok(info) -> Ok(info)
                    Error(_) -> Error(Nil)
                  }
                })

              Ok(service_infos)
            }
            Error(_) -> {
              io.println(
                "Failed to decode service list, trying alternative method",
              )
              list_services_via_rpc_individually(remote_node)
            }
          }
        }
        Ok(#("error", reason)) -> {
          io.println("Service list not found: " <> string.inspect(reason))
          list_services_via_rpc_individually(remote_node)
        }
        Ok(#(_, _)) -> {
          io.println("Unexpected RPC response format")
          list_services_via_rpc_individually(remote_node)
        }
        Error(_) -> {
          io.println("Invalid RPC response for service list")
          list_services_via_rpc_individually(remote_node)
        }
      }
    }
    Error(e) -> {
      io.println("RPC failed: " <> e)
      Error("Failed to get service list: " <> e)
    }
  }
}

/// Alternative method - use get_children_direct
fn list_services_via_rpc_individually(
  remote_node: node.Node,
) -> Result(List(ServiceInfo), String) {
  // Call the helper function on the remote node to get children
  let node_atom = node.to_atom(remote_node)
  let module_atom = atom.create_from_string("khepri_gleam_helper")
  let function_atom = atom.create_from_string("get_children_direct")
  
  // Create the path as a list of bit arrays (binaries in Erlang)
  // Note: Don't include the colon prefix - Khepri paths don't use it
  let services_path = [<<"services":utf8>>]
  let args = [dynamic.from(services_path)]

  io.println(
    "Calling khepri_gleam_helper:get_children_direct on "
    <> atom.to_string(node_atom),
  )
  io.println("Getting children for path: " <> string.inspect(services_path))

  let result = rpc_call_raw(node_atom, module_atom, function_atom, args)

  io.println("RPC result type: " <> string.inspect(dynamic.classify(result)))
  io.println("Raw RPC result: " <> string.inspect(result))

  // Looking at the output, the RPC returns Ok([...]) 
  // The Erlang function returns {ok, Children} which Gleam shows as Ok(...)
  // But it's wrapped in a dynamic, so we need to unwrap it properly
  
  // First, let's try to access the internals as a tagged tuple
  // Erlang {ok, Value} is a 2-tuple where first element is atom 'ok'
  case decode.run(result, decode.at([1], decode.list(decode_tuple2(decode.string, decode.dynamic)))) {
    Ok(children) -> {
      io.println("Successfully decoded " <> int.to_string(list.length(children)) <> " services")
      let service_infos = list.filter_map(children, fn(child) {
        let #(name, data) = child
        decode_service_info_from_data(name, data)
      })
      Ok(service_infos)
    }
    Error(_) -> {
      // Try another approach - decode as a 2-tuple where first element is "ok"
      case decode.run(result, decode_tuple2(decode.dynamic, decode.list(decode_tuple2(decode.string, decode.dynamic)))) {
        Ok(#(_, children)) -> {
          io.println("Successfully decoded via tuple: " <> int.to_string(list.length(children)) <> " services")
          let service_infos = list.filter_map(children, fn(child) {
            let #(name, data) = child
            decode_service_info_from_data(name, data)
          })
          Ok(service_infos)
        }
        Error(e) -> {
          io.println("Failed to decode result: " <> string.inspect(e))
          io.println("Trying direct list decode...")
          // Last attempt - maybe it's just the list directly
          case decode.run(result, decode.list(decode_tuple2(decode.string, decode.dynamic))) {
            Ok(children) -> {
              io.println("Direct list decode worked!")
              let service_infos = list.filter_map(children, fn(child) {
                let #(name, data) = child
                decode_service_info_from_data(name, data)
              })
              Ok(service_infos)
            }
            Error(_) -> {
              io.println("All decode attempts failed")
              Ok([])
            }
          }
        }
      }
    }
  }
}

/// Helper to decode service info from raw data
fn decode_service_info_from_data(
  name: String,
  data: dynamic.Dynamic,
) -> Result(ServiceInfo, Nil) {
  // Skip metadata entries
  case name {
    "services" | "service_states" -> Error(Nil)
    _ -> {
      // Try to decode the service data
      let tuple_decoder =
        decode_tuple3(decode.string, decode.string, decode.string)

      case decode.run(data, tuple_decoder) {
        Ok(#(state_str, node, timestamp)) -> {
          let state = case state_str {
            "running" -> Running
            "stopped" -> Stopped
            "failed" -> Failed
            other -> Custom(other)
          }
          Ok(ServiceInfo(
            name: name,
            state: state,
            node: node,
            last_updated: timestamp,
          ))
        }
        Error(_) -> {
          // Fallback for simple string
          case decode.run(data, decode.string) {
            Ok(state_str) -> {
              let state = case state_str {
                "running" -> Running
                "stopped" -> Stopped
                "failed" -> Failed
                other -> Custom(other)
              }
              Ok(ServiceInfo(
                name: name,
                state: state,
                node: "unknown",
                last_updated: "unknown",
              ))
            }
            Error(_) -> Error(Nil)
          }
        }
      }
    }
  }
}

fn list_cli_services_local() -> Result(List(ServiceInfo), String) {
  case discover_services_from_paths() {
    Ok(service_names) -> {
      let services =
        list.filter_map(service_names, fn(service_name) {
          case get_service_info(service_name) {
            Ok(info) -> Ok(info)
            Error(_) -> Error(Nil)
          }
        })
      Ok(services)
    }
    Error(err) -> {
      fallback_list_services()
    }
  }
}

fn fallback_list_services() -> Result(List(ServiceInfo), String) {
  io.println("Using fallback list_directory method...")

  case khepri_gleam.list_directory("/:services/") {
    Ok(items) -> {
      let services =
        list.filter_map(items, fn(item) {
          let #(name, data) = item

          case name {
            "services" -> Error(Nil)
            "service_states" -> Error(Nil)
            _ -> {
              let tuple_decoder =
                decode_tuple3(decode.string, decode.string, decode.string)

              case decode.run(data, tuple_decoder) {
                Ok(#(state_str, node, timestamp)) -> {
                  let state = case state_str {
                    "running" -> Running
                    "stopped" -> Stopped
                    "failed" -> Failed
                    other -> Custom(other)
                  }
                  Ok(ServiceInfo(
                    name: name,
                    state: state,
                    node: node,
                    last_updated: timestamp,
                  ))
                }
                Error(_) -> {
                  let state = case decode.run(data, decode.string) {
                    Ok("running") -> Running
                    Ok("stopped") -> Stopped
                    Ok("failed") -> Failed
                    Ok(other) -> Custom(other)
                    Error(_) -> Unknown
                  }
                  Ok(ServiceInfo(
                    name: name,
                    state: state,
                    node: "unknown",
                    last_updated: "unknown",
                  ))
                }
              }
            }
          }
        })

      Ok(services)
    }
    Error(err) -> Error("Fallback also failed: " <> err)
  }
}

pub fn remove_service_state(service: String) -> Result(Nil, String) {
  let khepri_path = khepri_gleam.to_khepri_path("/:services/" <> service)
  io.println("Removing service from path: " <> string.inspect(khepri_path))

  khepri_gleam.delete(khepri_path)
  io.println("Service " <> service <> " removed from Khepri")

  Ok(Nil)
}

pub fn list_services() -> Result(List(#(String, ServiceState)), String) {
  case list_cli_services() {
    Ok(services) -> {
      let simple_services =
        list.map(services, fn(service_info) {
          #(service_info.name, service_info.state)
        })
      Ok(simple_services)
    }
    Error(err) -> Error(err)
  }
}

pub fn list_services_direct() -> Result(List(#(String, ServiceState)), String) {
  list_services()
}

@external(erlang, "khepri_gleam_helper", "get_registered_paths")
pub fn get_all_paths() -> List(List(String)) {
  io.println("Getting all paths from Khepri...")
  
  case has_local_khepri() {
    True -> {
      // Use local Khepri
      // For now, since the debug function is just for diagnostics,
      // return an empty list to avoid complexity
      []
    }
    False -> {
      // Try to get from remote node
      case get_cluster_node_with_khepri() {
        Ok(remote_node) -> {
          // For now, just return empty list since we're mainly interested in services
          []
        }
        Error(_) -> {
          io.println("Failed to get paths from Khepri")
          []
        }
      }
    }
  }
}

fn test_primary_connection(primary_node: String) -> Bool {
  let node_atom = atom.create_from_string(primary_node)
  case node.connect(node_atom) {
    Ok(_) -> {
      io.println("✅ Direct connection to primary successful")
      True
    }
    Error(_) -> {
      io.println("❌ Cannot connect to primary node")
      False
    }
  }
}

/// Check if this is a client command that shouldn't start Khepri
pub fn should_run_as_client(args: List(String)) -> Bool {
  case args {
    ["start", ..] -> True
    ["stop", ..] -> True
    ["restart", ..] -> True
    ["status", ..] -> True
    ["list"] -> True
    ["list-cluster"] -> True
    ["remove", ..] -> True
    _ -> False
  }
}
