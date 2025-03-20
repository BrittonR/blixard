//// src/blixard/config/json_reader.gleam

///
/// JSON configuration file reader for declarative VM state
import gleam/dict.{type Dict}
import gleam/dynamic
import gleam/dynamic/decode
import gleam/int
import gleam/io
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/result
import gleam/string

import blixard/domain/types
import shellout

/// VM state configuration from JSON
pub type VmStateConfig {
  VmStateConfig(
    name: String,
    desired_state: types.ResourceState,
    host_id: Option(String),
    resources: Option(types.Resources),
    flake_path: Option(String),
    auto_restart: Bool,
  )
}

/// Root configuration structure
pub type StateConfig {
  StateConfig(
    vms: List(VmStateConfig),
    default_flake_path: String,
    reconcile_interval_sec: Int,
  )
}

/// Parse a JSON file and return the state configuration
pub fn parse_config_file(path: String) -> Result(StateConfig, String) {
  // Read the JSON file
  io.println("[JSON_READER] Reading config file from: " <> path)
  case shellout.command(run: "cat", with: [path], in: ".", opt: []) {
    Ok(content) -> {
      io.println(
        "[JSON_READER] File read successful, content length: "
        <> int.to_string(string.length(content)),
      )
      io.println(
        "[JSON_READER] File content preview: "
        <> string.slice(from: content, at_index: 0, length: 50)
        <> "...",
      )
      parse_config_content(content)
    }
    Error(err) -> {
      let #(_, message) = err
      io.println("[JSON_READER][ERROR] Failed to read config file: " <> message)
      Error("Failed to read config file: " <> message)
    }
  }
}

/// Parse JSON content
pub fn parse_config_content(content: String) -> Result(StateConfig, String) {
  // Default configuration
  let default_config =
    StateConfig(
      vms: [],
      default_flake_path: "./microvm_flakes",
      reconcile_interval_sec: 60,
    )

  io.println("[JSON_READER] Attempting to parse JSON content")

  // Attempt to parse the JSON
  let decode_result = json.decode(from: content, using: fn(x) { Ok(x) })
  case decode_result {
    Ok(json_data) -> {
      io.println("[JSON_READER] JSON successfully decoded")

      // Extract global configuration with proper field access
      io.println("[JSON_READER] Extracting global configuration")
      let global_result = dynamic.field("global", fn(x) { Ok(x) })(json_data)

      let global = case global_result {
        Ok(data) -> {
          io.println("[JSON_READER] Global section found")
          data
        }
        Error(err) -> {
          io.println(
            "[JSON_READER][WARNING] No global section found: "
            <> string.inspect(err),
          )
          dynamic.from(dict.new())
        }
      }

      // Get default_flake_path
      io.println("[JSON_READER] Extracting default_flake_path")
      let default_flake_path_result =
        dynamic.field("default_flake_path", dynamic.string)(global)

      let default_flake_path = case default_flake_path_result {
        Ok(path) -> {
          io.println("[JSON_READER] default_flake_path: " <> path)
          path
        }
        Error(_) -> {
          io.println(
            "[JSON_READER] Using default flake path: "
            <> default_config.default_flake_path,
          )
          default_config.default_flake_path
        }
      }

      // Get reconcile_interval_sec
      io.println("[JSON_READER] Extracting reconcile_interval_sec")
      let reconcile_interval_sec_result =
        dynamic.field("reconcile_interval_sec", dynamic.int)(global)

      let reconcile_interval_sec = case reconcile_interval_sec_result {
        Ok(interval) -> {
          io.println(
            "[JSON_READER] reconcile_interval_sec: " <> int.to_string(interval),
          )
          interval
        }
        Error(_) -> {
          io.println(
            "[JSON_READER] Using default reconcile interval: "
            <> int.to_string(default_config.reconcile_interval_sec),
          )
          default_config.reconcile_interval_sec
        }
      }

      // Extract VMs object
      io.println("[JSON_READER] Extracting VMs object")
      let vms_result = dynamic.field("vms", fn(x) { Ok(x) })(json_data)

      let vms_object = case vms_result {
        Ok(data) -> {
          io.println("[JSON_READER] VMs section found")
          data
        }
        Error(err) -> {
          io.println(
            "[JSON_READER][WARNING] No VMs section found: "
            <> string.inspect(err),
          )
          dynamic.from(dict.new())
        }
      }

      // Extract VM configurations
      io.println("[JSON_READER] Extracting VM configurations")
      let vms = extract_vms(vms_object)
      io.println(
        "[JSON_READER] Found " <> int.to_string(list.length(vms)) <> " VMs",
      )

      // List found VMs
      case list.length(vms) {
        0 -> io.println("[JSON_READER] No VMs found in configuration")
        _ -> {
          list.each(vms, fn(vm) {
            io.println(
              "[JSON_READER] Found VM: "
              <> vm.name
              <> ", state: "
              <> string.inspect(vm.desired_state),
            )
          })
        }
      }

      // Return the complete configuration
      let config =
        StateConfig(
          vms: vms,
          default_flake_path: default_flake_path,
          reconcile_interval_sec: reconcile_interval_sec,
        )

      io.println("[JSON_READER] Configuration parsing complete")
      Ok(config)
    }
    Error(err) -> {
      io.println(
        "[JSON_READER][ERROR] Failed to parse JSON: " <> string.inspect(err),
      )
      Error("Failed to parse JSON: " <> string.inspect(err))
    }
  }
}

