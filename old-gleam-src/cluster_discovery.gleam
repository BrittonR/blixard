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

/// Find nodes with multiple possible prefixes
pub fn find_nodes() -> List(String) {
  // Look for both service_cli nodes and khepri_node nodes
  let prefixes = ["service_cli", "khepri_node", "blixard"]

  io.println(
    "Discovering cluster nodes with prefixes: " <> string.inspect(prefixes),
  )

  // Find local nodes with any of these prefixes
  let local_nodes =
    list.flat_map(prefixes, fn(prefix) { find_local_nodes(prefix) })
    |> list.unique

  // Also try Tailscale nodes
  let tailscale_nodes =
    list.flat_map(prefixes, fn(prefix) { find_tailscale_nodes(prefix) })
    |> list.unique

  let all_nodes =
    list.append(local_nodes, tailscale_nodes)
    |> list.unique

  io.println("Found nodes: " <> string.inspect(all_nodes))
  all_nodes
}

/// Find any Khepri cluster nodes regardless of name
pub fn find_khepri_nodes() -> List(String) {
  io.println("Looking for any Khepri cluster nodes...")

  // First, get all local nodes from epmd
  case shellout.command(run: "epmd", with: ["-names"], in: ".", opt: []) {
    Ok(output) -> {
      // Parse all nodes, not just ones with specific prefix
      output
      |> string.trim
      |> string.split("\n")
      |> list.filter(fn(line) { string.contains(line, "name") })
      |> list.filter_map(fn(line) {
        case string.split(line, " ") {
          ["name", node_name, "at", ..] -> {
            let fqdn = node_name <> "@127.0.0.1"
            Ok(fqdn)
          }
          _ -> Error(Nil)
        }
      })
    }
    Error(_) -> []
  }
}

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
/// Finds all nodes and attempts to connect to each one.
///
/// # Returns
/// - Number of nodes successfully connected
pub fn connect_to_all_nodes() -> Int {
  let nodes = find_nodes()

  io.println("Found potential nodes: " <> string.inspect(nodes))

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

/// Find local nodes with a specific prefix
pub fn find_local_nodes(prefix: String) -> List(String) {
  io.println("Running EPMD to find local nodes with prefix: " <> prefix)

  case shellout.command(run: "epmd", with: ["-names"], in: ".", opt: []) {
    Ok(output) -> {
      io.println("Raw epmd output:\n" <> output)

      output
      |> string.trim
      |> string.split("\n")
      |> list.filter(fn(line) {
        case string.contains(line, "name") {
          True -> True
          False -> False
        }
      })
      |> list.filter_map(fn(line) {
        case string.split(line, " ") {
          ["name", node_name, "at", ..] -> {
            io.println("Parsed node: " <> node_name)

            case string.starts_with(node_name, prefix) {
              True -> {
                let fqdn = node_name <> "@127.0.0.1"
                io.println(
                  "Matched node with prefix '" <> prefix <> "': " <> fqdn,
                )
                Ok(fqdn)
              }
              False -> {
                io.println("Skipping node (prefix mismatch): " <> node_name)
                Error(Nil)
              }
            }
          }
          _ -> {
            io.println("Skipping line (unexpected format): " <> line)
            Error(Nil)
          }
        }
      })
    }
    Error(#(_, msg)) -> {
      io.println("Error running epmd -names: " <> msg)
      []
    }
  }
}
