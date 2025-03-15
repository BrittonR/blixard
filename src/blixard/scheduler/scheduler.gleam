//// src/blixard/scheduler/scheduler.gleam

///
/// Scheduler for placing MicroVMs on suitable hosts
import blixard/domain/types.{
  type Host, type MicroVm, type ResourceState, type Resources, type Uuid,
}
import blixard/storage/khepri_store.{type Khepri, type KhepriError}
import gleam/dict
import gleam/int
import gleam/io
import gleam/list
import gleam/option.{type Option}
import gleam/result

/// Strategy for scheduling VMs
pub type SchedulingStrategy {
  /// Place VM on the host with the most available resources
  MostAvailable
  /// Place VM on the host with the least available resources that can still fit the VM
  BinPacking
  /// Place VM on a specific host
  SpecificHost(host_id: Uuid)
  /// Place VM on a host with specific tags
  TagAffinity(tags: List(String))
}

/// Result of a scheduling decision
pub type SchedulingResult {
  Scheduled(vm_id: Uuid, host_id: Uuid)
  NoSuitableHost(reason: String)
  SchedulingError(error: String)
}

/// Check if a host has enough resources for a VM
fn has_enough_resources(host: Host, vm: MicroVm) -> Bool {
  let host_resources = host.available_resources
  let vm_resources = vm.resources

  host_resources.cpu_cores >= vm_resources.cpu_cores
  && host_resources.memory_mb >= vm_resources.memory_mb
  && host_resources.disk_gb >= vm_resources.disk_gb
}

/// Calculate remaining resources after placing a VM
fn calculate_remaining_resources(host: Host, vm: MicroVm) -> Resources {
  let host_resources = host.available_resources
  let vm_resources = vm.resources

  types.Resources(
    cpu_cores: host_resources.cpu_cores - vm_resources.cpu_cores,
    memory_mb: host_resources.memory_mb - vm_resources.memory_mb,
    disk_gb: host_resources.disk_gb - vm_resources.disk_gb,
  )
}

/// Find a suitable host using the MostAvailable strategy
fn find_most_available_host(
  hosts: List(Host),
  vm: MicroVm,
) -> Result(Host, String) {
  hosts
  |> list.filter(fn(host) { host.schedulable && has_enough_resources(host, vm) })
  |> list.sort(fn(a, b) {
    // Sort by total available resources (CPU + memory + disk)
    let a_total =
      a.available_resources.cpu_cores
      * 100
      + a.available_resources.memory_mb
      / 1024
      + a.available_resources.disk_gb

    let b_total =
      b.available_resources.cpu_cores
      * 100
      + b.available_resources.memory_mb
      / 1024
      + b.available_resources.disk_gb

    int.compare(b_total, a_total)
    // Descending order
  })
  |> list.first
  |> result.replace_error("No suitable host found with enough resources")
}

/// Find a suitable host using the BinPacking strategy
fn find_bin_packing_host(hosts: List(Host), vm: MicroVm) -> Result(Host, String) {
  hosts
  |> list.filter(fn(host) { host.schedulable && has_enough_resources(host, vm) })
  |> list.sort(fn(a, b) {
    // Sort by remaining resources after placing the VM (ascending)
    let a_remaining = calculate_remaining_resources(a, vm)
    let b_remaining = calculate_remaining_resources(b, vm)

    let a_total =
      a_remaining.cpu_cores
      * 100
      + a_remaining.memory_mb
      / 1024
      + a_remaining.disk_gb

    let b_total =
      b_remaining.cpu_cores
      * 100
      + b_remaining.memory_mb
      / 1024
      + b_remaining.disk_gb

    int.compare(a_total, b_total)
    // Ascending order
  })
  |> list.first
  |> result.replace_error("No suitable host found with enough resources")
}

/// Find a host with specific tags
fn find_host_with_tags(
  hosts: List(Host),
  vm: MicroVm,
  required_tags: List(String),
) -> Result(Host, String) {
  // Filter hosts that have all the required tags
  let hosts_with_tags =
    list.filter(hosts, fn(host) {
      list.all(required_tags, fn(tag) { list.contains(host.tags, tag) })
    })

  // Use MostAvailable strategy among the filtered hosts
  case hosts_with_tags {
    [] -> Error("No host found with the required tags")
    _ -> find_most_available_host(hosts_with_tags, vm)
  }
}

/// Find a specific host by ID
fn find_specific_host(
  hosts: List(Host),
  vm: MicroVm,
  host_id: Uuid,
) -> Result(Host, String) {
  hosts
  |> list.find(fn(host) { host.id == host_id })
  |> result.replace_error("Host not found with ID: " <> host_id)
  |> result.then(fn(host) {
    case host.schedulable && has_enough_resources(host, vm) {
      True -> Ok(host)
      False -> Error("Specified host cannot accommodate the VM")
    }
  })
}

/// Schedule a VM on a suitable host
pub fn schedule_vm(
  store: Khepri,
  vm_id: Uuid,
  strategy: SchedulingStrategy,
) -> SchedulingResult {
  // Retrieve the VM
  let vm_result = khepri_store.get_vm(store, vm_id)

  // Retrieve all hosts
  let hosts_result = khepri_store.list_hosts(store)

  case vm_result, hosts_result {
    Ok(vm), Ok(hosts) -> {
      // Select host based on strategy
      let host_result = case strategy {
        MostAvailable -> find_most_available_host(hosts, vm)
        BinPacking -> find_bin_packing_host(hosts, vm)
        SpecificHost(host_id) -> find_specific_host(hosts, vm, host_id)
        TagAffinity(tags) -> find_host_with_tags(hosts, vm, tags)
      }

      case host_result {
        Ok(host) -> {
          // Assign VM to host
          case khepri_store.assign_vm_to_host(store, vm_id, host.id) {
            Ok(_) -> {
              // Update VM state to Scheduling
              case
                khepri_store.update_vm_state(store, vm_id, types.Scheduling)
              {
                Ok(_) -> Scheduled(vm_id: vm_id, host_id: host.id)
                Error(err) ->
                  SchedulingError(
                    error: "Failed to update VM state: " <> debug_error(err),
                  )
              }
            }
            Error(err) ->
              SchedulingError(
                error: "Failed to assign VM to host: " <> debug_error(err),
              )
          }
        }
        Error(reason) -> NoSuitableHost(reason: reason)
      }
    }
    Error(err), _ ->
      SchedulingError(error: "Failed to retrieve VM: " <> debug_error(err))
    _, Error(err) ->
      SchedulingError(error: "Failed to retrieve hosts: " <> debug_error(err))
  }
}

/// Helper function to debug KhepriError
fn debug_error(error: KhepriError) -> String {
  case error {
    khepri_store.ConnectionError(msg) -> "Connection error: " <> msg
    khepri_store.ConsensusError(msg) -> "Consensus error: " <> msg
    khepri_store.StorageError(msg) -> "Storage error: " <> msg
    khepri_store.NotFound -> "Resource not found"
    khepri_store.InvalidData(msg) -> "Invalid data: " <> msg
  }
}