/// Extract VM configurations from the vms object
fn extract_vms(vms_object: dynamic.Dynamic) -> List(VmStateConfig) {
  io.println("[JSON_READER] Beginning extraction of VM configurations")

  // Extract VM keys and convert to configs
  let vm_keys = extract_vm_keys(vms_object)
  io.println("[JSON_READER] Keys found: " <> string.inspect(vm_keys))

  let vm_configs =
    list.filter_map(vm_keys, fn(vm_name) {
      io.println("[JSON_READER] Extracting config for VM: " <> vm_name)
      extract_vm_config(vms_object, vm_name)
    })

  io.println(
    "[JSON_READER] Extracted "
    <> int.to_string(list.length(vm_configs))
    <> " VM configurations",
  )
  vm_configs
}

/// Extract VM keys from the vms object
fn extract_vm_keys(vms_object: dynamic.Dynamic) -> List(String) {
  io.println("[JSON_READER] Extracting VM keys from configuration")

  // Common VM names to check - make sure to include the specific VM name from the config
  let common_names = [
    // Common server roles
    "webserver", "database", "api", "app", "frontend", "backend", "proxy",
    "loadbalancer", "cache", "search", "worker", "scheduler", "logger",
    "monitor", "admin", "auth", "mail", "cdn", "queue",
    // Custom VMs (ensure this matches your config)
    "my-microvm",
    // Numbered instances
    "vm1", "vm2", "vm3", "node1", "node2", "server1", "server2",
    // Environment types
    "dev", "test", "staging", "prod",
    // Generic names
    "main", "primary", "secondary", "backup",
  ]

  io.println(
    "[JSON_READER] Checking for VM names: " <> string.inspect(common_names),
  )

  // Try to debug what keys are actually in the vms_object
  io.println("[JSON_READER] VM object type: " <> dynamic.classify(vms_object))

  let found_vms =
    list.filter_map(common_names, fn(name) {
      io.println("[JSON_READER] Checking for VM with name: " <> name)
      let field_result = dynamic.field(name, fn(x) { Ok(x) })(vms_object)
      case field_result {
        Ok(_) -> {
          io.println("[JSON_READER] Found VM with name: " <> name)
          Ok(name)
        }
        Error(_) -> {
          Error(Nil)
        }
      }
    })

  io.println("[JSON_READER] Found VMs: " <> string.inspect(found_vms))
  found_vms
}

