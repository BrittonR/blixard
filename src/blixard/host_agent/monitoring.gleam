//// src/blixard/host_agent/monitoring.gleam

///
/// System monitoring and metrics functions
import blixard/domain/types as domain_types
import blixard/host_agent/state
import blixard/host_agent/types
import blixard/storage/khepri_store.{type Khepri}
import gleam/float
import gleam/int
import gleam/list
import gleam/result
import gleam/string
import shellout

// Helper function to get element at index
fn get_at(list: List(a), index: Int) -> Result(a, Nil) {
  case index {
    0 -> list.first(list)
    _ -> list |> list.drop(index) |> list.first
  }
}

// Get current system load average
pub fn get_load_average() -> List(Float) {
  let load_result =
    shellout.command(run: "cat", with: ["/proc/loadavg"], in: ".", opt: [])

  case load_result {
    Ok(output) -> {
      // Parse load average: "0.08 0.03 0.01 2/811 15140"
      let parts = output |> string.trim |> string.split(" ")

      case parts {
        [load1, load5, load15, ..] -> [
          result.unwrap(float.parse(load1), 0.0),
          result.unwrap(float.parse(load5), 0.0),
          result.unwrap(float.parse(load15), 0.0),
        ]
        _ -> [0.0, 0.0, 0.0]
      }
    }
    Error(_) -> [0.0, 0.0, 0.0]
  }
}

// Get current memory usage as a percentage
pub fn get_memory_usage() -> Float {
  let memory_result =
    shellout.command(run: "free", with: ["-m"], in: ".", opt: [])

  case memory_result {
    Ok(output) -> {
      // Parse memory output
      let lines = string.split(output, "\n")

      // Get second line (index 1) if it exists
      case lines |> list.drop(1) |> list.first {
        Ok(mem_line) -> {
          let parts =
            string.split(mem_line, " ")
            |> list.filter(fn(s) { !string.is_empty(s) })

          case get_at(parts, 1), get_at(parts, 2) {
            Ok(total_str), Ok(used_str) -> {
              let total = result.unwrap(int.parse(total_str), 1)
              let used = result.unwrap(int.parse(used_str), 0)

              case total {
                0 -> 0.0
                _ -> int.to_float(used) /. int.to_float(total) *. 100.0
              }
            }
            _, _ -> 0.0
          }
        }
        Error(_) -> 0.0
      }
    }
    Error(_) -> 0.0
  }
}

// Get current disk usage as a percentage
pub fn get_disk_usage() -> Float {
  let disk_result =
    shellout.command(run: "df", with: ["-h", "/"], in: ".", opt: [])

  case disk_result {
    Ok(output) -> {
      // Parse disk output
      let lines = string.split(output, "\n")

      // Get second line (index 1) if it exists
      case lines |> list.drop(1) |> list.first {
        Ok(disk_line) -> {
          let parts =
            string.split(disk_line, " ")
            |> list.filter(fn(s) { !string.is_empty(s) })

          case list.length(parts) >= 5 {
            True -> {
              // Get the fifth element (index 4)
              let usage_str = result.unwrap(get_at(parts, 4), "%")
              result.unwrap(
                float.parse(string.replace(usage_str, "%", "")),
                0.0,
              )
            }
            False -> 0.0
          }
        }
        Error(_) -> 0.0
      }
    }
    Error(_) -> 0.0
  }
}

// Generate a HostStatus object with current metrics
pub fn get_host_status(
  host_id: domain_types.Uuid,
  store: Khepri,
) -> Result(types.HostStatus, String) {
  // Get host data from Khepri
  let host_result = state.get_host(store, host_id)

  // Get all VMs for this host from Khepri
  let vms_result = state.get_host_vms(store, host_id)

  case host_result, vms_result {
    Ok(host), Ok(vms) -> {
      // Count running VMs
      let running_vms =
        list.filter_map(vms, fn(vm) {
          case vm.state == domain_types.Running {
            True -> Ok(vm.id)
            False -> Error(Nil)
          }
        })

      // Create status - using constructor function pattern
      Ok(types.HostStatus(
        host_id,
        list.length(vms),
        running_vms,
        host.available_resources,
        host.schedulable,
        get_load_average(),
        get_memory_usage(),
        get_disk_usage(),
      ))
    }
    Error(err), _ -> Error(err)
    _, Error(err) -> Error(err)
  }
}

