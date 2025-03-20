// src/blixard/storage/khepri_config.gleam

///
/// Configuration for storage backend modes
import envoy
import gleam/io
import gleam/list
import gleam/result
import gleam/string

/// Different modes for the storage backend
pub type StorageMode {
  /// Use the mock implementation for testing/development
  MockMode
  /// Use the ETS/DETS implementation for simple persistence
  EtsDetsMode
  /// Use the real Khepri implementation for production (future)
  KhepriMode
}

/// Get the current storage mode based on environment variables
pub fn get_mode() -> StorageMode {
  // Log which mode we're checking
  io.println("[CONFIG] Checking storage mode from environment")

  let mode = case envoy.get("BLIXARD_STORAGE_MODE") {
    Ok("ets_dets") -> {
      io.println("[CONFIG] Using ETS/DETS storage implementation")
      EtsDetsMode
    }
    Ok("khepri") -> {
      io.println("[CONFIG] Using Khepri implementation")
      KhepriMode
    }
    _ -> {
      // Default to mock for safety during transition
      io.println("[CONFIG] Using MOCK storage implementation (default)")
      MockMode
    }
  }

  mode
}

/// Check if a specific operation should use the ETS/DETS implementation
pub fn use_ets_dets_for_operation(operation: String) -> Bool {
  case envoy.get("BLIXARD_ETSDETS_OPS") {
    Ok(ops_list) -> {
      // Parse comma-separated list of operations
      let ops =
        ops_list
        |> string.split(",")
        |> list.map(string.trim)

      // Check if the operation is in the list
      let is_enabled = list.contains(ops, operation)

      // Log which implementation we're using for this operation
      case is_enabled {
        True ->
          io.println(
            "[CONFIG] Operation '"
            <> operation
            <> "' using ETS/DETS implementation",
          )
        False ->
          io.println(
            "[CONFIG] Operation '" <> operation <> "' using MOCK implementation",
          )
      }

      is_enabled
    }
    _ -> False
  }
}

/// Check if a specific operation should use the Khepri implementation
/// (Reserved for future implementation)
pub fn use_khepri_for_operation(operation: String) -> Bool {
  case envoy.get("BLIXARD_KHEPRI_OPS") {
    Ok(ops_list) -> {
      // Parse comma-separated list of operations
      let ops =
        ops_list
        |> string.split(",")
        |> list.map(string.trim)

      // Check if the operation is in the list
      let is_enabled = list.contains(ops, operation)

      // Log which implementation we're using for this operation
      case is_enabled {
        True ->
          io.println(
            "[CONFIG] Operation '"
            <> operation
            <> "' using KHEPRI implementation",
          )
        False ->
          io.println(
            "[CONFIG] Operation '"
            <> operation
            <> "' not using KHEPRI implementation",
          )
      }

      is_enabled
    }
    _ -> False
  }
}
