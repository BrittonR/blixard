//! Cluster configuration save/load functionality
//!
//! This module provides the ability to export and import cluster configurations,
//! allowing users to save their cluster setup and recreate it later.

use crate::BlixardResult;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

/// Complete cluster configuration that can be saved/loaded
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfiguration {
    /// Configuration metadata
    pub metadata: ConfigMetadata,

    /// List of nodes in the cluster
    pub nodes: Vec<NodeConfiguration>,

    /// List of VMs in the cluster
    pub vms: Vec<VmConfiguration>,

    /// VM templates
    pub vm_templates: Vec<VmTemplate>,

    /// Cluster-wide settings
    pub settings: ClusterSettings,
}

/// Metadata about the configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigMetadata {
    /// Configuration format version
    pub version: String,

    /// When this configuration was created
    pub created_at: String,

    /// Optional description
    pub description: Option<String>,

    /// Cluster name
    pub cluster_name: Option<String>,
}

/// Node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfiguration {
    /// Node ID
    pub id: u64,

    /// Node bind address
    pub bind_address: String,

    /// Data directory
    pub data_dir: String,

    /// VM backend type
    pub vm_backend: String,

    /// Whether this node uses Tailscale
    pub use_tailscale: bool,

    /// Node role (leader/follower)
    pub role: String,

    /// Node status
    pub status: String,
}

/// VM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmConfiguration {
    /// VM name
    pub name: String,

    /// Number of vCPUs
    pub vcpus: u32,

    /// Memory in MB
    pub memory: u32,

    /// Node ID where VM is assigned
    pub node_id: u64,

    /// Current status
    pub status: String,

    /// Optional config path
    pub config_path: Option<String>,
}

/// VM template configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmTemplate {
    /// Template name
    pub name: String,

    /// Template description
    pub description: String,

    /// Default vCPUs
    pub vcpus: u32,

    /// Default memory in MB
    pub memory: u32,

    /// Template type
    pub template_type: String,
}

/// Cluster-wide settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterSettings {
    /// Default VM backend
    pub default_vm_backend: String,

    /// Default refresh rate in seconds
    pub refresh_rate: u64,

    /// Maximum log entries to keep
    pub max_log_entries: usize,

    /// Whether to auto-connect on startup
    pub auto_connect: bool,
}

impl ClusterConfiguration {
    /// Create a new cluster configuration with current timestamp
    pub fn new(description: Option<String>) -> Self {
        Self {
            metadata: ConfigMetadata {
                version: "1.0".to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                description,
                cluster_name: None,
            },
            nodes: Vec::new(),
            vms: Vec::new(),
            vm_templates: Vec::new(),
            settings: ClusterSettings::default(),
        }
    }

    /// Save configuration to a YAML file
    pub async fn save_to_file(&self, path: &Path) -> BlixardResult<()> {
        let yaml = serde_yaml::to_string(self).map_err(|e| crate::BlixardError::Internal {
            message: format!("Failed to serialize configuration: {}", e),
        })?;

        fs::write(path, yaml)
            .await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to write configuration file: {}", e),
            })?;

        Ok(())
    }

    /// Load configuration from a YAML file
    pub async fn load_from_file(path: &Path) -> BlixardResult<Self> {
        let yaml = fs::read_to_string(path)
            .await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to read configuration file: {}", e),
            })?;

        let config: Self =
            serde_yaml::from_str(&yaml).map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to parse configuration: {}", e),
            })?;

        // Validate configuration version
        if config.metadata.version != "1.0" {
            return Err(crate::BlixardError::Internal {
                message: format!(
                    "Unsupported configuration version: {}",
                    config.metadata.version
                ),
            });
        }

        Ok(config)
    }

    /// Validate the configuration
    pub fn validate(&self) -> BlixardResult<()> {
        // Check for duplicate node IDs
        let mut node_ids = std::collections::HashSet::new();
        for node in &self.nodes {
            if !node_ids.insert(node.id) {
                return Err(crate::BlixardError::Internal {
                    message: format!("Duplicate node ID: {}", node.id),
                });
            }
        }

        // Check for duplicate VM names
        let mut vm_names = std::collections::HashSet::new();
        for vm in &self.vms {
            if !vm_names.insert(&vm.name) {
                return Err(crate::BlixardError::Internal {
                    message: format!("Duplicate VM name: {}", vm.name),
                });
            }

            // Check that VM is assigned to a valid node
            if !node_ids.contains(&vm.node_id) {
                return Err(crate::BlixardError::Internal {
                    message: format!(
                        "VM '{}' assigned to non-existent node {}",
                        vm.name, vm.node_id
                    ),
                });
            }
        }

        Ok(())
    }
}

impl Default for ClusterSettings {
    fn default() -> Self {
        Self {
            default_vm_backend: "microvm".to_string(),
            refresh_rate: 5,
            max_log_entries: 1000,
            auto_connect: true,
        }
    }
}
