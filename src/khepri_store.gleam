// src/khepri_store.gleam
import gleam/dynamic
import gleam/erlang/atom
import gleam/erlang/node
import gleam/erlang/process
import gleam/io
import gleam/list
import gleam/result
import gleam/string
import khepri_gleam
import khepri_gleam_cluster

// Type representing service state

// Cluster configuration
pub type ClusterConfig {
  ClusterConfig(node_role: NodeRole, cookie: String)
}

// Node role in the cluster
pub type NodeRole {
  Primary
  // First node, starts the cluster
  Secondary(primary_node: String)
  // Joins existing cluster
}

// Initialize a standalone Khepri instance (non-clustered)
pub fn init() -> Result(Nil, String) {
  // Start Khepri
  io.println("Initializing Khepri store...")
  khepri_gleam.start()

  // Create service state directory if it doesn't exist
  let services_path = khepri_gleam.to_khepri_path("/:services")
  case khepri_gleam.exists("/:services") {
    False -> {
      khepri_gleam.put(services_path, "service_states")
      io.println("Created services storage")
    }
    True -> io.println("Services storage exists")
  }

  Ok(Nil)
}

// Initialize Khepri with clustering support
pub fn init_cluster(config: ClusterConfig) -> Result(Nil, String) {
  // Start Khepri first
  io.println("Starting Khepri...")
  khepri_gleam.start()

  // Create services path just in case
  let services_path = khepri_gleam.to_khepri_path("/:services")
  case khepri_gleam.exists("/:services") {
    False -> {
      khepri_gleam.put(services_path, "service_states")
      io.println("Created services storage")
    }
    True -> Nil
  }

  // Start the cluster actor
  case khepri_gleam_cluster.start() {
    Ok(cluster) -> {
      // Handle cluster based on role
      case config.node_role {
        Primary -> {
          // Primary node - wait for leader election
          io.println("Running as primary node")
          let _ = khepri_gleam_cluster.wait_for_leader(5000)
          io.println("Leader election complete")
          Ok(Nil)
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

// Join primary node with retries
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

// Store a service state
pub fn store_service_state(
  service: String,
  state: ServiceState,
) -> Result(Nil, String) {
  let path = "/:services/" <> service
  let state_string = case state {
    Running -> "running"
    Stopped -> "stopped"
    Failed -> "failed"
    Unknown -> "unknown"
    Custom(message) -> message
    // Handle the Custom variant
  }
  let khepri_path = khepri_gleam.to_khepri_path(path)
  khepri_gleam.put(khepri_path, state_string)

  Ok(Nil)
}

// Get a service state
pub fn get_service_state(service: String) -> Result(ServiceState, String) {
  let path = "/:services/" <> service

  case khepri_gleam.get_string(path) {
    Ok("running") -> Ok(Running)
    Ok("stopped") -> Ok(Stopped)
    Ok("failed") -> Ok(Failed)
    Ok(_) -> Ok(Unknown)
    Error(e) -> Error(e)
  }
}

// List all services and their states
pub fn list_services() -> Result(List(#(String, ServiceState)), String) {
  case khepri_gleam.list_directory("/:services") {
    Ok(items) -> {
      let service_states =
        items
        |> list.map(fn(item) {
          let #(name, data) = item

          // Convert data to string if it's dynamic
          let state_str = case dynamic.string(data) {
            Ok(str) -> str
            Error(_) -> "unknown"
          }

          let state = case state_str {
            "running" -> Running
            "stopped" -> Stopped
            "failed" -> Failed
            _ -> Unknown
          }

          #(name, state)
        })

      Ok(service_states)
    }
    Error(e) -> Error(e)
  }
}

// Add to khepri_store.gleam
pub fn store_join_notification(
  key: String,
  message: String,
) -> Result(Nil, String) {
  // Store in a special path for join notifications
  // This needs to be a path that's compatible with your existing store functions

  // We'll use a custom state type that can handle strings for join messages
  // Assuming you have service states like Running, Stopped, etc.
  store_service_state(key, Custom(message))
}

// Add this to your ServiceState type (if not already there)
pub type ServiceState {
  Running
  Stopped
  Failed
  Unknown
  // Add this new option:
  Custom(String)
}
