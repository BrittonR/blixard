// src/service_manager.gleam
import cli
import gleam/erlang
import gleam/io
import khepri_store
import node_manager
import service_handlers

pub fn main() -> Nil {
  // Get command-line arguments
  let args = erlang.start_arguments()

  // Process special setup commands first
  case args {
    ["--init-primary"] -> {
      node_manager.start_primary_node()
      Nil
    }
    ["--init-secondary", primary_node] -> {
      node_manager.start_secondary_node(primary_node)
      Nil
    }
    _ -> {
      // Regular command processing - ensure Erlang distribution is started
      node_manager.ensure_distribution()

      // Initialize Khepri (assumes it's already running in cluster mode)
      let _ = khepri_store.init()

      // Process commands
      cli.handle_command(args)
    }
  }
}
