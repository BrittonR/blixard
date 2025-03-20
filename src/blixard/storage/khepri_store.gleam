//// src/blixard/storage/khepri_store.gleam

///
/// Integration with Khepri, a tree-structured, replicated database with RAFT consensus
import blixard/domain/types.{
  type Host, type MicroVm, type ResourceState, type Uuid,
}
import gleam/dict
import gleam/dynamic
import gleam/erlang
import gleam/erlang/atom
import gleam/int
import gleam/io
import gleam/list
import gleam/option.{type Option}
import gleam/result
import gleam/string

/// Khepri paths for different resource types
pub const vms_path = ["blixard", "vms"]

pub const hosts_path = ["blixard", "hosts"]

pub const networks_path = ["blixard", "networks"]

/// Type to represent Khepri process
pub opaque type Khepri {
  Khepri(name: String)
}

/// Error types for Khepri operations
pub type KhepriError {
  ConnectionError(String)
  ConsensusError(String)
  StorageError(String)
  NotFound
  InvalidData(String)
}

/// Connect to Khepri and start the store
@external(erlang, "blixard_khepri_store", "start")
fn erlang_start(
  nodes: List(String),
  cluster_name: String,
) -> Result(Khepri, KhepriError)

/// Connect to Khepri and start the store with debug wrapper
pub fn start(
  nodes: List(String),
  cluster_name: String,
) -> Result(Khepri, KhepriError) {
  io.println(
    "[KHEPRI] Starting Khepri store with cluster name: " <> cluster_name,
  )

  let result = erlang_start(nodes, cluster_name)

  case result {
    Ok(store) -> {
      io.println("[KHEPRI] Khepri store started successfully")
      Ok(store)
    }
    Error(err) -> {
      io.println(
        "[KHEPRI ERROR] Failed to start Khepri store: " <> debug_error(err),
      )
      Error(err)
    }
  }
}

/// Stop the Khepri store
@external(erlang, "blixard_khepri_store", "stop")
fn erlang_stop(store: Khepri) -> Result(Nil, KhepriError)

/// Stop the Khepri store with debug wrapper
pub fn stop(store: Khepri) -> Result(Nil, KhepriError) {
  io.println("[KHEPRI] Stopping Khepri store")

  let result = erlang_stop(store)

  case result {
    Ok(_) -> {
      io.println("[KHEPRI] Khepri store stopped successfully")
      Ok(Nil)
    }
    Error(err) -> {
      io.println(
        "[KHEPRI ERROR] Failed to stop Khepri store: " <> debug_error(err),
      )
      Error(err)
    }
  }
}

/// Store a MicroVM in Khepri
@external(erlang, "blixard_khepri_store", "put_vm")
fn erlang_put_vm(store: Khepri, vm: MicroVm) -> Result(Nil, KhepriError)

/// Store a MicroVM in Khepri with debug wrapper
pub fn put_vm(store: Khepri, vm: MicroVm) -> Result(Nil, KhepriError) {
  io.println("[KHEPRI] Storing VM in Khepri:")
  io.println("  - ID: " <> vm.id)
  io.println("  - Name: " <> vm.name)
  io.println("  - State: " <> string.inspect(vm.state))
  io.println("  - Host ID: " <> string.inspect(vm.host_id))

  let result = erlang_put_vm(store, vm)

  case result {
    Ok(_) -> {
      io.println("[KHEPRI] VM stored successfully")
      Ok(Nil)
    }
    Error(err) -> {
      io.println("[KHEPRI ERROR] Failed to store VM: " <> debug_error(err))
      Error(err)
    }
  }
}

/// Retrieve a MicroVM from Khepri
@external(erlang, "blixard_khepri_store", "get_vm")
fn erlang_get_vm(store: Khepri, id: Uuid) -> Result(MicroVm, KhepriError)

