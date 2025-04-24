// src/node_manager.gleam
//
// Node management for the distributed cluster
//
// This module handles Erlang node distribution and cluster management,
// including starting primary and secondary nodes and ensuring
// distributed operation for CLI commands.

import cluster_discovery
import gleam/erlang
import gleam/erlang/atom
import gleam/erlang/node
import gleam/erlang/process
import gleam/int
import gleam/io
import gleam/list
import khepri_gleam
import khepri_gleam_cluster
import khepri_store
import replication_monitor

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
      // Start distribution using our helper
      // Generate a unique CLI name with timestamp
      let name =
        "service_cli_" <> int.to_string(erlang.system_time(erlang.Millisecond))
      let hostname = "127.0.0.1"
      // Use IP address instead of "localhost"
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
          Nil
        }
        False -> {
          io.println("Running as distributed node: " <> new_name)

          // Try to discover and connect to other nodes
          let _ = cluster_discovery.connect_to_all_nodes("khepri_node")
          Nil
        }
      }
    }
    False -> {
      io.println("Already running in distributed mode: " <> node_name)
      Nil
    }
  }
}

/// Start a primary node for the Khepri cluster
///
/// The primary node is responsible for:
/// - Initializing the Khepri cluster
/// - Participating in leader election
/// - Handling replication to secondary nodes
///
/// # Effects
/// - Starts Erlang distribution
/// - Initializes the Khepri cluster as primary
/// - Starts monitoring processes
/// - Keeps the node running indefinitely
pub fn start_primary_node() -> Nil {
  io.println("Starting primary Khepri node...")

  // Start distribution
  ensure_distribution()

  // Initialize the Khepri cluster as primary
  let config =
    khepri_store.ClusterConfig(
      node_role: khepri_store.Primary,
      cookie: "khepri_cookie",
    )

  case khepri_store.init_cluster(config) {
    Ok(_) -> {
      io.println("Primary Khepri node started successfully")
      io.println("Waiting for connections...")

      // Start monitoring for new nodes
      replication_monitor.start_node_monitor()

      // Start continuous monitoring
      replication_monitor.start_continuous_replication_monitor()

      // Start primary write test
      replication_monitor.start_primary_write_test()

      // Keep the process running
      process.sleep_forever()
    }
    Error(err) -> {
      io.println_error("Failed to start primary node: " <> err)
      Nil
    }
  }
}

/// Start a secondary node for the Khepri cluster
///
/// The secondary node:
/// - Connects to the primary node
/// - Joins the Khepri cluster
/// - Replicates data from the primary
/// - Can serve read requests
///
/// # Arguments
/// - `primary_node`: Erlang node name of the primary node
///
/// # Effects
/// - Starts Erlang distribution
/// - Initializes the Khepri cluster as secondary
/// - Joins the cluster with the primary
/// - Starts monitoring processes
/// - Keeps the node running indefinitely
pub fn start_secondary_node(primary_node: String) -> Nil {
  io.println("Starting secondary Khepri node...")
  io.println("Primary node: " <> primary_node)

  // Start distribution
  ensure_distribution()

  // Initialize the Khepri cluster as secondary
  let config =
    khepri_store.ClusterConfig(
      node_role: khepri_store.Secondary(primary_node),
      cookie: "khepri_cookie",
    )

  case khepri_store.init_cluster(config) {
    Ok(_) -> {
      io.println("Secondary Khepri node started successfully")
      io.println("Connected to primary node")

      // Write join data using the existing store_service_state function
      let current_node = node.self()
      let node_name = atom.to_string(node.to_atom(current_node))
      let timestamp = int.to_string(erlang.system_time(erlang.Millisecond))

      // Create a join message
      let join_key = "join_" <> node_name
      let join_message = "Node " <> node_name <> " joined at " <> timestamp

      // Store using service state
      io.println("Writing join notification to the cluster...")
      let _ = khepri_store.store_join_notification(join_key, timestamp)

      io.println("Sent join notification to the cluster")

      // Start secondary monitoring
      replication_monitor.start_secondary_read_test()
      replication_monitor.start_replication_test_cycle()

      // Keep the process running
      process.sleep_forever()
    }
    Error(err) -> {
      io.println_error("Failed to start secondary node: " <> err)
      Nil
    }
  }
}
