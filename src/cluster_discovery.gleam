// src/cluster_discovery.gleam
//
// Cluster node discovery and connection
//
// This module handles discovery of other nodes in the cluster using
// Tailscale network, and establishing connections between nodes.
// It provides functionality to find and connect to other nodes
// in the distributed system.

import gleam/erlang/atom
import gleam/erlang/node
import gleam/int
import gleam/io
import gleam/list
import gleam/string
import shellout.{LetBeStderr, LetBeStdout}

/// Find Tailscale hosts using DNS
///
/// Uses the Tailscale CLI to discover other hosts on the Tailscale network,
/// which can be used as potential nodes in our cluster.
///
/// # Arguments
/// - `base_name`: Base name for the node (e.g., "khepri_node")
///
/// # Returns
/// - List of fully qualified node names (e.g., "khepri_node@hostname.local")
pub fn find_tailscale_nodes(base_name: String) -> List(String) {
  io.println("Searching for Tailscale hosts...")

  // Try to execute the tailscale command to get active peers
  case
    shellout.command(
      run: "sh",
      with: [
        "-c",
        "tailscale status --json | jq -r '.Peer[] | select(.Active) | .HostName'",
      ],
      in: ".",
      opt: [],
    )
  {
    Ok(output) -> {
      // Parse the output into a list of hostnames
      let hostnames =
        output
        |> string.trim
        |> string.split("\n")
        |> list.filter(fn(s) { !string.is_empty(s) })

      // Convert hostnames to full node names
      list.map(hostnames, fn(hostname) {
        base_name <> "@" <> hostname <> ".local"
      })
    }
    Error(#(_, message)) -> {
      io.println(
        "Failed to get Tailscale hosts, using local node only: " <> message,
      )
      []
    }
  }
}

/// Connect to a specific node
///
/// Attempts to establish a connection to another Erlang node.
///
/// # Arguments
/// - `node_name`: Fully qualified name of the node to connect to
///
/// # Returns
/// - `True` if connection succeeded
/// - `False` if connection failed
pub fn connect_to_node(node_name: String) -> Bool {
  // Convert string to atom for Erlang node module
  let node_atom = atom.create_from_string(node_name)

  // Attempt to connect
  case node.connect(node_atom) {
    Ok(_) -> {
      io.println("Connected to node: " <> node_name)
      True
    }
    Error(_) -> {
      io.println("Failed to connect to node: " <> node_name)
      False
    }
  }
}

/// Connect to all discovered nodes
///
/// Finds all Tailscale hosts and attempts to connect to each one.
///
/// # Arguments
/// - `base_name`: Base name for the nodes (e.g., "khepri_node")
///
/// # Returns
/// - Number of nodes successfully connected
pub fn connect_to_all_nodes(base_name: String) -> Int {
  // Find potential nodes using our enhanced discovery
  let nodes = find_nodes(base_name)

  io.println("Found potential nodes: " <> string.inspect(nodes))

  // Try to connect to each node
  let connected_count =
    list.fold(nodes, 0, fn(count, node_name) {
      case connect_to_node(node_name) {
        True -> count + 1
        False -> count
      }
    })

  io.println("Connected to " <> int.to_string(connected_count) <> " nodes")
  connected_count
}

pub fn find_nodes(base_name: String) -> List(String) {
  io.println("Searching for nodes...")

  // Try to find Tailscale hosts first (for multi-machine setups)
  let tailscale_nodes = find_tailscale_nodes(base_name)

  // Add local discovery for development
  let local_nodes = find_local_nodes(base_name)

  // Combine both node lists
  list.append(tailscale_nodes, local_nodes)
}

pub fn find_local_nodes(base_name: String) -> List(String) {
  // Try to find locally running Erlang nodes
  case shellout.command(run: "epmd", with: ["-names"], in: ".", opt: []) {
    Ok(output) -> {
      io.println("EPMD output: " <> output)

      // Parse the epmd output to extract node names
      // The output format is like:
      // epmd: up and running on port 4369 with data:
      // name khepri_node at port 52641

      output
      |> string.split("\n")
      |> list.filter(fn(line) { string.contains(line, "name") })
      |> list.map(fn(line) {
        case string.split(line, " ") {
          ["name", node_name, ..] ->
            // Construct the full node name
            node_name <> "@127.0.0.1"
          _ -> ""
        }
      })
      |> list.filter(fn(name) { !string.is_empty(name) })
    }
    Error(_) -> {
      // If epmd command fails, try the direct approach
      // with the most common node name format
      [base_name <> "@127.0.0.1"]
    }
  }
}