/// Retrieve a MicroVM from Khepri with debug wrapper
pub fn get_vm(store: Khepri, id: Uuid) -> Result(MicroVm, KhepriError) {
  io.println("[KHEPRI] Getting VM from Khepri with ID: " <> id)

  let result = erlang_get_vm(store, id)

  case result {
    Ok(vm) -> {
      io.println("[KHEPRI] VM retrieved successfully:")
      io.println("  - Name: " <> vm.name)
      io.println("  - State: " <> string.inspect(vm.state))
      Ok(vm)
    }
    Error(err) -> {
      io.println("[KHEPRI ERROR] Failed to get VM: " <> debug_error(err))
      Error(err)
    }
  }
}

/// List all MicroVMs
@external(erlang, "blixard_khepri_store", "list_vms")
fn erlang_list_vms(store: Khepri) -> Result(List(MicroVm), KhepriError)

/// List all MicroVMs with debug wrapper
pub fn list_vms(store: Khepri) -> Result(List(MicroVm), KhepriError) {
  io.println("[KHEPRI] Listing all VMs from Khepri")

  let result = erlang_list_vms(store)

  case result {
    Ok(vms) -> {
      io.println("[KHEPRI] Found " <> int.to_string(list.length(vms)) <> " VMs")
      Ok(vms)
    }
    Error(err) -> {
      io.println("[KHEPRI ERROR] Failed to list VMs: " <> debug_error(err))
      Error(err)
    }
  }
}

/// Delete a MicroVM
@external(erlang, "blixard_khepri_store", "delete_vm")
fn erlang_delete_vm(store: Khepri, id: Uuid) -> Result(Nil, KhepriError)

/// Delete a MicroVM with debug wrapper
pub fn delete_vm(store: Khepri, id: Uuid) -> Result(Nil, KhepriError) {
  io.println("[KHEPRI] Deleting VM from Khepri with ID: " <> id)

  let result = erlang_delete_vm(store, id)

  case result {
    Ok(_) -> {
      io.println("[KHEPRI] VM deleted successfully")
      Ok(Nil)
    }
    Error(err) -> {
      io.println("[KHEPRI ERROR] Failed to delete VM: " <> debug_error(err))
      Error(err)
    }
  }
}

/// Store a Host in Khepri
@external(erlang, "blixard_khepri_store", "put_host")
fn erlang_put_host(store: Khepri, host: Host) -> Result(Nil, KhepriError)

/// Store a Host in Khepri with debug wrapper
pub fn put_host(store: Khepri, host: Host) -> Result(Nil, KhepriError) {
  io.println("[KHEPRI] Storing Host in Khepri:")
  io.println("  - ID: " <> host.id)
  io.println("  - Name: " <> host.name)
  io.println("  - Control IP: " <> host.control_ip)

  let result = erlang_put_host(store, host)

  case result {
    Ok(_) -> {
      io.println("[KHEPRI] Host stored successfully")
      Ok(Nil)
    }
    Error(err) -> {
      io.println("[KHEPRI ERROR] Failed to store Host: " <> debug_error(err))
      Error(err)
    }
  }
}

/// Retrieve a Host from Khepri
@external(erlang, "blixard_khepri_store", "get_host")
fn erlang_get_host(store: Khepri, id: Uuid) -> Result(Host, KhepriError)

/// Retrieve a Host from Khepri with debug wrapper
pub fn get_host(store: Khepri, id: Uuid) -> Result(Host, KhepriError) {
  io.println("[KHEPRI] Getting Host from Khepri with ID: " <> id)

  let result = erlang_get_host(store, id)

  case result {
    Ok(host) -> {
      io.println("[KHEPRI] Host retrieved successfully:")
      io.println("  - Name: " <> host.name)
      io.println("  - Control IP: " <> host.control_ip)
      Ok(host)
    }
    Error(err) -> {
      io.println("[KHEPRI ERROR] Failed to get Host: " <> debug_error(err))
      Error(err)
    }
  }
}

/// List all Hosts
@external(erlang, "blixard_khepri_store", "list_hosts")
fn erlang_list_hosts(store: Khepri) -> Result(List(Host), KhepriError)

