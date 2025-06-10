// src/cluster_node.gleam
// Persistent cluster node that runs Khepri
import gleam/erlang
import gleam/erlang/atom
import gleam/erlang/node
import gleam/erlang/process
import gleam/int
import gleam/io
import gleam/option.{None, Some}
import gleam/string
import khepri_store

/// Start a persistent cluster node
pub fn main() -> Nil {
  let args = erlang.start_arguments()

  case args {
    ["primary"] -> start_primary()
    ["secondary", primary_node] -> start_secondary(primary_node)
    _ -> {
      io.println("Usage:")
      io.println("  gleam run -m cluster_node -- primary")
      io.println("  gleam run -m cluster_node -- secondary <primary-node>")
    }
  }
}

fn start_primary() -> Nil {
  io.println("Starting PRIMARY cluster node...")

  // Set up node name
  start_distribution("khepri_primary", "127.0.0.1", "khepri_cookie")

  let config =
    khepri_store.ClusterConfig(
      node_role: khepri_store.Primary,
      cookie: "khepri_cookie",
    )

  case khepri_store.init_cluster(config) {
    Ok(_) -> {
      io.println("✅ Primary node started successfully")
      io.println("Node name: " <> atom.to_string(node.to_atom(node.self())))
      io.println("Waiting for secondary nodes...")

      // Keep running
      process.sleep_forever()
    }
    Error(err) -> {
      io.println("❌ Failed to start primary node: " <> err)
    }
  }
}

fn start_secondary(primary_node: String) -> Nil {
  io.println("Starting SECONDARY cluster node...")

  // Set up node name with unique suffix
  let timestamp = erlang.system_time(erlang.Millisecond)
  start_distribution(
    "khepri_secondary_" <> int.to_string(timestamp),
    "127.0.0.1",
    "khepri_cookie",
  )

  let config =
    khepri_store.ClusterConfig(
      node_role: khepri_store.Secondary(primary_node),
      cookie: "khepri_cookie",
    )

  case khepri_store.init_cluster(config) {
    Ok(_) -> {
      io.println("✅ Secondary node started successfully")
      io.println("Node name: " <> atom.to_string(node.to_atom(node.self())))
      io.println("Connected to: " <> primary_node)

      // Keep running
      process.sleep_forever()
    }
    Error(err) -> {
      io.println("❌ Failed to start secondary node: " <> err)
    }
  }
}

@external(erlang, "distribution_helper", "start_distribution")
fn start_distribution(name: String, host: String, cookie: String) -> Nil
