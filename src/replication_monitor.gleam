// src/replication_monitor.gleam
import gleam/erlang
import gleam/erlang/atom
import gleam/erlang/node
import gleam/erlang/process
import gleam/int
import gleam/io
import gleam/list
import gleam/string
import khepri_gleam
import khepri_gleam_cluster
import khepri_store

// Function to monitor for new nodes joining the cluster
pub fn start_node_monitor() -> Nil {
  // Set up an actor to monitor nodes
  let _ = process.start(fn() { monitor_loop(list.new()) }, True)
  Nil
}

fn monitor_loop(known_nodes: List(String)) -> Nil {
  // Get current connected nodes
  let current_nodes =
    node.visible()
    |> list.map(fn(n) { atom.to_string(node.to_atom(n)) })

  // Find new nodes that weren't in our known list
  let new_nodes =
    list.filter(current_nodes, fn(node_name) {
      !list.contains(known_nodes, node_name)
    })

  // Print notification for each new node
  case list.length(new_nodes) {
    0 -> Nil
    _ -> {
      io.println("\n==== CLUSTER UPDATE ====")
      io.println(
        int.to_string(list.length(new_nodes))
        <> " new node(s) joined the cluster:",
      )

      list.each(new_nodes, fn(node_name) {
        io.println("➕ " <> node_name <> " has joined the cluster!")
      })

      io.println(
        "Total cluster size: "
        <> int.to_string(list.length(current_nodes) + 1)
        <> " nodes",
      )
      io.println("=====================\n")
    }
  }

  // Sleep for a bit, then check again with the updated known node list
  process.sleep(1000)
  monitor_loop(current_nodes)
}

// New function to start cluster event monitor on all nodes
pub fn start_cluster_event_monitor() -> Nil {
  // Set up an actor to monitor cluster events
  let _ = process.start(fn() { cluster_event_monitor(list.new()) }, True)
  Nil
}

// Monitor function that watches for cluster events
fn cluster_event_monitor(known_events: List(String)) -> Nil {
  // Get current cluster events
  case khepri_store.get_cluster_events() {
    Ok(events) -> {
      // Get event keys 
      let event_keys =
        list.map(events, fn(event) {
          let #(key, _) = event
          key
        })

      // Find new events with extra safety checks
      let new_events =
        list.filter(events, fn(event) {
          let #(key, _) = event
          !list.contains(known_events, key)
        })

      // Process new join events
      let join_events =
        list.filter(new_events, fn(event) {
          let #(key, _) = event
          string.contains(key, "join_")
        })

      case list.length(join_events) {
        0 -> Nil
        _ -> {
          io.println("\n==== CLUSTER UPDATE ====")
          list.each(join_events, fn(event) {
            let #(key, node_name) = event
            // Just print the node name directly - much simpler!
            io.println("➕ " <> node_name <> " has joined the cluster!")
          })
          io.println("======================\n")
        }
      }

      // Continue monitoring with updated known events
      process.sleep(3000)
      cluster_event_monitor(event_keys)
    }
    Error(err) -> {
      // Log the error but don't crash
      io.println("Warning: Error getting cluster events: " <> err)
      process.sleep(3000)
      cluster_event_monitor(known_events)
    }
  }
}

pub fn start_replication_test_cycle() -> Nil {
  // Removed since we no longer want read tests
  Nil
}

pub fn start_continuous_replication_monitor() -> Nil {
  // Removed since we no longer want read tests
  Nil
}

pub fn start_primary_write_test() -> Nil {
  // Removed since we no longer want read tests
  Nil
}

pub fn start_secondary_read_test() -> Nil {
  // Removed since we no longer want read tests
  Nil
}
