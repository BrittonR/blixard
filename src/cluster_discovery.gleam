// src/cluster_discovery.gleam
import gleam/erlang/atom
import gleam/erlang/node
import gleam/int
import gleam/io
import gleam/list
import gleam/string
import shellout.{LetBeStderr, LetBeStdout}

// Find tailscale hosts using DNS
pub fn find_tailscale_nodes(base_name: String) -> List(String) {
  io.println("Searching for Tailscale hosts...")

  // Try to execute the tailscale command
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

// Connect to a node
pub fn connect_to_node(node_name: String) -> Bool {
  let node_atom = atom.create_from_string(node_name)

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

// Connect to all discovered nodes
pub fn connect_to_all_nodes(base_name: String) -> Int {
  let nodes = find_tailscale_nodes(base_name)

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
