//// src/blixard/host_agent/state.gleam

///
/// Khepri access functions and watch management
import blixard/domain/types as domain_types
import blixard/host_agent/types

import blixard/storage/khepri_store.{type Khepri, type KhepriError}
import gleam/dict.{type Dict}
import gleam/dynamic
import gleam/erlang/process.{type Subject}
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result

/// Get all VMs for the current host
pub fn get_host_vms(
  store: Khepri,
  host_id: domain_types.Uuid,
) -> Result(List(domain_types.MicroVm), String) {
  // Get all VMs
  case khepri_store.list_vms(store) {
    Ok(all_vms) -> {
      // Filter VMs assigned to this host
      let host_vms =
        list.filter(all_vms, fn(vm) {
          case vm.host_id {
            Some(id) -> id == host_id
            None -> False
          }
        })
      Ok(host_vms)
    }
    Error(err) -> Error("Failed to list VMs: " <> debug_error(err))
  }
}

/// Get VM by ID
pub fn get_vm(
  store: Khepri,
  vm_id: domain_types.Uuid,
) -> Result(domain_types.MicroVm, String) {
  case khepri_store.get_vm(store, vm_id) {
    Ok(vm) -> Ok(vm)
    Error(err) -> Error("Failed to get VM: " <> debug_error(err))
  }
}

/// Get host data
pub fn get_host(
  store: Khepri,
  host_id: domain_types.Uuid,
) -> Result(domain_types.Host, String) {
  case khepri_store.get_host(store, host_id) {
    Ok(host) -> Ok(host)
    Error(err) -> Error("Failed to get host: " <> debug_error(err))
  }
}

/// Find VM by name
pub fn find_vm_by_name(
  store: Khepri,
  name: String,
) -> Result(domain_types.MicroVm, String) {
  case khepri_store.list_vms(store) {
    Ok(vms) -> {
      case list.find(vms, fn(vm) { vm.name == name }) {
        Ok(vm) -> Ok(vm)
        Error(Nil) -> Error("VM with name '" <> name <> "' not found")
      }
    }
    Error(err) -> Error("Failed to list VMs: " <> debug_error(err))
  }
}

/// Update a VM state in Khepri
pub fn update_vm_state(
  store: Khepri,
  vm_id: domain_types.Uuid,
  new_state: domain_types.ResourceState,
) -> Result(Nil, String) {
  case khepri_store.update_vm_state(store, vm_id, new_state) {
    Ok(_) -> Ok(Nil)
    Error(err) -> Error("Failed to update VM state: " <> debug_error(err))
  }
}

/// Update host resources in Khepri
pub fn update_host_resources(
  store: Khepri,
  host_id: domain_types.Uuid,
  resources: domain_types.Resources,
) -> Result(domain_types.Host, String) {
  case khepri_store.get_host(store, host_id) {
    Ok(host) -> {
      let updated_host =
        domain_types.Host(..host, available_resources: resources)
      case khepri_store.put_host(store, updated_host) {
        Ok(_) -> Ok(updated_host)
        Error(err) ->
          Error("Failed to update host resources: " <> debug_error(err))
      }
    }
    Error(err) -> Error("Failed to get host: " <> debug_error(err))
  }
}

/// Update host schedulable setting in Khepri
pub fn update_host_schedulable(
  store: Khepri,
  host_id: domain_types.Uuid,
  schedulable: Bool,
) -> Result(domain_types.Host, String) {
  case khepri_store.get_host(store, host_id) {
    Ok(host) -> {
      let updated_host = domain_types.Host(..host, schedulable: schedulable)
      case khepri_store.put_host(store, updated_host) {
        Ok(_) -> Ok(updated_host)
        Error(err) ->
          Error("Failed to update host schedulable: " <> debug_error(err))
      }
    }
    Error(err) -> Error("Failed to get host: " <> debug_error(err))
  }
}

// Set up watch for VM changes if Khepri supports it
// Note: This is a placeholder - implementation depends on Khepri capabilities
pub fn watch_vms(
  store: Khepri,
  callback: Subject(types.Command),
) -> Result(Nil, String) {
  // Placeholder for Khepri watch functionality
  // Actual implementation would register watches on relevant Khepri paths
  Ok(Nil)
}

// Helper function to debug KhepriError
pub fn debug_error(error: KhepriError) -> String {
  case error {
    khepri_store.ConnectionError(msg) -> "Connection error: " <> msg
    khepri_store.ConsensusError(msg) -> "Consensus error: " <> msg
    khepri_store.StorageError(msg) -> "Storage error: " <> msg
    khepri_store.NotFound -> "Resource not found"
    khepri_store.InvalidData(msg) -> "Invalid data: " <> msg
  }
}
