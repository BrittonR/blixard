// src/node_manager.gleam
//
// Node management for the distributed cluster
//
// This module handles Erlang node distribution and cluster management,
// including starting nodes and ensuring distributed operation for CLI commands.

import cluster_discovery
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
import khepri_store
import replication_monitor

/// Standard prefix for all Blixard nodes
const node_prefix = "blixard"

/// External function to start Erlang distribution
/// Defined in src/distribution_helper.erl
@external(erlang, "distribution_helper", "start_distribution")
fn start_distribution(
  node_name: String,
  hostname: String,
  cookie: String,
) -> Nil

/// Ensure the current process is running in distributed mode
///
/// This function is used by CLI commands to ensure they can
/// communicate with the Khepri cluster nodes.
///
/// # Effects
/// - Starts Erlang distribution if not already started
/// - Discovers and connects to other nodes in the cluster
pub fn ensure_distribution() -> Nil {
  let current_node = node.self()
  let node_name = atom.to_string(node.to_atom(current_node))

  // Check if already distributed
  case node_name == "nonode@nohost" {
    True -> {
      // Start distribution using consistent naming
      let timestamp = int.to_string(erlang.system_time(erlang.Millisecond))
      let name = node_prefix <> "_cli_" <> timestamp
      let hostname = "127.0.0.1"
      let cookie = "khepri_cookie"

      start_distribution(name, hostname, cookie)

      // Verify distribution started
      let new_node = node.self()
      let new_name = atom.to_string(node.to_atom(new_node))

      case new_name == "nonode@nohost" {
        True -> {
          io.println(
            "WARNING: Failed to start distribution, functionality will be limited",
          )
        }
        False -> {
          io.println("Running as distributed node: " <> new_name)

          // Try to discover and connect to other nodes
          connect_to_cluster_nodes()
        }
      }
    }
    False -> {
      io.println("Already running in distributed mode: " <> node_name)
      // Still try to connect to cluster nodes
      connect_to_cluster_nodes()
    }
  }
}

/// Try to connect to other cluster nodes
fn connect_to_cluster_nodes() -> Nil {
  let discovered = cluster_discovery.find_khepri_nodes()
  let connected_count =
    list.fold(discovered, 0, fn(count, node_name) {
      case cluster_discovery.connect_to_node(node_name) {
        True -> count + 1
        False -> count
      }
    })

  case connected_count > 0 {
    True ->
      io.println(
        "Connected to " <> int.to_string(connected_count) <> " cluster nodes",
      )
    False -> io.println("No cluster nodes found to connect to")
  }
}

/// Start a node and join or create a cluster
///
/// If seed_node is provided, connect to that node.
/// If no seed_node is provided, attempt to discover existing nodes.
/// If no nodes are found, become the first node in the cluster.
///
/// # Arguments
/// - `seed_node`: Optional seed node to connect to
pub fn start_cluster_node(seed_node: Option(String)) -> Nil {
  io.println("Starting Khepri cluster node...")

  // Use consistent naming for cluster nodes
  let timestamp = int.to_string(erlang.system_time(erlang.Millisecond))
  let name = node_prefix <> "_node_" <> timestamp
  let hostname = "127.0.0.1"
  let cookie = "khepri_cookie"

  // Start distribution with consistent name
  start_distribution(name, hostname, cookie)

  // Get our own node name
  let current_node = node.self()
  let current_node_name = atom.to_string(node.to_atom(current_node))

  io.println("Started as node: " <> current_node_name)

  // Try to discover existing nodes if no seed node was provided
  let discovered_nodes = case seed_node {
    None -> {
      // Find nodes but exclude ourselves
      cluster_discovery.find_khepri_nodes()
      |> list.filter(fn(node_name) { node_name != current_node_name })
    }
    Some(node) -> [node]
  }

  // Initialize based on whether we found existing nodes
  case list.length(discovered_nodes) > 0 {
    True -> {
      // We found nodes, join as secondary
      let primary_node =
        list.first(discovered_nodes)
        |> result.unwrap("unknown_node")

      io.println("Found existing cluster node: " <> primary_node)

      let config =
        khepri_store.ClusterConfig(
          node_role: khepri_store.Secondary(primary_node),
          cookie: "khepri_cookie",
        )

      join_existing_cluster(config)
    }
    False -> {
      // No nodes found, initialize as primary
      io.println("No existing cluster nodes found. Initializing as first node.")

      let config =
        khepri_store.ClusterConfig(
          node_role: khepri_store.Primary,
          cookie: "khepri_cookie",
        )

      initialize_as_first_node(config)
    }
  }
}

