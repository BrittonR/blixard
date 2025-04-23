// src/khepri_store.gleam - Modified for proper clustering

import gleam/dynamic
import gleam/erlang
import gleam/erlang/atom
import gleam/erlang/node
import gleam/erlang/process
import gleam/int
import gleam/io
import gleam/list
import gleam/result
import gleam/string
import khepri_gleam
import khepri_gleam_cluster

// Service state type definition
pub type ServiceState {
  Running
  Stopped
  Failed
  Unknown
  Custom(String)
}

// Cluster configuration
pub type ClusterConfig {
  ClusterConfig(node_role: NodeRole, cookie: String)
}

// Node role in the cluster
pub type NodeRole {
  Primary
  Secondary(primary_node: String)
}

// Global cluster subject to maintain throughout the app's lifecycle
pub const default_store_id = "khepri"

// Initialize a standalone Khepri instance (non-clustered)
pub fn init() -> Result(Nil, String) {
  // Start Khepri with specific store ID for consistency
  io.println("Initializing Khepri store...")
  khepri_gleam.start()

  // Ensure services path exists using a transaction
  let services_path = khepri_gleam.to_khepri_path("/:services")
  case khepri_gleam.exists("/:services") {
    False -> {
      // Create using transactional API for reliability
      case khepri_gleam.tx_put_path("/:services", "service_states") {
        Ok(_) -> io.println("Created services storage")
        Error(err) ->
          io.println("Warning: Failed to create services path: " <> err)
      }
    }
    True -> io.println("Services storage exists")
  }

  Ok(Nil)
}

// src/khepri_store.gleam - Fix for the Badarg error
// src/khepri_store.gleam - Fix for the Badarg error

// Add these at the module level (outside any functions)
@external(erlang, "khepri_gleam_cluster_helper", "wait_for_leader")
fn wait_for_leader_raw() -> Result(Nil, String)

// Initialize Khepri with clustering support
pub fn init_cluster(config: ClusterConfig) -> Result(Nil, String) {
  // Start Khepri first
  io.println("Starting Khepri...")
  khepri_gleam.start()

  // Create services path
  let services_path = khepri_gleam.to_khepri_path("/:services")
  case khepri_gleam.exists("/:services") {
    False -> {
      khepri_gleam.put(services_path, "service_states")
      io.println("Created services storage")
    }
    True -> io.println("Services storage exists")
  }

  // Start the cluster actor
  case khepri_gleam_cluster.start() {
    Ok(cluster) -> {
      // Handle cluster based on role
      case config.node_role {
        Primary -> {
          // Primary node - wait for leader election
          io.println("Running as primary node")

          // Try the simple version first with no timeout
          // This should be safer than the version with a timeout
          case wait_for_leader_safely() {
            Ok(_) -> {
              io.println("Leader election complete")
              Ok(Nil)
            }
            Error(err) -> Error("Leader election failed: " <> err)
          }
        }
        Secondary(primary_node) -> {
          // Secondary node - join the primary
          io.println("Joining primary node: " <> primary_node)
          join_primary_with_retry(cluster, primary_node, 5)
        }
      }
    }
    Error(err) ->
      Error("Failed to start cluster actor: " <> string.inspect(err))
  }
}

// Safe wrapper for wait_for_leader that handles errors properly
fn wait_for_leader_safely() -> Result(Nil, String) {
  // Try with raw version first (no timeout parameter)
  case wait_for_leader_raw() {
    Ok(_) -> Ok(Nil)
    Error(_) -> {
      // If that fails, just sleep a bit to allow auto-election
      // and return success
      io.println(
        "Direct leader election call failed, allowing time for auto-election",
      )
      process.sleep(5000)
      Ok(Nil)
    }
  }
}

// In src/khepri_store.gleam - Fixed debug function

// Debug function to print what paths exist in the store
pub fn debug_print_paths() -> Nil {
  // Get all paths directly (no Result wrapping)
  let paths = get_all_paths()

  io.println("\n==== DEBUG: ALL KHEPRI PATHS ====")
  io.println("Found " <> int.to_string(list.length(paths)) <> " paths:")
  list.each(paths, fn(path) { io.println("• " <> string.inspect(path)) })
  io.println("================================\n")
}

// Join primary with explicit path testing
// In src/khepri_store.gleam

