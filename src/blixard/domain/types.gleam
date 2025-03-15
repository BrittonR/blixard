//// src/blixard/domain/types.gleam

///
/// Core domain models for the Blixard orchestrator
import gleam/dict.{type Dict}
import gleam/option.{type Option, None, Some}

/// Unique identifier for resources
pub type Uuid =
  String

/// Resource states for state machines
pub type ResourceState {
  Pending
  Scheduling
  Provisioning
  Starting
  Running
  Paused
  Stopping
  Stopped
  Failed
}

/// VM types that can be deployed
pub type VmType {
  /// Long-running VM with its own Tailscale identity
  Persistent
  /// Short-lived VM that uses subnet routing
  Serverless
}

/// Resource requirements for a VM
pub type Resources {
  Resources(cpu_cores: Int, memory_mb: Int, disk_gb: Int)
}

/// Storage volume configuration
pub type StorageVolume {
  StorageVolume(
    id: Uuid,
    name: String,
    size_gb: Int,
    path: String,
    persistent: Bool,
  )
}

/// Network interface configuration
pub type NetworkInterface {
  NetworkInterface(
    id: Uuid,
    name: String,
    ipv4_address: Option(String),
    ipv6_address: Option(String),
    mac_address: Option(String),
  )
}

/// Tailscale integration configuration
pub type TailscaleConfig {
  TailscaleConfig(
    enabled: Bool,
    auth_key: Option(String),
    hostname: String,
    tags: List(String),
    // Whether the VM uses its own Tailscale identity (persistent)
    // or uses subnet routing (serverless)
    direct_client: Bool,
  )
}

/// NixOS configuration for the VM
pub type NixosConfig {
  NixosConfig(
    // Path to the NixOS configuration
    config_path: String,
    // Specific configuration overrides
    overrides: Dict(String, String),
    // URL to the configuration in a Nix binary cache
    cache_url: Option(String),
  )
}

/// MicroVM definition
pub type MicroVm {
  MicroVm(
    id: Uuid,
    name: String,
    description: Option(String),
    vm_type: VmType,
    resources: Resources,
    state: ResourceState,
    host_id: Option(Uuid),
    storage_volumes: List(StorageVolume),
    network_interfaces: List(NetworkInterface),
    tailscale_config: TailscaleConfig,
    nixos_config: NixosConfig,
    // Additional metadata
    labels: Dict(String, String),
    created_at: String,
    updated_at: String,
  )
}

/// Host machine definition
pub type Host {
  Host(
    id: Uuid,
    name: String,
    description: Option(String),
    // IP address for control plane communication
    control_ip: String,
    // Connection status
    connected: Bool,
    // Available resources
    available_resources: Resources,
    // Total resources
    total_resources: Resources,
    // VMs currently running on this host
    vm_ids: List(Uuid),
    // Whether this host can run new VMs
    schedulable: Bool,
    // Tags for scheduling constraints
    tags: List(String),
    // Additional metadata
    labels: Dict(String, String),
    created_at: String,
    updated_at: String,
  )
}