/// Extract VM configuration for a specific VM name
fn extract_vm_config(
  vms_object: dynamic.Dynamic,
  vm_name: String,
) -> Result(VmStateConfig, Nil) {
  io.println("[JSON_READER] Extracting configuration for VM: " <> vm_name)
  let vm_data_result = dynamic.field(vm_name, fn(x) { Ok(x) })(vms_object)

  case vm_data_result {
    Ok(data) -> {
      io.println("[JSON_READER] Found VM data for: " <> vm_name)

      // Extract VM fields with logging
      io.println("[JSON_READER] Extracting desired_state")
      let desired_state = extract_desired_state(data)
      io.println(
        "[JSON_READER] Desired state: " <> string.inspect(desired_state),
      )

      io.println("[JSON_READER] Extracting host_id")
      let host_id = extract_host_id(data)
      io.println("[JSON_READER] Host ID: " <> string.inspect(host_id))

      io.println("[JSON_READER] Extracting flake_path")
      let flake_path = extract_flake_path(data)
      io.println("[JSON_READER] Flake path: " <> string.inspect(flake_path))

      io.println("[JSON_READER] Extracting auto_restart")
      let auto_restart = extract_auto_restart(data)
      io.println("[JSON_READER] Auto restart: " <> string.inspect(auto_restart))

      io.println("[JSON_READER] Extracting resources")
      let resources = extract_resources(data)
      io.println("[JSON_READER] Resources: " <> string.inspect(resources))

      // Create the VM config
      let vm_config =
        VmStateConfig(
          name: vm_name,
          desired_state: desired_state,
          host_id: host_id,
          resources: resources,
          flake_path: flake_path,
          auto_restart: auto_restart,
        )

      io.println(
        "[JSON_READER] VM config extracted successfully for " <> vm_name,
      )
      Ok(vm_config)
    }
    Error(err) -> {
      io.println(
        "[JSON_READER][ERROR] Failed to extract VM data for "
        <> vm_name
        <> ": "
        <> string.inspect(err),
      )
      Error(Nil)
    }
  }
}

/// Extract desired_state from VM data
fn extract_desired_state(data: dynamic.Dynamic) -> types.ResourceState {
  io.println("[JSON_READER] Extracting desired_state field")
  let state_result = dynamic.field("desired_state", dynamic.string)(data)

  case state_result {
    Ok(state_str) -> {
      io.println("[JSON_READER] Found desired_state: " <> state_str)
      case parse_resource_state(state_str) {
        Ok(state) -> {
          io.println(
            "[JSON_READER] Parsed state successfully: " <> string.inspect(state),
          )
          state
        }
        Error(err) -> {
          io.println("[JSON_READER][ERROR] Invalid state value: " <> err)
          io.println("[JSON_READER] Using default state: Stopped")
          types.Stopped
        }
      }
    }
    Error(err) -> {
      io.println(
        "[JSON_READER][WARNING] No desired_state found: " <> string.inspect(err),
      )
      io.println("[JSON_READER] Using default state: Stopped")
      types.Stopped
    }
  }
}

/// Extract host_id from VM data
fn extract_host_id(data: dynamic.Dynamic) -> Option(String) {
  io.println("[JSON_READER] Extracting host_id field")
  let host_result = dynamic.field("host_id", dynamic.string)(data)

  case host_result {
    Ok(host_id) -> {
      io.println("[JSON_READER] Found host_id: " <> host_id)
      Some(host_id)
    }
    Error(err) -> {
      io.println(
        "[JSON_READER][WARNING] No host_id found: " <> string.inspect(err),
      )
      None
    }
  }
}

/// Extract flake_path from VM data
fn extract_flake_path(data: dynamic.Dynamic) -> Option(String) {
  io.println("[JSON_READER] Extracting flake_path field")
  let path_result = dynamic.field("flake_path", dynamic.string)(data)

  case path_result {
    Ok(path) -> {
      io.println("[JSON_READER] Found flake_path: " <> path)
      Some(path)
    }
    Error(err) -> {
      io.println(
        "[JSON_READER][WARNING] No flake_path found: " <> string.inspect(err),
      )
      None
    }
  }
}

