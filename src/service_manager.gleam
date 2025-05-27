// src/service_manager.gleam
import cli
import gleam/erlang
import gleam/erlang/process
import gleam/io
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string
import khepri_store
import node_manager

/// Main function that parses command line arguments and dispatches
/// to the appropriate operation mode
pub fn main() -> Nil {
  // Get command-line arguments and remove the program name
  let args = erlang.start_arguments()

  io.println("Processing arguments: " <> string.inspect(args))

  // Process args
  case args {
    // Join cluster with auto-discovery
    ["--join-cluster"] -> {
      node_manager.start_cluster_node(None)
      Nil
    }
    // Join cluster with specified seed node
    ["--join-cluster", seed_node] -> {
      node_manager.start_cluster_node(Some(seed_node))
      Nil
    }
    // Stop any running cluster nodes
    ["--stop-cluster"] -> {
      node_manager.stop_cluster()
      Nil
    }
    // Backward compatibility with old commands
    ["--init-primary"] -> {
      node_manager.start_cluster_node(None)
      Nil
    }
    ["--init-secondary", primary_node] -> {
      node_manager.start_cluster_node(Some(primary_node))
      Nil
    }
    // Regular command processing path for CLI operations
    _ -> {
      // NEW: Try to join cluster first, then run the command
      run_cli_command_with_cluster(args)
    }
  }
}

/// Run a CLI command after ensuring we're connected to the cluster
fn run_cli_command_with_cluster(args: List(String)) -> Nil {
  io.println("Setting up distributed mode for CLI command...")

  // Ensure Erlang distribution is started
  node_manager.ensure_distribution()

  // Try to find and connect to existing cluster nodes
  let cluster_connection_result = connect_to_existing_cluster()

  case cluster_connection_result {
    Ok(_) -> {
      io.println("Successfully connected to cluster, running CLI command...")

      // Give the cluster connection time to stabilize
      process.sleep(2000)

      // Process the CLI command
      cli.handle_command(args)
    }
    Error(err) -> {
      io.println("Could not connect to cluster: " <> err)
      io.println("Running in standalone mode...")

      // Initialize standalone Khepri and run command
      let _ = khepri_store.init()
      cli.handle_command(args)
    }
  }
}

/// Try to connect to an existing cluster
fn connect_to_existing_cluster() -> Result(Nil, String) {
  // Try to discover existing cluster nodes
  let discovered_nodes = discover_cluster_nodes()

  case list.length(discovered_nodes) > 0 {
    True -> {
      // Found cluster nodes, try to join
      let primary_node = case list.first(discovered_nodes) {
        Ok(node) -> node
        Error(_) -> "unknown"
      }

      io.println(
        "Found cluster node: " <> primary_node <> ", attempting to join...",
      )

      // Initialize cluster connection
      let config =
        khepri_store.ClusterConfig(
          node_role: khepri_store.Secondary(primary_node),
          cookie: "khepri_cookie",
        )

      case khepri_store.init_cluster(config) {
        Ok(_) -> {
          io.println("Successfully joined cluster")
          Ok(Nil)
        }
        Error(err) -> Error("Failed to join cluster: " <> err)
      }
    }
    False -> Error("No cluster nodes found")
  }
}

/// Discover existing cluster nodes
fn discover_cluster_nodes() -> List(String) {
  // Look for nodes with the "khepri_node" prefix
  cluster_discovery.find_nodes("khepri_node")
}

// Import the cluster discovery module
import cluster_discovery
