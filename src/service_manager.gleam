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
  let args = erlang.start_arguments()

  io.println("Processing arguments: " <> string.inspect(args))

  case args {
    ["--join-cluster"] -> {
      node_manager.start_cluster_node(None)
      Nil
    }
    ["--join-cluster", seed_node] -> {
      node_manager.start_cluster_node(Some(seed_node))
      Nil
    }
    ["--stop-cluster"] -> {
      node_manager.stop_cluster()
      Nil
    }
    ["cleanup"] -> {
      // Special case: cleanup doesn't need cluster connection
      cli.handle_command(args)
      Nil
    }
    ["--init-primary"] -> {
      node_manager.start_cluster_node(None)
      Nil
    }
    ["--init-secondary", primary_node] -> {
      node_manager.start_cluster_node(Some(primary_node))
      Nil
    }
    _ -> {
      run_cli_command_with_cluster(args)
    }
  }
}

/// Run a CLI command after ensuring we're connected to the cluster
fn run_cli_command_with_cluster(args: List(String)) -> Nil {
  io.println("Setting up distributed mode for CLI command...")

  // Ensure Erlang distribution is started
  node_manager.ensure_distribution()

  // Check if this is a client command
  case is_client_command(args) {
    True -> {
      // Client mode - DON'T start Khepri, just process the command
      io.println("Running in client mode (no local Khepri)")

      // Try to connect to cluster nodes first
      let _ = connect_to_cluster_for_client()

      // Process the command
      cli.handle_command(args)
    }
    False -> {
      // Full node mode - existing logic
      let cluster_connection_result = connect_to_existing_cluster()

      case cluster_connection_result {
        Ok(_) -> {
          io.println(
            "Successfully connected to cluster, running CLI command...",
          )
          process.sleep(2000)
          cli.handle_command(args)
        }
        Error(err) -> {
          io.println("Could not connect to cluster: " <> err)
          io.println("Running in standalone mode...")

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
  }
}

fn is_client_command(args: List(String)) -> Bool {
  // Use the centralized function from khepri_store
  khepri_store.should_run_as_client(args)
}

fn connect_to_cluster_for_client() -> Nil {
  // Just ensure we can see cluster nodes
  let nodes = cluster_discovery.find_khepri_nodes()

  list.each(nodes, fn(node_name) {
    let _ = cluster_discovery.connect_to_node(node_name)
    Nil
  })

  io.println(
    "Connected to "
    <> int.to_string(list.length(node.visible()))
    <> " cluster nodes",
  )
}

/// Try to connect to an existing cluster with better discovery
fn connect_to_existing_cluster() -> Result(Nil, String) {
  let discovered_nodes = discover_all_cluster_nodes()

  case list.length(discovered_nodes) > 0 {
    True -> {
      io.println(
        "Found "
        <> int.to_string(list.length(discovered_nodes))
        <> " potential cluster nodes",
      )
      try_connect_to_nodes(discovered_nodes)
    }
    False -> {
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
          try_connect_to_nodes(rest)
        }
      }
    }
  }
}

/// Discover all possible cluster nodes
fn discover_all_cluster_nodes() -> List(String) {
  let discovered = cluster_discovery.find_khepri_nodes()
  let visible =
    node.visible()
    |> list.map(fn(n) { atom.to_string(node.to_atom(n)) })

  list.append(discovered, visible)
  |> list.unique
  |> list.filter(fn(node_name) {
    let self_name = atom.to_string(node.to_atom(node.self()))
    node_name != self_name
  })
}
