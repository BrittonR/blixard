//// src/blixard.gleam

///
/// Main entry point for the Blixard orchestrator
import gleam/dict
import gleam/erlang
import gleam/erlang/process
import gleam/int
import gleam/io
import gleam/list
import gleam/option.{type Option}
import gleam/result
import gleam/string

import blixard/debug_test
import blixard/domain/types
import blixard/host_agent/agent
import blixard/orchestrator/core
import blixard/storage/khepri_store
import blixard/test_khepri

// Main function
pub fn main() {
  // Parse command-line arguments
  let args = erlang.start_arguments()

  // Check which mode to run
  case args {
    // Debug test mode
    ["--debug-ffi"] -> {
      debug_test.main()
    }

    // Khepri test mode
    ["--test-khepri"] -> {
      test_khepri.main()
    }

    // Normal mode
    _ -> {
      run_normal()
    }
  }
}

fn run_normal() {
  io.println("Starting Blixard - NixOS microVM orchestrator")

  // Start Khepri store
  io.println("Initializing Khepri store...")
  let store_result =
    khepri_store.start(["blixard@127.0.0.1"], "blixard_cluster")

  case store_result {
    Ok(store) -> {
      io.println("Khepri store initialized successfully!")

      // For now, just log that we're running
      io.println("Blixard initialized! This is a placeholder implementation.")
      io.println(
        "In a real implementation, we would start the orchestrator and API server.",
      )
      io.println("\nOptions:")
      io.println("  --test-khepri : Run Khepri store tests")
      io.println("  --debug-ffi   : Run FFI debugging tests")

      // Keep the application running indefinitely
      process.sleep_forever()
    }

    Error(err) -> {
      io.println("Failed to initialize Khepri store: " <> safe_debug_error(err))
      // In the error case, we just let the process exit normally
    }
  }
}

/// Helper function to debug KhepriError with safer handling
fn safe_debug_error(error: khepri_store.KhepriError) -> String {
  case error {
    khepri_store.ConnectionError(msg) ->
      "Connection error: " <> string.inspect(msg)
    khepri_store.ConsensusError(msg) ->
      "Consensus error: " <> string.inspect(msg)
    khepri_store.StorageError(msg) -> "Storage error: " <> string.inspect(msg)
    khepri_store.NotFound -> "Resource not found"
    khepri_store.InvalidData(msg) -> "Invalid data: " <> string.inspect(msg)
    _ -> "Unknown error: " <> string.inspect(error)
  }
}
