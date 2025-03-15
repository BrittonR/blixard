//// src/blixard/test_khepri.gleam

///
/// Test program for the Khepri store
import gleam/dict
import gleam/io
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string

import blixard/domain/types
import blixard/storage/khepri_store

pub fn main() {
  io.println("Starting Khepri Store Test")
  io.println("==========================")

  // Initialize the Khepri store
  io.println("\n1. Initializing Khepri store...")
  let store_result =
    khepri_store.start(["blixard@127.0.0.1"], "blixard_test_cluster")

  case store_result {
    Ok(store) -> {
      io.println("   ✓ Khepri store initialized successfully!")

      // Create test VM
      io.println("\n2. Creating test microVM...")
      let vm = create_test_vm()
      io.println("   ✓ Created test VM with ID: " <> vm.id)

      // Store VM in Khepri
      io.println("\n3. Storing VM in Khepri...")
      case khepri_store.put_vm(store, vm) {
        Ok(_) -> {
          io.println("   ✓ VM stored successfully!")

          // Retrieve VM from Khepri
          io.println("\n4. Retrieving VM from Khepri...")
          case khepri_store.get_vm(store, vm.id) {
            Ok(retrieved_vm) -> {
              io.println("   ✓ VM retrieved successfully!")
              io.println("   ✓ VM name: " <> retrieved_vm.name)
              io.println(
                "   ✓ VM state: " <> string.inspect(retrieved_vm.state),
              )

              // List all VMs
              io.println("\n5. Listing all VMs...")
              case khepri_store.list_vms(store) {
                Ok(vms) -> {
                  io.println(
                    "   ✓ Found "
                    <> string.inspect(list.length(vms))
                    <> " VM(s)",
                  )

                  // Update VM state
                  io.println("\n6. Updating VM state to Running...")
                  case
                    khepri_store.update_vm_state(store, vm.id, types.Running)
                  {
                    Ok(_) -> {
                      io.println("   ✓ VM state updated successfully!")

                      // Verify state was updated
                      case khepri_store.get_vm(store, vm.id) {
                        Ok(updated_vm) -> {
                          io.println(
                            "   ✓ Updated VM state: "
                            <> string.inspect(updated_vm.state),
                          )

                          // Create test host
                          io.println("\n7. Creating and storing test host...")
                          let host = create_test_host()

                          case khepri_store.put_host(store, host) {
                            Ok(_) -> {
                              io.println("   ✓ Host stored successfully!")

                              // Assign VM to host
                              io.println("\n8. Assigning VM to host...")
                              case
                                khepri_store.assign_vm_to_host(
                                  store,
                                  vm.id,
                                  host.id,
                                )
                              {
                                Ok(_) -> {
                                  io.println(
                                    "   ✓ VM assigned to host successfully!",
                                  )

                                  // Verify assignment
                                  case khepri_store.get_vm(store, vm.id) {
                                    Ok(assigned_vm) -> {
                                      io.println(
                                        "   ✓ VM host_id: "
                                        <> string.inspect(assigned_vm.host_id),
                                      )

                                      // Clean up
                                      io.println("\n9. Cleaning up...")
                                      let _ =
                                        khepri_store.delete_vm(store, vm.id)
                                      let _ =
                                        khepri_store.delete_host(store, host.id)
                                      let _ = khepri_store.stop(store)
                                      io.println("   ✓ Cleanup completed!")

                                      io.println(
                                        "\nAll tests completed successfully! ✨",
                                      )
                                    }
                                    Error(err) ->
                                      io.println(
                                        "   ✗ Failed to get assigned VM: "
                                        <> string.inspect(err),
                                      )
                                  }
                                }
                                Error(err) ->
                                  io.println(
                                    "   ✗ Failed to assign VM to host: "
                                    <> string.inspect(err),
                                  )
                              }
                            }
                            Error(err) ->
                              io.println(
                                "   ✗ Failed to store host: "
                                <> string.inspect(err),
                              )
                          }
                        }
                        Error(err) ->
                          io.println(
                            "   ✗ Failed to get updated VM: "
                            <> string.inspect(err),
                          )
                      }
                    }
                    Error(err) ->
                      io.println(
                        "   ✗ Failed to update VM state: "
                        <> string.inspect(err),
                      )
                  }
                }
                Error(err) ->
                  io.println("   ✗ Failed to list VMs: " <> string.inspect(err))
              }
            }
            Error(err) ->
              io.println("   ✗ Failed to retrieve VM: " <> string.inspect(err))
          }
        }
        Error(err) ->
          io.println("   ✗ Failed to store VM: " <> string.inspect(err))
      }
    }

    Error(err) -> {
      io.println(
        "   ✗ Failed to initialize Khepri store: " <> string.inspect(err),
      )
    }
  }
}

// Create a test VM for the demo
fn create_test_vm() -> types.MicroVm {
  types.MicroVm(
    id: "test-vm-1",
    name: "Test VM",
    description: Some("A test VM for Khepri store testing"),
    vm_type: types.Persistent,
    resources: types.Resources(cpu_cores: 2, memory_mb: 2048, disk_gb: 20),
    state: types.Pending,
    host_id: None,
    storage_volumes: [
      types.StorageVolume(
        id: "test-vol-1",
        name: "Root Volume",
        size_gb: 20,
        path: "/dev/vda",
        persistent: True,
      ),
    ],
    network_interfaces: [
      types.NetworkInterface(
        id: "test-nic-1",
        name: "eth0",
        ipv4_address: None,
        ipv6_address: None,
        mac_address: None,
      ),
    ],
    tailscale_config: types.TailscaleConfig(
      enabled: True,
      auth_key: None,
      hostname: "test-vm-1",
      tags: ["test", "development"],
      direct_client: True,
    ),
    nixos_config: types.NixosConfig(
      config_path: "/etc/nixos/vm-configs/test-vm-1.nix",
      overrides: dict.new(),
      cache_url: None,
    ),
    labels: dict.from_list([
      #("environment", "testing"),
      #("project", "blixard"),
    ]),
    created_at: "2025-03-14T12:00:00Z",
    updated_at: "2025-03-14T12:00:00Z",
  )
}

// Create a test host for the demo
fn create_test_host() -> types.Host {
  types.Host(
    id: "test-host-1",
    name: "Test Host",
    description: Some("A test host for Khepri store testing"),
    control_ip: "192.168.1.100",
    connected: True,
    available_resources: types.Resources(
      cpu_cores: 8,
      memory_mb: 16_384,
      disk_gb: 500,
    ),
    total_resources: types.Resources(
      cpu_cores: 8,
      memory_mb: 16_384,
      disk_gb: 500,
    ),
    vm_ids: [],
    schedulable: True,
    tags: ["test", "development"],
    labels: dict.from_list([#("datacenter", "local"), #("rack", "virtual")]),
    created_at: "2025-03-14T12:00:00Z",
    updated_at: "2025-03-14T12:00:00Z",
  )
}