// Initialize as the first node in the cluster
fn initialize_as_first_node(config: khepri_store.ClusterConfig) -> Nil {
  case khepri_store.init_cluster(config) {
    Ok(_) -> {
      io.println("First cluster node started successfully")
      io.println("Waiting for connections...")

      // Start the cluster event monitor
      replication_monitor.start_cluster_event_monitor()

      // Keep the process running
      process.sleep_forever()
    }
    Error(err) -> {
      io.println_error("Failed to start first node: " <> err)
      Nil
    }
  }
}

// Join an existing cluster
fn join_existing_cluster(config: khepri_store.ClusterConfig) -> Nil {
  case khepri_store.init_cluster(config) {
    Ok(_) -> {
      io.println("Successfully joined the cluster")

      // Get current node name
      let current_node = node.self()
      let node_name = atom.to_string(node.to_atom(current_node))

      // Broadcast join with simplified format
      let _ = khepri_store.store_join_notification(node_name)

      // Start the cluster event monitor
      replication_monitor.start_cluster_event_monitor()

      io.println("Node is ready and connected to the cluster")

      // Keep the process running
      process.sleep_forever()
    }
    Error(err) -> {
      io.println_error("Failed to join cluster: " <> err)
      Nil
    }
  }
}

/// Start a primary node for the Khepri cluster (DEPRECATED)
/// Use start_cluster_node(None) instead
///
/// The primary node is responsible for:
/// - Initializing the Khepri cluster
/// - Participating in leader election
/// - Handling replication to secondary nodes
pub fn start_primary_node() -> Nil {
  io.println("DEPRECATED: Please use start_cluster_node instead")
  start_cluster_node(None)
}

/// Start a secondary node for the Khepri cluster (DEPRECATED)
/// Use start_cluster_node(Some(primary_node)) instead
///
/// The secondary node:
/// - Connects to the primary node
/// - Joins the Khepri cluster
/// - Replicates data from the primary
/// - Can serve read requests
pub fn start_secondary_node(primary_node: String) -> Nil {
  io.println("DEPRECATED: Please use start_cluster_node instead")
  start_cluster_node(Some(primary_node))
}

/// External function to call init:stop on a remote node
@external(erlang, "node_manager_helper", "remote_stop")
fn remote_stop(node_name: String) -> Result(Nil, String)

/// Stop any running cluster nodes
///
/// Discovers running cluster nodes and sends a shutdown signal
pub fn stop_cluster() -> Nil {
  io.println("Looking for cluster nodes to stop...")

  // Start distribution so we can communicate with other nodes
  ensure_distribution()

  // Find running nodes
  let discovered_nodes = cluster_discovery.find_khepri_nodes()

  case list.length(discovered_nodes) {
    0 -> {
      io.println("No cluster nodes found.")
      Nil
    }
    count -> {
      io.println("Found " <> int.to_string(count) <> " cluster nodes.")

      // Send shutdown signal to each node
      list.each(discovered_nodes, fn(node_name) {
        io.println("Sending shutdown signal to " <> node_name)

        // Try to connect and send shutdown signal
        case remote_stop(node_name) {
          Ok(_) -> io.println("✅ Shutdown signal sent to " <> node_name)
          Error(err) ->
            io.println(
              "❌ Failed to send shutdown signal to " <> node_name <> ": " <> err,
            )
        }
      })

      io.println("Shutdown process completed.")
      Nil
    }
  }
}