// Generate Prometheus metrics
pub fn generate_prometheus_metrics(
  host_id: domain_types.Uuid,
  store: Khepri,
  metrics: types.Metrics,
) -> Result(String, String) {
  // Get host data from Khepri
  let host_result = state.get_host(store, host_id)

  // Get all VMs for this host
  let vms_result = state.get_host_vms(store, host_id)

  case host_result, vms_result {
    Ok(host), Ok(vms) -> {
      // Count running VMs
      let running_vm_count =
        list.filter(vms, fn(vm) { vm.state == domain_types.Running })
        |> list.length

      // System metrics
      let load = get_load_average()

      // Extract load values safely
      let load_1m = result.unwrap(get_at(load, 0), 0.0)
      let load_5m = result.unwrap(get_at(load, 1), 0.0)
      let load_15m = result.unwrap(get_at(load, 2), 0.0)

      let mem_usage = get_memory_usage()
      let disk_usage = get_disk_usage()

      // Generate prometheus format
      let metrics_list = [
        // VM operation counters
        "# HELP blixard_vm_start_count Number of VMs started",
        "# TYPE blixard_vm_start_count counter",
        "blixard_vm_start_count{host=\""
          <> host_id
          <> "\"} "
          <> int.to_string(metrics.vm_start_count),
        "# HELP blixard_vm_stop_count Number of VMs stopped",
        "# TYPE blixard_vm_stop_count counter",
        "blixard_vm_stop_count{host=\""
          <> host_id
          <> "\"} "
          <> int.to_string(metrics.vm_stop_count),
        "# HELP blixard_vm_create_count Number of VMs created",
        "# TYPE blixard_vm_create_count counter",
        "blixard_vm_create_count{host=\""
          <> host_id
          <> "\"} "
          <> int.to_string(metrics.vm_create_count),
        "# HELP blixard_vm_update_count Number of VMs updated",
        "# TYPE blixard_vm_update_count counter",
        "blixard_vm_update_count{host=\""
          <> host_id
          <> "\"} "
          <> int.to_string(metrics.vm_update_count),
        "# HELP blixard_vm_restart_count Number of VMs restarted",
        "# TYPE blixard_vm_restart_count counter",
        "blixard_vm_restart_count{host=\""
          <> host_id
          <> "\"} "
          <> int.to_string(metrics.vm_restart_count),
        "# HELP blixard_vm_error_count Number of VM operation errors",
        "# TYPE blixard_vm_error_count counter",
        "blixard_vm_error_count{host=\""
          <> host_id
          <> "\"} "
          <> int.to_string(metrics.vm_error_count),
        // Host metrics
        "# HELP blixard_vm_count Number of VMs managed by this host",
        "# TYPE blixard_vm_count gauge",
        "blixard_vm_count{host=\""
          <> host_id
          <> "\"} "
          <> int.to_string(list.length(vms)),
        "# HELP blixard_running_vm_count Number of running VMs on this host",
        "# TYPE blixard_running_vm_count gauge",
        "blixard_running_vm_count{host=\""
          <> host_id
          <> "\"} "
          <> int.to_string(running_vm_count),
        "# HELP blixard_host_schedulable Whether this host is schedulable",
        "# TYPE blixard_host_schedulable gauge",
        "blixard_host_schedulable{host=\""
          <> host_id
          <> "\"} "
          <> case host.schedulable {
          True -> "1"
          False -> "0"
        },
        // Resources metrics
        "# HELP blixard_host_cpu_cores Available CPU cores on this host",
        "# TYPE blixard_host_cpu_cores gauge",
        "blixard_host_cpu_cores{host=\""
          <> host_id
          <> "\"} "
          <> int.to_string(host.available_resources.cpu_cores),
        "# HELP blixard_host_memory_mb Available memory in MB on this host",
        "# TYPE blixard_host_memory_mb gauge",
        "blixard_host_memory_mb{host=\""
          <> host_id
          <> "\"} "
          <> int.to_string(host.available_resources.memory_mb),
        "# HELP blixard_host_disk_gb Available disk space in GB on this host",
        "# TYPE blixard_host_disk_gb gauge",
        "blixard_host_disk_gb{host=\""
          <> host_id
          <> "\"} "
          <> int.to_string(host.available_resources.disk_gb),
        // System metrics
        "# HELP blixard_host_load Load average on this host",
        "# TYPE blixard_host_load gauge",
        "blixard_host_load{host=\""
          <> host_id
          <> "\", interval=\"1m\"} "
          <> float.to_string(load_1m),
        "blixard_host_load{host=\""
          <> host_id
          <> "\", interval=\"5m\"} "
          <> float.to_string(load_5m),
        "blixard_host_load{host=\""
          <> host_id
          <> "\", interval=\"15m\"} "
          <> float.to_string(load_15m),
        "# HELP blixard_host_memory_usage_percent Memory usage percentage on this host",
        "# TYPE blixard_host_memory_usage_percent gauge",
        "blixard_host_memory_usage_percent{host=\""
          <> host_id
          <> "\"} "
          <> float.to_string(mem_usage),
        "# HELP blixard_host_disk_usage_percent Disk usage percentage on this host",
        "# TYPE blixard_host_disk_usage_percent gauge",
        "blixard_host_disk_usage_percent{host=\""
          <> host_id
          <> "\"} "
          <> float.to_string(disk_usage),
      ]

      Ok(string.join(metrics_list, "\n"))
    }
    Error(err), _ -> Error(err)
    _, Error(err) -> Error(err)
  }
}
