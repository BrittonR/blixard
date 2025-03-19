//// src/blixard/host_agent/cli.gleam

///
/// CLI interface for interacting with a running host agent
import blixard/domain/types as domain_types
import blixard/host_agent/types
import gleam/dynamic
import gleam/erlang
import gleam/erlang/atom
import gleam/erlang/process.{type Subject}
import gleam/io
import gleam/list
import gleam/option.{None, Some}
import gleam/result
import gleam/string

// External function to find a registered process by atom
@external(erlang, "erlang", "whereis")
fn erlang_whereis(name: atom.Atom) -> dynamic.Dynamic

// External function to check if a term is a pid
@external(erlang, "erlang", "is_pid")
fn is_pid(term: dynamic.Dynamic) -> Bool

// Send command to a running host agent
pub fn send_command(command: types.Command) -> Result(dynamic.Dynamic, String) {
  // Get the agent PID
  let agent_name = atom.create_from_string("blixard_host_agent")
  let agent_pid = erlang_whereis(agent_name)

  case is_pid(agent_pid) {
    True -> {
      // Create a new reply subject
      let reply_subject = process.new_subject()

      // Send the command with our subject
      process.send(cast_pid(agent_pid), command)

      // Wait for a response with a timeout
      case process.receive(reply_subject, 5000) {
        Ok(result) -> Ok(result)
        Error(_) -> Error("Timeout waiting for response from host agent")
      }
    }
    False ->
      Error(
        "Host agent not found. Is it running? Try starting it with --host-agent",
      )
  }
}

// Cast a dynamic value to a process Subject
@external(erlang, "erlang", "binary_to_term")
fn cast_pid(pid: dynamic.Dynamic) -> Subject(types.Command)

// List all VMs on the host
pub fn list_vms() -> Result(List(types.MicroVMStatus), String) {
  // Create a subject for the reply
  let reply_subject = process.new_subject()

  // Send the command
  let result = send_command(types.ListVMs(reply_subject))

  // For now, to make it compile, we return an empty list
  // In a real implementation, we'd handle the dynamic result properly
  case result {
    Ok(_) -> Ok([])
    // Placeholder
    Error(err) -> Error(err)
  }
}

// Start a VM
pub fn start_vm(vm_id: String) -> Result(Nil, String) {
  // Create a subject for the reply
  let reply_subject = process.new_subject()

  // Send the command
  let result = send_command(types.StartVM(vm_id, reply_subject))

  // For now, to make it compile, we return Nil
  // In a real implementation, we'd handle the dynamic result properly
  case result {
    Ok(_) -> Ok(Nil)
    // Placeholder
    Error(err) -> Error(err)
  }
}

// Stop a VM
pub fn stop_vm(vm_id: String) -> Result(Nil, String) {
  // Create a subject for the reply
  let reply_subject = process.new_subject()

  // Send the command
  let result = send_command(types.StopVM(vm_id, reply_subject))

  // For now, to make it compile, we return Nil
  // In a real implementation, we'd handle the dynamic result properly
  case result {
    Ok(_) -> Ok(Nil)
    // Placeholder
    Error(err) -> Error(err)
  }
}

// Restart a VM
pub fn restart_vm(vm_id: String) -> Result(Nil, String) {
  // Create a subject for the reply
  let reply_subject = process.new_subject()

  // Send the command
  let result = send_command(types.RestartVM(vm_id, reply_subject))

  // For now, to make it compile, we return Nil
  // In a real implementation, we'd handle the dynamic result properly
  case result {
    Ok(_) -> Ok(Nil)
    // Placeholder
    Error(err) -> Error(err)
  }
}

// Get host status
pub fn get_status() -> Result(types.HostStatus, String) {
  // Create a subject for the reply
  let reply_subject = process.new_subject()

  // Send the command
  let result = send_command(types.GetStatus(reply_subject))

  // For now, to make it compile, we return a placeholder status
  // In a real implementation, we'd handle the dynamic result properly
  case result {
    Ok(_) ->
      Ok(types.HostStatus(
        host_id: "placeholder",
        vm_count: 0,
        running_vms: [],
        available_resources: domain_types.Resources(
          cpu_cores: 0,
          memory_mb: 0,
          disk_gb: 0,
        ),
        schedulable: False,
        load_average: [0.0, 0.0, 0.0],
        memory_usage_percent: 0.0,
        disk_usage_percent: 0.0,
      ))
    Error(err) -> Error(err)
  }
}

// Get Prometheus metrics
pub fn get_metrics() -> Result(String, String) {
  // Create a subject for the reply
  let reply_subject = process.new_subject()

  // Send the command
  let result = send_command(types.ExportPrometheusMetrics(reply_subject))

  // For now, to make it compile, we return an empty string
  // In a real implementation, we'd handle the dynamic result properly
  case result {
    Ok(_) -> Ok("")
    // Placeholder
    Error(err) -> Error(err)
  }
}
