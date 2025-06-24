use serde::{Serialize, Deserialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VmConfig {
    pub name: String,
    pub vm_index: u32, // Unique index for static IP assignment (1-254)
    pub hypervisor: Hypervisor,
    pub vcpus: u32,
    pub memory: u32, // MB
    pub networks: Vec<NetworkConfig>,
    pub volumes: Vec<VolumeConfig>,
    pub nixos_modules: Vec<NixModule>,
    pub flake_modules: Vec<String>, // References to reusable modules
    pub kernel: Option<KernelConfig>,
    pub init_command: Option<String>,
}

impl Default for VmConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            vm_index: 1,
            hypervisor: Hypervisor::CloudHypervisor,
            vcpus: 1,
            memory: 512,
            networks: Vec::new(),
            volumes: Vec::new(),
            nixos_modules: Vec::new(),
            flake_modules: Vec::new(),
            kernel: None,
            init_command: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Hypervisor {
    CloudHypervisor,
    Firecracker,
    Qemu,
}

impl std::fmt::Display for Hypervisor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Hypervisor::CloudHypervisor => write!(f, "cloud-hypervisor"),
            Hypervisor::Firecracker => write!(f, "firecracker"),
            Hypervisor::Qemu => write!(f, "qemu"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NetworkConfig {
    /// Traditional TAP interface with bridge
    Tap {
        name: String,
        bridge: Option<String>,
        mac: Option<String>,
    },
    /// Routed network with macvtap interface (recommended)
    Routed {
        id: String,           // Interface ID (e.g., "vm-test")
        mac: String,          // MAC address (e.g., "02:00:00:01:01:01") 
        ip: String,           // VM IP address (e.g., "10.0.0.10")
        gateway: String,      // Gateway IP (usually "10.0.0.1")
        subnet: String,       // Subnet CIDR (e.g., "10.0.0.0/24")
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VolumeConfig {
    RootDisk { 
        size: u64, // MB
    },
    DataDisk { 
        path: String,
        size: u64,
        read_only: bool,
    },
    Share {
        tag: String,
        source: PathBuf,
        mount_point: PathBuf,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NixModule {
    Inline(String),
    File(PathBuf),
    FlakePart(String), // Reference to a flake-parts module
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KernelConfig {
    pub package: Option<String>,
    pub cmdline: Option<String>,
}