use super::app::{
    ClusterInfo, ClusterNodeInfo, ClusterResourceInfo, NodeResourceInfo, PlacementStrategy, VmInfo,
};
use crate::{client::UnifiedClient, BlixardResult};
use blixard_core::{
    iroh_types::{
        ClusterResourceSummaryRequest, ClusterStatusRequest, CreateVmRequest,
        CreateVmWithSchedulingRequest, DeleteVmRequest, GetVmStatusRequest, ListVmsRequest,
        ScheduleVmPlacementRequest, StartVmRequest, StopVmRequest,
    },
    types::VmStatus,
};
use std::time::Duration;
use tokio::time::sleep;

/// Configuration for network operation retry behavior
///
/// Defines parameters for exponential backoff retry logic used
/// throughout the VM client to handle network failures gracefully.
/// This helps provide resilient connectivity in distributed environments.
///
/// # Fields
///
/// * `max_attempts` - Maximum number of retry attempts before giving up
/// * `initial_delay` - Starting delay between retry attempts
/// * `max_delay` - Maximum delay between retries (caps exponential backoff)
/// * `backoff_multiplier` - Factor to multiply delay by each retry
/// * `request_timeout` - Timeout for individual network requests
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    #[allow(dead_code)]
    pub max_attempts: u32,
    /// Initial delay between retries
    #[allow(dead_code)]
    pub initial_delay: Duration,
    /// Maximum delay between retries
    #[allow(dead_code)]
    pub max_delay: Duration,
    /// Exponential backoff multiplier
    #[allow(dead_code)]
    pub backoff_multiplier: f64,
    /// Timeout for individual requests
    #[allow(dead_code)]
    pub request_timeout: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            backoff_multiplier: 2.0,
            request_timeout: Duration::from_secs(10),
        }
    }
}

/// Client for communicating with Blixard cluster nodes
///
/// Provides a high-level interface for VM and cluster operations,
/// with built-in retry logic, error handling, and type conversions
/// suitable for TUI applications. Wraps the lower-level UnifiedClient
/// with TUI-specific conveniences.
///
/// # Features
///
/// - Automatic retry with exponential backoff
/// - Type conversion from protocol types to TUI types
/// - Comprehensive error handling and reporting
/// - Support for VM lifecycle operations
/// - Cluster status and resource monitoring
/// - VM placement and scheduling operations
///
/// # Examples
///
/// ```rust
/// # use blixard::tui::vm_client::VmClient;
/// # async fn example() -> blixard::BlixardResult<()> {
/// let mut client = VmClient::new("127.0.0.1:7001").await?;
/// let vms = client.list_vms().await?;
/// println!("Found {} VMs", vms.len());
/// # Ok(())
/// # }
/// ```
pub struct VmClient {
    client: UnifiedClient,
}

