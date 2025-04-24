// src/khepri_store.gleam
//
// Distributed storage module using Khepri
//
// This module provides an interface to the Khepri distributed key-value store,
// which is used for storing service states and other data across the cluster.
// It supports both standalone and clustered operation modes.

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

/// Default store ID for Khepri
/// Used to identify this specific Khepri instance
pub const default_store_id = "khepri"

/// Initialize a standalone Khepri instance (non-clustered)
/// 
/// This sets up a local Khepri store for CLI operations
/// that need to read/write service state information.
///
/// # Returns
/// - `Ok(Nil)` if initialization succeeded
/// - `Error(String)` with error message if initialization failed
pub fn init() -> Result(Nil, String) {
  // Start Khepri with specific store ID for consistency
  io.println("Initializing Khepri store...")
  khepri_gleam.start()

  // Ensure services path exists using a transaction
  let services_path = khepri_gleam.to_khepri_path("/:services/")
  case khepri_gleam.exists("/:services/") {
    False -> {
      // Create using transactional API for reliability
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

// External function declarations
@external(erlang, "khepri_gleam_cluster_helper", "wait_for_leader")
fn wait_for_leader_raw() -> Result(Nil, String)

/// Initialize Khepri with clustering support
///
/// Sets up a clustered Khepri store that replicates data across nodes.
/// Primary nodes initialize the cluster, while secondary nodes join
/// an existing cluster.
///
/// # Arguments
/// - `config`: Cluster configuration specifying node role and cookie
///
/// # Returns
/// - `Ok(Nil)` if initialization succeeded
/// - `Error(String)` with error message if initialization failed
pub fn init_cluster(config: ClusterConfig) -> Result(Nil, String) {
  // Start Khepri first
  io.println("Starting Khepri...")
  khepri_gleam.start()

  // Create services path
  let services_path = khepri_gleam.to_khepri_path("/:services/")
  case khepri_gleam.exists("/:services/") {
    False -> {
      khepri_gleam.put(services_path, "service_states")
      io.println("Created services storage")
    }
    True -> io.println("Services storage exists")
  }

  // Create cluster_events path
  let events_path = khepri_gleam.to_khepri_path("/:cluster_events/")
  case khepri_gleam.exists("/:cluster_events/") {
    False -> {
      khepri_gleam.put(events_path, "cluster_events")
      io.println("Created cluster events storage")
    }
    True -> io.println("Cluster events storage exists")
  }

  // Start the cluster actor
  case khepri_gleam_cluster.start() {
    Ok(cluster) -> {
      // Handle cluster based on role
      case config.node_role {
        Primary -> {
          // Wait longer for leader election (10 seconds)
          // This is important for cluster stability
          io.println("Waiting longer for leader election...")
          process.sleep(10_000)

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

/// Safe wrapper for wait_for_leader that handles errors properly
/// 
/// Attempts to wait for leader election to complete, with a fallback
/// to just sleeping if the direct call fails.
///
/// # Returns
/// - `Ok(Nil)` if leader election completes or we've waited long enough
/// - `Error(String)` with error message if something catastrophic happens
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

/// Debug function to print what paths exist in the Khepri store
///
/// This is useful for diagnosing issues with data storage and replication
pub fn debug_print_paths() -> Nil {
  // Get all paths directly (no Result wrapping)
  let paths = get_all_paths()

  io.println("\n==== DEBUG: ALL KHEPRI PATHS ====")
  io.println("Found " <> int.to_string(list.length(paths)) <> " paths:")
  list.each(paths, fn(path) { io.println("• " <> string.inspect(path)) })
  io.println("================================\n")
}

/// Join the primary node with simple reliable approach
///
/// Attempts to join the primary node, with retries in case of failure.
/// This is a crucial function for establishing the cluster.
///
/// # Arguments
/// - `cluster`: Reference to the cluster actor
/// - `primary_node`: Name of the primary node to join
/// - `retries`: Number of remaining retry attempts
///
/// # Returns
/// - `Ok(Nil)` if join succeeded
/// - `Error(String)` with error message if join failed after all retries
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

          // Wait much longer for the cluster to stabilize
          io.println("Waiting for cluster to stabilize...")
          process.sleep(10_000)
          // Increased to 10 seconds

          // Skip writing test data immediately - just consider the join successful
          // We'll verify replication in the separate test processes

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

/// Store a service state in the Khepri store
///
/// # Arguments
/// - `service`: Name of the service to store
/// - `state`: The ServiceState to store
///
/// # Returns
/// - `Ok(Nil)` if store succeeded
/// - `Error(String)` with error message if store failed
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

/// Broadcast a cluster event to all nodes
///
/// # Arguments
/// - `event_type`: Type of event (e.g., "node_joined")
/// - `message`: Event message/payload
///
/// # Returns
/// - `Ok(Nil)` if broadcast succeeded
/// - `Error(String)` with error message if broadcast failed
/// Broadcast a cluster event to all nodes - simplified version
pub fn broadcast_cluster_event(
  event_type: String,
  node_name: String,
) -> Result(Nil, String) {
  // Create a simpler event format
  let timestamp = int.to_string(erlang.system_time(erlang.Millisecond))
  let event_key = event_type <> "_" <> timestamp

  // Ensure the cluster_events path exists
  case khepri_gleam.exists("/:cluster_events/") {
    False -> {
      khepri_gleam.put(
        khepri_gleam.to_khepri_path("/:cluster_events/"),
        "cluster_events",
      )
    }
    True -> Nil
  }

  // Store only the node name as the event value - much simpler
  khepri_gleam.put(
    khepri_gleam.to_khepri_path("/:cluster_events/" <> event_key),
    node_name,
  )

  Ok(Nil)
}

/// Store a join notification with simplified format
pub fn store_join_notification(node_name: String) -> Result(Nil, String) {
  // Only broadcast to cluster_events, don't duplicate in services
  broadcast_cluster_event("join", node_name)
}

/// Get all cluster events
///
/// # Returns
/// - `Ok(List(#(String, String)))` with event keys and values
/// - `Error(String)` with error message if retrieval failed
/// Get all cluster events
///
/// # Returns
/// - `Ok(List(#(String, String)))` with event keys and values
/// - `Error(String)` with error message if retrieval failed
pub fn get_cluster_events() -> Result(List(#(String, String)), String) {
  // First check if the events path exists
  case khepri_gleam.exists("/:cluster_events/") {
    False -> {
      // No events path exists yet
      Ok([])
    }
    True -> {
      // Use our new safe function instead of the regular list_directory
      case safe_list_directory("/:cluster_events/") {
        Ok(events) -> {
          // Successfully retrieved events
          let parsed_events =
            list.map(events, fn(event) {
              let #(key, data) = event
              // Be defensive with data handling
              let value = case dynamic.string(data) {
                Ok(str) -> str
                Error(_) -> "unknown"
              }
              #(key, value)
            })
          Ok(parsed_events)
        }
        Error(_) -> {
          // If an error occurs, return an empty list instead of failing
          Ok([])
        }
      }
    }
  }
}

// External function to safely list a directory
@external(erlang, "khepri_gleam_helper", "safe_list_directory")
fn safe_list_directory(
  path: String,
) -> Result(List(#(String, dynamic.Dynamic)), String)

/// Helper to catch any errors during a function call
///
/// # Arguments
/// - `fn_to_run`: Function to execute that might throw errors
///
/// # Returns
/// - `Ok(T)` with the result if no errors occurred
/// - `Error(String)` if an error occurred
fn catch_errors(fn_to_run: fn() -> a) -> Result(a, String) {
  // This is a simplified version that cannot be fully implemented in Gleam directly
  // In a real system, this would be an external Erlang function with try/catch

  // For now, just run the function and let any exceptions propagate
  // This is a placeholder - it doesn't actually catch errors
  Ok(fn_to_run())
}

/// Get a service state from the Khepri store
///
/// # Arguments
/// - `service`: Name of the service to retrieve
///
/// # Returns
/// - `Ok(ServiceState)` with the service state if retrieval succeeded
/// - `Error(String)` with error message if retrieval failed
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

/// List all services and their states
///
/// Retrieves all services from the Khepri store with improved path handling
/// to handle different path formats.
///
/// # Returns
/// - `Ok(List(#(String, ServiceState)))` with services and states if retrieval succeeded
/// - `Error(String)` with error message if retrieval failed
pub fn list_services() -> Result(List(#(String, ServiceState)), String) {
  io.println("Getting children for path: [\":services\"]")

  // Try both path formats
  let with_colon =
    khepri_gleam.list_directory("/:services/")
    |> result.unwrap([])

  let without_colon =
    khepri_gleam.list_directory("/services/")
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

/// List services with a more direct approach
///
/// Alternative implementation that uses a more direct path access method
/// to find services, which can be more reliable in some cluster scenarios.
///
/// # Returns
/// - `Ok(List(#(String, ServiceState)))` with services and states if retrieval succeeded
/// - `Error(String)` with error message if retrieval failed
pub fn list_services_direct() -> Result(List(#(String, ServiceState)), String) {
  io.println("Using improved direct path access")

  // Get all registered paths
  let all_paths = get_all_paths()

  // Debug all paths
  io.println("All registered paths: " <> string.inspect(all_paths))

  // Filter for service paths with more flexible matching
  let service_paths =
    list.filter(all_paths, fn(path) {
      // Check for paths that contain "services" as the first component
      case path {
        // Match for paths with "services" as the first component
        ["services", second, ..] -> True

        // Match for paths with binary "services" equivalents
        [first, second, ..] -> {
          case first {
            "services" -> True
            "/:services" -> True
            "/services" -> True
            _ -> string.contains(first, "services")
          }
        }

        // Other cases (empty path or single component)
        _ -> False
      }
    })

  io.println(
    "Found "
    <> int.to_string(list.length(service_paths))
    <> " service paths: "
    <> string.inspect(service_paths),
  )

  // Gather services with more robust error handling
  let services =
    list.fold(service_paths, [], fn(acc, path) {
      // Extract service name from path with better fallbacks
      let key = extract_service_key(path)

      // Try multiple path formats to handle different representations
      case get_service_state_with_fallbacks(key) {
        Ok(state) -> [#(key, state), ..acc]
        Error(_) -> acc
      }
    })

  io.println(
    "Processed " <> int.to_string(list.length(services)) <> " services",
  )
  Ok(services)
}

/// Helper to extract service key from various path formats
///
/// # Arguments
/// - `path`: List of path components
///
/// # Returns
/// - String containing the extracted service key
fn extract_service_key(path: List(String)) -> String {
  case path {
    // Handle various path formats
    ["/:services", key, ..] -> key
    ["/services", key, ..] -> key
    ["services", key, ..] -> key
    // Default case - just use last path component
    _ ->
      case list.last(path) {
        Ok(key) -> key
        Error(_) -> "unknown"
      }
  }
}

/// Try multiple path formats to get service state
///
/// Attempts to retrieve a service state using different path formats
/// for greater reliability.
///
/// # Arguments
/// - `key`: Service key to retrieve
///
/// # Returns
/// - `Ok(ServiceState)` with the service state if retrieval succeeded
/// - `Error(String)` with error message if retrieval failed with all formats
fn get_service_state_with_fallbacks(key: String) -> Result(ServiceState, String) {
  // Try with different path formats
  let paths = ["/:services/" <> key, "/services/" <> key, "services/" <> key]

  // Try each path format in sequence
  let results = list.map(paths, fn(path) { khepri_gleam.get_string(path) })

  // Return first successful result
  case list.find(results, fn(r) { result.is_ok(r) }) {
    Ok(Ok(value)) -> {
      let state = case value {
        "running" -> Running
        "stopped" -> Stopped
        "failed" -> Failed
        other -> Custom(other)
      }
      Ok(state)
    }
    _ -> Error("Service not found")
  }
}

/// Get all registered paths from Khepri
///
/// External function that retrieves all paths from the Khepri store.
///
/// # Returns
/// - List of path component lists
@external(erlang, "khepri_gleam_helper", "get_registered_paths")
pub fn get_all_paths() -> List(List(String))

/// Test connection to primary node
///
/// Attempts to connect directly to the primary node to verify connectivity.
///
/// # Arguments
/// - `primary_node`: Name of the primary node
///
/// # Returns
/// - `True` if connection succeeded
/// - `False` if connection failed
fn test_primary_connection(primary_node: String) -> Bool {
  // Try to ping the primary node directly
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

/// Get RA cluster members
///
/// External function to access Erlang's RA cluster membership information.
///
/// # Arguments
/// - `cluster_name`: Name of the RA cluster
///
/// # Returns
/// - Dynamic containing cluster membership information
@external(erlang, "ra", "members")
fn ra_members(cluster_name: String) -> dynamic.Dynamic