/// List all Hosts with debug wrapper
pub fn list_hosts(store: Khepri) -> Result(List(Host), KhepriError) {
  io.println("[KHEPRI] Listing all Hosts from Khepri")

  let result = erlang_list_hosts(store)

  case result {
    Ok(hosts) -> {
      io.println(
        "[KHEPRI] Found " <> int.to_string(list.length(hosts)) <> " Hosts",
      )
      Ok(hosts)
    }
    Error(err) -> {
      io.println("[KHEPRI ERROR] Failed to list Hosts: " <> debug_error(err))
      Error(err)
    }
  }
}

/// Delete a Host
@external(erlang, "blixard_khepri_store", "delete_host")
fn erlang_delete_host(store: Khepri, id: Uuid) -> Result(Nil, KhepriError)

/// Delete a Host with debug wrapper
pub fn delete_host(store: Khepri, id: Uuid) -> Result(Nil, KhepriError) {
  io.println("[KHEPRI] Deleting Host from Khepri with ID: " <> id)

  let result = erlang_delete_host(store, id)

  case result {
    Ok(_) -> {
      io.println("[KHEPRI] Host deleted successfully")
      Ok(Nil)
    }
    Error(err) -> {
      io.println("[KHEPRI ERROR] Failed to delete Host: " <> debug_error(err))
      Error(err)
    }
  }
}

/// Update the state of a VM
@external(erlang, "blixard_khepri_store", "update_vm_state")
fn erlang_update_vm_state(
  store: Khepri,
  id: Uuid,
  state: ResourceState,
) -> Result(Nil, KhepriError)

/// Update the state of a VM with debug wrapper
pub fn update_vm_state(
  store: Khepri,
  id: Uuid,
  state: ResourceState,
) -> Result(Nil, KhepriError) {
  io.println("[KHEPRI] Updating VM state in Khepri:")
  io.println("  - VM ID: " <> id)
  io.println("  - New State: " <> string.inspect(state))

  let result = erlang_update_vm_state(store, id, state)

  case result {
    Ok(_) -> {
      io.println("[KHEPRI] VM state updated successfully")
      Ok(Nil)
    }
    Error(err) -> {
      io.println(
        "[KHEPRI ERROR] Failed to update VM state: " <> debug_error(err),
      )
      Error(err)
    }
  }
}

/// Update host assignment for a VM
@external(erlang, "blixard_khepri_store", "assign_vm_to_host")
fn erlang_assign_vm_to_host(
  store: Khepri,
  vm_id: Uuid,
  host_id: Uuid,
) -> Result(Nil, KhepriError)

/// Update host assignment for a VM with debug wrapper
pub fn assign_vm_to_host(
  store: Khepri,
  vm_id: Uuid,
  host_id: Uuid,
) -> Result(Nil, KhepriError) {
  io.println("[KHEPRI] Assigning VM to Host in Khepri:")
  io.println("  - VM ID: " <> vm_id)
  io.println("  - Host ID: " <> host_id)

  let result = erlang_assign_vm_to_host(store, vm_id, host_id)

  case result {
    Ok(_) -> {
      io.println("[KHEPRI] VM assigned to Host successfully")
      Ok(Nil)
    }
    Error(err) -> {
      io.println(
        "[KHEPRI ERROR] Failed to assign VM to Host: " <> debug_error(err),
      )
      Error(err)
    }
  }
}

/// Atomic VM scheduling - performs state update and host assignment in one transaction
@external(erlang, "blixard_khepri_tx", "atomic_vm_scheduling")
pub fn atomic_vm_scheduling(
  store: Khepri,
  vm_id: Uuid,
  host_id: Uuid,
  state: ResourceState,
) -> Result(Nil, KhepriError)

/// Helper function to debug KhepriError
pub fn debug_error(error: KhepriError) -> String {
  case error {
    ConnectionError(msg) -> "Connection error: " <> msg
    ConsensusError(msg) -> "Consensus error: " <> msg
    StorageError(msg) -> "Storage error: " <> msg
    NotFound -> "Resource not found"
    InvalidData(msg) -> "Invalid data: " <> msg
  }
}