impl VmClient {
    /// Create a new VM client with default retry configuration
    ///
    /// Establishes a connection to the Blixard cluster at the specified address
    /// using default retry settings (3 attempts, exponential backoff).
    ///
    /// # Arguments
    ///
    /// * `addr` - Network address of a cluster node (e.g., "127.0.0.1:7001")
    ///
    /// # Returns
    ///
    /// * `Ok(VmClient)` - Successfully connected client
    /// * `Err(BlixardError)` - Connection failed after all retries
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use blixard::tui::vm_client::VmClient;
    /// # async fn example() -> blixard::BlixardResult<()> {
    /// let client = VmClient::new("127.0.0.1:7001").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new(addr: &str) -> BlixardResult<Self> {
        Self::new_with_config(addr, RetryConfig::default()).await
    }

    /// Create a new VM client with custom retry configuration
    ///
    /// Similar to `new()` but allows customization of retry behavior
    /// for environments with different network characteristics.
    ///
    /// # Arguments
    ///
    /// * `addr` - Network address of a cluster node
    /// * `retry_config` - Custom retry configuration
    ///
    /// # Returns
    ///
    /// * `Ok(VmClient)` - Successfully connected client
    /// * `Err(BlixardError)` - Connection failed after all retries
    pub async fn new_with_config(addr: &str, retry_config: RetryConfig) -> BlixardResult<Self> {
        // Try to connect with retries
        let client = Self::retry_operation(
            || async {
                UnifiedClient::new(addr)
                    .await
                    .map_err(|e| crate::BlixardError::Internal {
                        message: format!("Failed to connect to blixard node: {}", e),
                    })
            },
            &retry_config,
            "connect",
        )
        .await?;

        Ok(Self { client })
    }

    /// Execute an operation with retry logic
    async fn retry_operation<F, Fut, T>(
        operation: F,
        config: &RetryConfig,
        operation_name: &str,
    ) -> BlixardResult<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = BlixardResult<T>>,
    {
        let mut attempt = 0;
        let mut delay = config.initial_delay;

        loop {
            attempt += 1;

            match tokio::time::timeout(config.request_timeout, operation()).await {
                Ok(Ok(result)) => return Ok(result),
                Ok(Err(e)) if attempt >= config.max_attempts => return Err(e),
                Ok(Err(e)) => {
                    tracing::warn!(
                        "Operation '{}' failed (attempt {}/{}): {}",
                        operation_name,
                        attempt,
                        config.max_attempts,
                        e
                    );
                }
                Err(_) if attempt >= config.max_attempts => {
                    return Err(crate::BlixardError::Internal {
                        message: format!(
                            "Operation '{}' timed out after {} attempts",
                            operation_name, config.max_attempts
                        ),
                    });
                }
                Err(_) => {
                    tracing::warn!(
                        "Operation '{}' timed out (attempt {}/{})",
                        operation_name,
                        attempt,
                        config.max_attempts
                    );
                }
            }

            // Wait before retry with exponential backoff
            sleep(delay).await;
            delay = std::cmp::min(
                Duration::from_secs_f64(delay.as_secs_f64() * config.backoff_multiplier),
                config.max_delay,
            );
        }
    }

    /// Retrieve a list of all VMs in the cluster
    ///
    /// Fetches current VM information from the cluster and converts
    /// protocol types to TUI-friendly VmInfo structures. This includes
    /// status, resource allocation, and placement information.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<VmInfo>)` - List of all VMs with current status
    /// * `Err(BlixardError)` - Failed to retrieve VM list
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use blixard::tui::vm_client::VmClient;
    /// # async fn example(client: &mut VmClient) -> blixard::BlixardResult<()> {
    /// let vms = client.list_vms().await?;
    /// for vm in vms {
    ///     println!("VM {}: {:?} on node {}", vm.name, vm.status, vm.node_id);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_vms(&mut self) -> BlixardResult<Vec<VmInfo>> {
        let request = ListVmsRequest {};
        let resp =
            self.client
                .list_vms(request)
                .await
                .map_err(|e| crate::BlixardError::Internal {
                    message: format!("Failed to list VMs: {}", e),
                })?;
        let mut vms = Vec::with_capacity(resp.vms.len());

        for vm in resp.vms {
            let status = match vm.state {
                0 => VmStatus::Stopped,  // VM_STATE_UNKNOWN -> Stopped
                1 => VmStatus::Creating, // VM_STATE_CREATED -> Creating
                2 => VmStatus::Starting, // VM_STATE_STARTING -> Starting
                3 => VmStatus::Running,  // VM_STATE_RUNNING -> Running
                4 => VmStatus::Stopping, // VM_STATE_STOPPING -> Stopping
                5 => VmStatus::Stopped,  // VM_STATE_STOPPED -> Stopped
                6 => VmStatus::Failed,   // VM_STATE_FAILED -> Failed
                _ => VmStatus::Failed,
            };

            vms.push(VmInfo {
                name: vm.name,
                status,
                vcpus: vm.vcpus,
                memory: vm.memory_mb,
                node_id: vm.node_id,
                ip_address: if vm.ip_address.is_empty() {
                    None
                } else {
                    Some(vm.ip_address)
                },
                uptime: None,             // TODO: Get from VM status
                cpu_usage: None,          // TODO: Get from VM metrics
                memory_usage: None,       // TODO: Get from VM metrics
                placement_strategy: None, // TODO: Add to proto
                created_at: None,         // TODO: Add to proto
                config_path: None,        // TODO: Add to proto
            });
        }

        Ok(vms)
    }

    /// Create a new virtual machine with specified resources
    ///
    /// Creates a VM with the given name and resource allocation.
    /// The cluster scheduler will automatically select an appropriate
    /// node for placement based on available resources.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique name for the new VM
    /// * `vcpus` - Number of virtual CPU cores to allocate
    /// * `memory` - Memory allocation in MB
    ///
    /// # Returns
    ///
    /// * `Ok(())` - VM created successfully
    /// * `Err(BlixardError)` - Creation failed (duplicate name, insufficient resources, etc.)
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use blixard::tui::vm_client::VmClient;
    /// # async fn example(client: &mut VmClient) -> blixard::BlixardResult<()> {
    /// client.create_vm("web-server-01", 2, 2048).await?;
    /// println!("VM created successfully");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_vm(&mut self, name: &str, vcpus: u32, memory: u32) -> BlixardResult<()> {
        let request = CreateVmRequest {
            name: name.to_string(),
            config_path: String::new(),
            vcpus,
            memory_mb: memory,
        };

        let resp =
            self.client
                .create_vm(request)
                .await
                .map_err(|e| crate::BlixardError::Internal {
                    message: format!("Failed to create VM: {}", e),
                })?;

        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("VM creation failed: {}", resp.message),
            });
        }

        Ok(())
    }

    /// Start a previously created virtual machine
    ///
    /// Initiates the boot process for a VM that is currently in Stopped state.
    /// The VM will transition through Starting to Running state if successful.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the VM to start
    ///
    /// # Returns
    ///
    /// * `Ok(())` - VM start initiated successfully
    /// * `Err(BlixardError)` - Start failed (VM not found, already running, etc.)
    pub async fn start_vm(&mut self, name: &str) -> BlixardResult<()> {
        let request = StartVmRequest {
            name: name.to_string(),
        };

        let resp =
            self.client
                .start_vm(request)
                .await
                .map_err(|e| crate::BlixardError::Internal {
                    message: format!("Failed to start VM: {}", e),
                })?;

        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("VM start failed: {}", resp.message),
            });
        }

        Ok(())
    }

    /// Stop a running virtual machine
    ///
    /// Gracefully shuts down a VM that is currently running.
    /// The VM will transition through Stopping to Stopped state.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the VM to stop
    ///
    /// # Returns
    ///
    /// * `Ok(())` - VM stop initiated successfully
    /// * `Err(BlixardError)` - Stop failed (VM not found, already stopped, etc.)
    pub async fn stop_vm(&mut self, name: &str) -> BlixardResult<()> {
        let request = StopVmRequest {
            name: name.to_string(),
        };

        let resp =
            self.client
                .stop_vm(request)
                .await
                .map_err(|e| crate::BlixardError::Internal {
                    message: format!("Failed to stop VM: {}", e),
                })?;

        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("VM stop failed: {}", resp.message),
            });
        }

        Ok(())
    }

    /// Permanently delete a virtual machine
    ///
    /// Removes a VM and all associated resources from the cluster.
    /// The VM must be in Stopped state before deletion.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the VM to delete
    ///
    /// # Returns
    ///
    /// * `Ok(())` - VM deleted successfully
    /// * `Err(BlixardError)` - Deletion failed (VM not found, still running, etc.)
    ///
    /// # Warning
    ///
    /// This operation is irreversible. All VM data will be lost.
    pub async fn delete_vm(&mut self, name: &str) -> BlixardResult<()> {
        let request = DeleteVmRequest {
            name: name.to_string(),
        };

        let resp =
            self.client
                .delete_vm(request)
                .await
                .map_err(|e| crate::BlixardError::Internal {
                    message: format!("Failed to delete VM: {}", e),
                })?;

        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("VM deletion failed: {}", resp.message),
            });
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn get_vm_status(&mut self, name: &str) -> BlixardResult<Option<VmInfo>> {
        let request = GetVmStatusRequest {
            name: name.to_string(),
        };

        let resp = self.client.get_vm_status(request).await.map_err(|e| {
            crate::BlixardError::Internal {
                message: format!("Failed to get VM status: {}", e),
            }
        })?;

        if !resp.found {
            return Ok(None);
        }

        let vm = match resp.vm_info {
            Some(vm) => vm,
            None => {
                return Err(crate::BlixardError::Internal {
                    message: "VM info not found in response".to_string(),
                });
            }
        };
        let status = match vm.state {
            0 => VmStatus::Stopped,  // VM_STATE_UNKNOWN -> Stopped
            1 => VmStatus::Creating, // VM_STATE_CREATED -> Creating
            2 => VmStatus::Starting, // VM_STATE_STARTING -> Starting
            3 => VmStatus::Running,  // VM_STATE_RUNNING -> Running
            4 => VmStatus::Stopping, // VM_STATE_STOPPING -> Stopping
            5 => VmStatus::Stopped,  // VM_STATE_STOPPED -> Stopped
            6 => VmStatus::Failed,   // VM_STATE_FAILED -> Failed
            _ => VmStatus::Failed,
        };

        Ok(Some(VmInfo {
            name: vm.name,
            status,
            vcpus: vm.vcpus,
            memory: vm.memory_mb,
            node_id: vm.node_id,
            ip_address: if vm.ip_address.is_empty() {
                None
            } else {
                Some(vm.ip_address)
            },
            uptime: None,             // TODO: Get from VM status
            cpu_usage: None,          // TODO: Get from VM metrics
            memory_usage: None,       // TODO: Get from VM metrics
            placement_strategy: None, // TODO: Add to proto
            created_at: None,         // TODO: Add to proto
            config_path: None,        // TODO: Add to proto
        }))
    }

    /// Get comprehensive cluster status and membership information
    ///
    /// Retrieves current cluster state including leader information,
    /// Raft consensus status, and details about all member nodes.
    /// This is essential for cluster monitoring and health assessment.
    ///
    /// # Returns
    ///
    /// * `Ok(ClusterInfo)` - Current cluster status and node information
    /// * `Err(BlixardError)` - Failed to retrieve cluster status
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use blixard::tui::vm_client::VmClient;
    /// # async fn example(client: &mut VmClient) -> blixard::BlixardResult<()> {
    /// let status = client.get_cluster_status().await?;
    /// println!("Cluster has {} nodes, leader is node {}", 
    ///          status.node_count, status.leader_id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_cluster_status(&mut self) -> BlixardResult<ClusterInfo> {
        let request = ClusterStatusRequest {};

        let status = self.client.get_cluster_status(request).await.map_err(|e| {
            crate::BlixardError::Internal {
                message: format!("Failed to get cluster status: {}", e),
            }
        })?;

        // Map all nodes with detailed info
        let nodes: Vec<ClusterNodeInfo> = status
            .nodes
            .iter()
            .map(|node| {
                let state_name = match node.state {
                    0 => "Unknown",
                    1 => "Follower",
                    2 => "Candidate",
                    3 => "Leader",
                    _ => "Invalid",
                };
                ClusterNodeInfo {
                    id: node.id,
                    address: node.address.clone(),
                    state: state_name.to_string(),
                    is_current: node.id == status.leader_id || status.nodes.len() == 1, // Rough heuristic
                }
            })
            .collect();

        // Find current node info
        let (current_node_id, current_node_state) = nodes
            .iter()
            .find(|node| node.id == status.leader_id || status.nodes.len() == 1)
            .map(|node| (node.id, node.state.clone()))
            .unwrap_or((0, "Unknown".to_string()));

        Ok(ClusterInfo {
            leader_id: status.leader_id,
            term: status.term,
            node_count: status.nodes.len(),
            current_node_id,
            current_node_state,
            nodes,
        })
    }

    /// Create VM with intelligent scheduling
    #[allow(dead_code)]
    pub async fn create_vm_with_scheduling(
        &mut self,
        name: &str,
        vcpus: u32,
        memory: u32,
        placement_strategy: PlacementStrategy,
    ) -> BlixardResult<(u64, String)> {
        let strategy_proto = match placement_strategy {
            PlacementStrategy::MostAvailable => 0, // PLACEMENT_STRATEGY_MOST_AVAILABLE
            PlacementStrategy::LeastAvailable => 1, // PLACEMENT_STRATEGY_LEAST_AVAILABLE
            PlacementStrategy::RoundRobin => 2,    // PLACEMENT_STRATEGY_ROUND_ROBIN
            PlacementStrategy::Manual => 3,        // PLACEMENT_STRATEGY_MANUAL
        };

        let request = CreateVmWithSchedulingRequest {
            name: name.to_string(),
            config_path: String::new(),
            vcpus,
            memory_mb: memory,
            strategy: strategy_proto,
        };

        let resp = self
            .client
            .create_vm_with_scheduling(request)
            .await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to create VM with scheduling: {}", e),
            })?;

        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("VM creation with scheduling failed: {}", resp.message),
            });
        }

        Ok((resp.target_node_id, resp.placement_reason))
    }

    /// Get cluster resource summary
    #[allow(dead_code)]
    pub async fn get_cluster_resources(&mut self) -> BlixardResult<ClusterResourceInfo> {
        let request = ClusterResourceSummaryRequest {};

        let resp = self
            .client
            .get_cluster_resource_summary(request)
            .await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to get cluster resources: {}", e),
            })?;

        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("Failed to get cluster resources: {}", resp.message),
            });
        }

        let summary = resp.summary.ok_or_else(|| crate::BlixardError::Internal {
            message: "No cluster resource summary in response".to_string(),
        })?;

        let nodes = summary
            .nodes
            .into_iter()
            .map(|node| {
                let capabilities = node.capabilities.unwrap_or_default();
                NodeResourceInfo {
                    node_id: node.node_id,
                    cpu_cores: capabilities.cpu_cores,
                    memory_mb: capabilities.memory_mb,
                    disk_gb: capabilities.disk_gb,
                    used_vcpus: node.used_vcpus,
                    used_memory_mb: node.used_memory_mb,
                    used_disk_gb: node.used_disk_gb,
                    running_vms: node.running_vms,
                    features: capabilities.features,
                }
            })
            .collect();

        Ok(ClusterResourceInfo {
            total_nodes: summary.total_nodes,
            total_vcpus: summary.total_vcpus,
            used_vcpus: summary.used_vcpus,
            total_memory_mb: summary.total_memory_mb,
            used_memory_mb: summary.used_memory_mb,
            total_disk_gb: summary.total_disk_gb,
            used_disk_gb: summary.used_disk_gb,
            total_running_vms: summary.total_running_vms,
            nodes,
        })
    }

    /// Schedule VM placement (dry run)
    #[allow(dead_code)]
    pub async fn schedule_vm_placement(
        &mut self,
        name: &str,
        vcpus: u32,
        memory: u32,
        placement_strategy: PlacementStrategy,
    ) -> BlixardResult<(u64, String, Vec<u64>)> {
        let strategy_proto = match placement_strategy {
            PlacementStrategy::MostAvailable => 0,
            PlacementStrategy::LeastAvailable => 1,
            PlacementStrategy::RoundRobin => 2,
            PlacementStrategy::Manual => 3,
        };

        let request = ScheduleVmPlacementRequest {
            name: name.to_string(),
            config_path: String::new(),
            vcpus,
            memory_mb: memory,
            strategy: strategy_proto,
        };

        let resp = self
            .client
            .schedule_vm_placement(request)
            .await
            .map_err(|e| crate::BlixardError::Internal {
                message: format!("Failed to schedule VM placement: {}", e),
            })?;

        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("VM placement scheduling failed: {}", resp.message),
            });
        }

        Ok((
            resp.target_node_id,
            resp.placement_reason,
            resp.alternative_nodes,
        ))
    }

    /// Add a new node to the cluster
    ///
    /// Requests that a node join the existing cluster, expanding cluster
    /// capacity and providing additional redundancy. The joining node
    /// must be reachable and compatible with the cluster version.
    ///
    /// # Arguments
    ///
    /// * `node_id` - Unique identifier for the joining node
    /// * `bind_address` - Network address where the node can be reached
    ///
    /// # Returns
    ///
    /// * `Ok(String)` - Success message from the cluster
    /// * `Err(BlixardError)` - Join operation failed
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use blixard::tui::vm_client::VmClient;
    /// # async fn example(client: &mut VmClient) -> blixard::BlixardResult<()> {\
    /// let result = client.join_cluster(2, \"192.168.1.100:7001\").await?;\
    /// println!(\"Node joined: {}\", result);\
    /// # Ok(())\
    /// # }\
    /// ```
    pub async fn join_cluster(
        &mut self,
        node_id: u64,
        bind_address: &str,
    ) -> BlixardResult<String> {
        use blixard_core::iroh_types::JoinRequest;

        let request = JoinRequest {
            node_id,
            bind_address: bind_address.to_string(),
            p2p_node_addr: None,
        };

        let resp =
            self.client
                .join_cluster(request)
                .await
                .map_err(|e| crate::BlixardError::Internal {
                    message: format!("Failed to join cluster: {}", e),
                })?;

        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("Join cluster failed: {}", resp.message),
            });
        }

        Ok(resp.message)
    }

    /// Migrate a virtual machine to a different cluster node
    ///
    /// Moves a VM from its current node to a target node, with optional
    /// live migration to minimize downtime. This is useful for load balancing,
    /// maintenance, or resource optimization.
    ///
    /// # Arguments
    ///
    /// * `vm_name` - Name of the VM to migrate
    /// * `target_node_id` - ID of the destination node
    /// * `live_migration` - Whether to perform live migration (minimize downtime)
    ///
    /// # Returns
    ///
    /// * `Ok((source_node_id, target_node_id, message))` - Migration details
    /// * `Err(BlixardError)` - Migration failed
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use blixard::tui::vm_client::VmClient;
    /// # async fn example(client: &mut VmClient) -> blixard::BlixardResult<()> {
    /// let (source, target, msg) = client.migrate_vm("web-01", 2, true).await?;
    /// println!("Migrated from node {} to node {}: {}", source, target, msg);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn migrate_vm(
        &mut self,
        vm_name: &str,
        target_node_id: u64,
        live_migration: bool,
    ) -> BlixardResult<(u64, u64, String)> {
        use blixard_core::iroh_types::MigrateVmRequest;

        let request = MigrateVmRequest {
            vm_name: vm_name.to_string(),
            target_node_id,
            live_migration,
            force: false,
        };

        let resp =
            self.client
                .migrate_vm(request)
                .await
                .map_err(|e| crate::BlixardError::Internal {
                    message: format!("Failed to migrate VM: {}", e),
                })?;

        if !resp.success {
            return Err(crate::BlixardError::Internal {
                message: format!("VM migration failed: {}", resp.message),
            });
        }

        Ok((resp.source_node_id, resp.target_node_id, resp.message))
    }
}