/// Extract auto_restart from VM data
fn extract_auto_restart(data: dynamic.Dynamic) -> Bool {
  io.println("[JSON_READER] Extracting auto_restart field")
  let restart_result = dynamic.field("auto_restart", dynamic.bool)(data)

  case restart_result {
    Ok(restart) -> {
      io.println(
        "[JSON_READER] Found auto_restart: " <> string.inspect(restart),
      )
      restart
    }
    Error(err) -> {
      io.println(
        "[JSON_READER][WARNING] No auto_restart found: " <> string.inspect(err),
      )
      io.println("[JSON_READER] Using default auto_restart: True")
      True
    }
  }
}

/// Extract resources from VM data if all required fields are present
fn extract_resources(data: dynamic.Dynamic) -> Option(types.Resources) {
  io.println("[JSON_READER] Extracting resource fields")

  io.println("[JSON_READER] Extracting cpu_cores")
  let cpu_result = dynamic.field("cpu_cores", dynamic.int)(data)
  case cpu_result {
    Ok(cpu) ->
      io.println("[JSON_READER] Found cpu_cores: " <> int.to_string(cpu))
    Error(err) ->
      io.println(
        "[JSON_READER][WARNING] No cpu_cores found: " <> string.inspect(err),
      )
  }

  io.println("[JSON_READER] Extracting memory_mb")
  let memory_result = dynamic.field("memory_mb", dynamic.int)(data)
  case memory_result {
    Ok(memory) ->
      io.println("[JSON_READER] Found memory_mb: " <> int.to_string(memory))
    Error(err) ->
      io.println(
        "[JSON_READER][WARNING] No memory_mb found: " <> string.inspect(err),
      )
  }

  io.println("[JSON_READER] Extracting disk_gb")
  let disk_result = dynamic.field("disk_gb", dynamic.int)(data)
  case disk_result {
    Ok(disk) ->
      io.println("[JSON_READER] Found disk_gb: " <> int.to_string(disk))
    Error(err) ->
      io.println(
        "[JSON_READER][WARNING] No disk_gb found: " <> string.inspect(err),
      )
  }

  case cpu_result, memory_result, disk_result {
    Ok(cpu), Ok(memory), Ok(disk) -> {
      io.println(
        "[JSON_READER] All resource fields found, creating Resources object",
      )
      let resources =
        types.Resources(cpu_cores: cpu, memory_mb: memory, disk_gb: disk)
      Some(resources)
    }
    _, _, _ -> {
      io.println(
        "[JSON_READER][WARNING] Missing some resource fields, no Resources object created",
      )
      None
    }
  }
}

/// Parse a string to a ResourceState
fn parse_resource_state(state: String) -> Result(types.ResourceState, String) {
  io.println("[JSON_READER] Parsing resource state string: " <> state)
  let lowercase_state = string.lowercase(string.trim(state))
  io.println("[JSON_READER] Normalized state string: " <> lowercase_state)

  case lowercase_state {
    "pending" -> {
      io.println("[JSON_READER] Parsed as Pending state")
      Ok(types.Pending)
    }
    "scheduling" -> {
      io.println("[JSON_READER] Parsed as Scheduling state")
      Ok(types.Scheduling)
    }
    "provisioning" -> {
      io.println("[JSON_READER] Parsed as Provisioning state")
      Ok(types.Provisioning)
    }
    "starting" -> {
      io.println("[JSON_READER] Parsed as Starting state")
      Ok(types.Starting)
    }
    "running" -> {
      io.println("[JSON_READER] Parsed as Running state")
      Ok(types.Running)
    }
    "paused" -> {
      io.println("[JSON_READER] Parsed as Paused state")
      Ok(types.Paused)
    }
    "stopping" -> {
      io.println("[JSON_READER] Parsed as Stopping state")
      Ok(types.Stopping)
    }
    "stopped" -> {
      io.println("[JSON_READER] Parsed as Stopped state")
      Ok(types.Stopped)
    }
    "failed" -> {
      io.println("[JSON_READER] Parsed as Failed state")
      Ok(types.Failed)
    }
    _ -> {
      io.println("[JSON_READER][ERROR] Invalid state: " <> state)
      Error("Invalid state: " <> state)
    }
  }
}