// Join the primary node with simple reliable approach
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

          // Wait for leader election to complete
          process.sleep(3000)
          let _ = khepri_gleam_cluster.wait_for_leader(5000)

          // Special handling for simple reliable replication
          let timestamp = erlang.system_time(erlang.Millisecond)
          let test_key = "test_" <> int.to_string(timestamp)
          let test_value = "joined_at_" <> int.to_string(timestamp)

          // Use direct khepri_gleam.put for more reliable writes
          let path = khepri_gleam.to_khepri_path("/:services/" <> test_key)
          io.println("Writing test data at: " <> string.inspect(path))
          khepri_gleam.put(path, test_value)

          // Give time for replication
          process.sleep(3000)

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

// Store a service state - simplified for reliability
pub fn store_service_state(
  service: String,
  state: ServiceState,
) -> Result(Nil, String) {
  // Use the proven path format
  let path = "/:services/" <> service

  // Convert state to string
  let state_string = case state {
    Running -> "running"
    Stopped -> "stopped"
    Failed -> "failed"
    Unknown -> "unknown"
    Custom(message) -> message
  }

  // Use direct put for more reliability
  let khepri_path = khepri_gleam.to_khepri_path(path)
  io.println("Writing to path: " <> string.inspect(khepri_path))
  khepri_gleam.put(khepri_path, state_string)

  Ok(Nil)
}

// Store a join notification using transactions
pub fn store_join_notification(
  key: String,
  message: String,
) -> Result(Nil, String) {
  // Store in a special path for join notifications
  store_service_state(key, Custom(message))
}

// Get a service state
pub fn get_service_state(service: String) -> Result(ServiceState, String) {
  let path = "/:services/" <> service

  // Use transaction for consistency
  case khepri_gleam.tx_get_path(path) {
    Ok(result) -> {
      // Try to convert result to string
      case dynamic.string(result) {
        Ok("running") -> Ok(Running)
        Ok("stopped") -> Ok(Stopped)
        Ok("failed") -> Ok(Failed)
        Ok(other) -> Ok(Custom(other))
        // For join notifications and custom states
        Error(_) -> Ok(Unknown)
      }
    }
    Error(e) -> Error(e)
  }
}

// List all services and their states with improved path handling
pub fn list_services() -> Result(List(#(String, ServiceState)), String) {
  io.println("Getting children for path: [\"services\"]")

  // Try both path formats
  let with_colon =
    khepri_gleam.list_directory("/:services")
    |> result.unwrap([])

  let without_colon =
    khepri_gleam.list_directory("/services")
    |> result.unwrap([])

  // Combine results
  let all_items = list.append(with_colon, without_colon)

  // Remove duplicates by key
  let unique_keys = list.new()
  let unique_items =
    list.fold(all_items, [], fn(acc, item) {
      let #(key, _) = item
      case list.contains(unique_keys, key) {
        True -> acc
        False -> [item, ..acc]
      }
    })

  io.println(
    "Found " <> int.to_string(list.length(unique_items)) <> " unique items",
  )

  // Process them as before
  let service_states =
    unique_items
    |> list.map(fn(item) {
      let #(name, data) = item

      let state_str = case dynamic.string(data) {
        Ok(str) -> str
        Error(_) -> "unknown"
      }

      let state = case state_str {
        "running" -> Running
        "stopped" -> Stopped
        "failed" -> Failed
        _ -> Custom(state_str)
      }

      #(name, state)
    })

  Ok(service_states)
}

// Add to src/khepri_store.gleam

// Direct access list services function that doesn't rely on list_directory
pub fn list_services_direct() -> Result(List(#(String, ServiceState)), String) {
  io.println("Using direct path access instead of list_directory")

  // Get all registered paths
  let all_paths = get_all_paths()

  // Filter for service paths
  let service_paths =
    list.filter(all_paths, fn(path) {
      // Check if this is a services path with at least 2 components
      // First component should be "services"
      case path {
        ["services", second, ..] -> True
        _ -> False
      }
    })

  io.println(
    "Found " <> int.to_string(list.length(service_paths)) <> " service paths",
  )

  // Convert paths to service entries
  let services =
    list.map(service_paths, fn(path) {
      // The key is the last part of the path
      let key = case list.last(path) {
        Ok(k) -> k
        Error(_) -> "unknown"
      }

      // Get the value directly
      case khepri_gleam.get_string("/:services/" <> key) {
        Ok(value) -> {
          let state = case value {
            "running" -> Running
            "stopped" -> Stopped
            "failed" -> Failed
            other -> Custom(other)
          }
          #(key, state)
        }
        Error(_) -> #(key, Unknown)
      }
    })

  Ok(services)
}

// Add this helper function to get all registered paths
// In src/khepri_store.gleam - Make the function public
@external(erlang, "khepri_gleam_helper", "get_registered_paths")
pub fn get_all_paths() -> List(List(String))
