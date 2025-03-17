//// src/blixard/host_agent/types.gleam

///
/// Type definitions for the host agent
import blixard/domain/types
import blixard/storage/khepri_store.{type Khepri}
import gleam/dict.{type Dict}
import gleam/dynamic.{type Dynamic}
import gleam/erlang/process.{type Subject}
import gleam/option.{type Option}

// Commands that can be sent to the host agent
pub type Command {
  // VM lifecycle commands
  StartVM(vm_id: types.Uuid, reply_with: Subject(Result(Nil, String)))
  StopVM(vm_id: types.Uuid, reply_with: Subject(Result(Nil, String)))
  CreateVM(vm: types.MicroVm, reply_with: Subject(Result(Nil, String)))
  UpdateVM(vm_id: types.Uuid, reply_with: Subject(Result(Nil, String)))
  RestartVM(vm_id: types.Uuid, reply_with: Subject(Result(Nil, String)))
  ListVMs(reply_with: Subject(Result(List(MicroVMStatus), String)))

  // Host management commands
  UpdateResources(
    resources: types.Resources,
    reply_with: Subject(Result(Nil, String)),
  )
  SetSchedulable(schedulable: Bool, reply_with: Subject(Result(Nil, String)))

  // Monitoring
  GetStatus(reply_with: Subject(Result(HostStatus, String)))
  ExportPrometheusMetrics(reply_with: Subject(Result(String, String)))

  // Internal messages for Khepri updates (if watch functionality exists)
  KhepriUpdate(
    path: List(String),
    value: Option(Dynamic),
    reply_with: Subject(Result(Nil, String)),
  )
}

// Host agent state - minimized to avoid duplicating Khepri data
pub type State {
  State(
    host_id: types.Uuid,
    store: Khepri,
    metrics: Metrics,
    // No VM list - fetched from Khepri when needed
  )
}

pub type MicroVMStatus {
  MicroVMStatus(
    name: String,
    vm_id: Option(types.Uuid),
    is_running: Bool,
    is_outdated: Bool,
    system_version: Option(String),
  )
}

// Host status information
pub type HostStatus {
  HostStatus(
    host_id: types.Uuid,
    vm_count: Int,
    running_vms: List(types.Uuid),
    available_resources: types.Resources,
    schedulable: Bool,
    load_average: List(Float),
    memory_usage_percent: Float,
    disk_usage_percent: Float,
  )
}

// Metrics tracking for the host
pub type Metrics {
  Metrics(
    vm_start_count: Int,
    vm_stop_count: Int,
    vm_create_count: Int,
    vm_update_count: Int,
    vm_restart_count: Int,
    vm_error_count: Int,
    last_error: Option(String),
  )
}
