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
import gleam/io
import gleam/list
import gleam/option.{type Option}
import gleam/result

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
pub fn start(
  nodes: List(String),
  cluster_name: String,
) -> Result(Khepri, KhepriError)

/// Stop the Khepri store
@external(erlang, "blixard_khepri_store", "stop")
pub fn stop(store: Khepri) -> Result(Nil, KhepriError)

/// Store a MicroVM in Khepri
@external(erlang, "blixard_khepri_store", "put_vm")
pub fn put_vm(store: Khepri, vm: MicroVm) -> Result(Nil, KhepriError)

/// Retrieve a MicroVM from Khepri
@external(erlang, "blixard_khepri_store", "get_vm")
pub fn get_vm(store: Khepri, id: Uuid) -> Result(MicroVm, KhepriError)

/// List all MicroVMs
@external(erlang, "blixard_khepri_store", "list_vms")
pub fn list_vms(store: Khepri) -> Result(List(MicroVm), KhepriError)

/// Delete a MicroVM
@external(erlang, "blixard_khepri_store", "delete_vm")
pub fn delete_vm(store: Khepri, id: Uuid) -> Result(Nil, KhepriError)

/// Store a Host in Khepri
@external(erlang, "blixard_khepri_store", "put_host")
pub fn put_host(store: Khepri, host: Host) -> Result(Nil, KhepriError)

/// Retrieve a Host from Khepri
@external(erlang, "blixard_khepri_store", "get_host")
pub fn get_host(store: Khepri, id: Uuid) -> Result(Host, KhepriError)

/// List all Hosts
@external(erlang, "blixard_khepri_store", "list_hosts")
pub fn list_hosts(store: Khepri) -> Result(List(Host), KhepriError)

/// Delete a Host
@external(erlang, "blixard_khepri_store", "delete_host")
pub fn delete_host(store: Khepri, id: Uuid) -> Result(Nil, KhepriError)

/// Update the state of a VM
@external(erlang, "blixard_khepri_store", "update_vm_state")
pub fn update_vm_state(
  store: Khepri,
  id: Uuid,
  state: ResourceState,
) -> Result(Nil, KhepriError)

/// Update host assignment for a VM
@external(erlang, "blixard_khepri_store", "assign_vm_to_host")
pub fn assign_vm_to_host(
  store: Khepri,
  vm_id: Uuid,
  host_id: Uuid,
) -> Result(Nil, KhepriError)
