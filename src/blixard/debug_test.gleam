//// src/blixard/debug_test.gleam

///
/// A simple test to debug Gleam-Erlang FFI
import gleam/dict
import gleam/dynamic
import gleam/erlang/atom
import gleam/io
import gleam/option.{type Option, None, Some}
import gleam/string

import blixard/domain/types
import blixard/storage/khepri_store

// Special FFI for debugging
@external(erlang, "blixard_debug", "print_vm")
pub fn print_vm(vm: types.MicroVm) -> Nil

// Create erlang module for debugging
@external(erlang, "blixard_debug", "create_module")
pub fn create_module() -> Nil

pub fn main() {
  io.println("Debugging Gleam-Erlang FFI")
  io.println("==========================")

  // Create the debug module
  create_module()

  // Create test VM
  io.println("\nCreating test microVM...")
  let vm =
    types.MicroVm(
      id: "test-vm-1",
      name: "Test VM",
      description: Some("Test description"),
      vm_type: types.Persistent,
      resources: types.Resources(cpu_cores: 2, memory_mb: 2048, disk_gb: 20),
      state: types.Pending,
      host_id: None,
      storage_volumes: [],
      network_interfaces: [],
      tailscale_config: types.TailscaleConfig(
        enabled: True,
        auth_key: None,
        hostname: "test-vm",
        tags: ["test"],
        direct_client: True,
      ),
      nixos_config: types.NixosConfig(
        config_path: "/test/path",
        overrides: dict.new(),
        cache_url: None,
      ),
      labels: dict.new(),
      created_at: "now",
      updated_at: "now",
    )

  // Print the VM to see its structure
  io.println("Printing VM structure via FFI...")
  print_vm(vm)

  // Initialize the Khepri store
  io.println("\nInitializing Khepri store...")
  let store_result =
    khepri_store.start(["blixard@127.0.0.1"], "blixard_test_cluster")

  case store_result {
    Ok(store) -> {
      io.println("Khepri store initialized successfully!")

      // Try to store the VM
      io.println("\nStoring VM in Khepri...")
      case khepri_store.put_vm(store, vm) {
        Ok(_) -> io.println("VM stored successfully!")
        Error(err) -> io.println("Failed to store VM: " <> string.inspect(err))
      }
    }

    Error(err) -> {
      io.println("Failed to initialize Khepri store: " <> string.inspect(err))
    }
  }
}
