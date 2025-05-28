// src/service_manager.gleam
import cli
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
      case khepri_store.init() {
        Ok(_) -> {
          io.println("Initialized standalone Khepri store")
          cli.handle_command(args)
        }
        Error(init_err) -> {
          io.println("Failed to initialize standalone store: " <> init_err)
        }
      }
    }
  }
}

/// Try to connect to an existing cluster with better discovery
fn connect_to_existing_cluster() -> Result(Nil, String) {
  // Try multiple discovery methods
  let discovered_nodes = discover_all_cluster_nodes()

  case list.length(discovered_nodes) > 0 {
    True -> {
      io.println(
        "Found "
        <> int.to_string(list.length(discovered_nodes))
        <> " potential cluster nodes",
      )

      // Try to connect to each node until one succeeds
      try_connect_to_nodes(discovered_nodes)
    }
    False -> {
      // No nodes found, but check if we're already connected
      let connected = node.visible()
      case list.length(connected) > 0 {
        True -> {
          io.println("Already connected to cluster nodes")
          Ok(Nil)
        }
        False -> Error("No cluster nodes found")
      }
    }
  }
}

/// Try to connect to a list of nodes
fn try_connect_to_nodes(nodes: List(String)) -> Result(Nil, String) {
  case nodes {
    [] -> Error("Failed to connect to any nodes")
    [node_name, ..rest] -> {
      io.println("Attempting to connect to: " <> node_name)

      // Try to join this node's Khepri cluster
      let config =
        khepri_store.ClusterConfig(
          node_role: khepri_store.Secondary(node_name),
          cookie: "khepri_cookie",
        )

      case khepri_store.init_cluster(config) {
        Ok(_) -> {
          io.println("Successfully joined cluster via " <> node_name)
          Ok(Nil)
        }
        Error(err) -> {
          io.println("Failed to join via " <> node_name <> ": " <> err)
          // Try the next node
          try_connect_to_nodes(rest)
        }
      }
    }
  }
}

/// Discover all possible cluster nodes
fn discover_all_cluster_nodes() -> List(String) {
  // Get all nodes from various sources
  let discovered = cluster_discovery.find_khepri_nodes()
  let visible =
    node.visible()
    |> list.map(fn(n) { atom.to_string(node.to_atom(n)) })

  // Combine and deduplicate
  list.append(discovered, visible)
  |> list.unique
  |> list.filter(fn(node_name) {
    // Filter out our own node
    let self_name = atom.to_string(node.to_atom(node.self()))
    node_name != self_name
  })
}
