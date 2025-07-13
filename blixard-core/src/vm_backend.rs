use async_trait::async_trait;
use redb::{Database, ReadableTable};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use uuid;

use crate::error::{BlixardError, BlixardResult};
#[cfg(feature = "observability")]
use crate::metrics_otel::{attributes, metrics, Timer};
// use crate::patterns::{CircuitBreaker, CircuitBreakerConfig, RetryConfig, retry};
use crate::types::{VmCommand, VmConfig, VmStatus};
use crate::vm_health_types::{HealthCheckResult, HealthCheckType, VmHealthStatus};
use crate::vm_scheduler::{PlacementDecision, PlacementStrategy, VmScheduler};
// Temporarily disabled: tracing_otel uses tonic
// use crate::tracing_otel;

#[cfg(feature = "failpoints")]
use crate::fail_point;

/// Abstract interface for VM backend implementations
///
/// This trait defines the contract that all VM backends must implement,
/// allowing the core distributed systems logic to be decoupled from
/// specific VM implementations (microvm.nix, Docker, Firecracker, etc.)
///
/// ## Design Principles
///
/// ### 1. Async-First Architecture
/// All operations are async to handle potentially long-running VM operations
/// without blocking the event loop. Implementations must be careful about:
/// - **Cancellation Safety**: Operations should handle task cancellation gracefully
/// - **Resource Cleanup**: Failed operations must clean up partial state
/// - **Timeout Handling**: Long operations should respect reasonable timeouts
///
/// ### 2. Error Recovery
/// Operations may fail at any point due to:
/// - **Resource Exhaustion**: Out of memory, disk space, or CPU cores
/// - **Network Issues**: Unable to reach hypervisor or VM
/// - **Configuration Errors**: Invalid VM configuration or missing files
/// - **Hardware Failures**: Host system issues or hardware faults
///
/// ### 3. Thread Safety
/// Implementations must be `Send + Sync` for multi-threaded access:
/// - **Concurrent Operations**: Multiple VMs can be managed simultaneously
/// - **Shared State**: Backend state must be protected with appropriate synchronization
/// - **No Blocking**: Must not perform blocking I/O in async contexts
///
/// ### 4. Observability
/// All operations should provide comprehensive logging and metrics:
/// - **Structured Logging**: Use `tracing` for contextual log messages
/// - **Error Context**: Include detailed error information for debugging
/// - **Performance Metrics**: Track operation latency and resource usage
///
/// ## Implementation Guidelines
///
/// ### Resource Management
/// ```rust,no_run
/// use blixard_core::{VmBackend, VmConfig, BlixardResult};
/// use async_trait::async_trait;
///
/// struct MyVmBackend {
///     // Keep resource tracking state
///     resource_tracker: Arc<ResourceTracker>,
/// }
///
/// #[async_trait]
/// impl VmBackend for MyVmBackend {
///     async fn create_vm(&self, config: &VmConfig, node_id: u64) -> BlixardResult<()> {
///         // 1. Validate configuration
///         self.validate_vm_config(config).await?;
///         
///         // 2. Check resource availability
///         self.resource_tracker.reserve_resources(config).await?;
///         
///         // 3. Create VM with rollback on failure
///         match self.do_create_vm(config, node_id).await {
///             Ok(()) => Ok(()),
///             Err(e) => {
///                 // Clean up on failure
///                 self.resource_tracker.release_resources(config).await?;
///                 Err(e)
///             }
///         }
///     }
/// }
/// ```
///
/// ### Error Handling Patterns
/// ```rust,no_run
/// use blixard_core::{BlixardError, BlixardResult};
///
/// async fn start_vm_example(&self, name: &str) -> BlixardResult<()> {
///     // Check if VM exists
///     let vm_config = self.get_vm_config(name).await
///         .map_err(|e| BlixardError::VmError(format!("VM '{}' not found: {}", name, e)))?;
///     
///     // Validate VM can be started
///     if !self.can_start_vm(&vm_config).await? {
///         return Err(BlixardError::VmError(
///             format!("VM '{}' cannot be started (insufficient resources)", name)
///         ));
///     }
///     
///     // Start with timeout
///     tokio::time::timeout(
///         std::time::Duration::from_secs(30),
///         self.hypervisor_start_vm(name)
///     ).await
///     .map_err(|_| BlixardError::Timeout {
///         operation: format!("start VM '{}'", name),
///         timeout_secs: 30,
///     })?
/// }
/// ```
///
/// ### Health Check Implementation
/// ```rust,no_run
/// use blixard_core::{HealthCheckType, HealthCheckResult};
///
/// async fn perform_health_check_example(
///     &self,
///     vm_name: &str,
///     check_name: &str,
///     check_type: &HealthCheckType,
/// ) -> BlixardResult<HealthCheckResult> {
///     match check_type {
///         HealthCheckType::Tcp { host, port } => {
///             // Try TCP connection with timeout
///             let result = tokio::time::timeout(
///                 std::time::Duration::from_secs(5),
///                 tokio::net::TcpStream::connect((host.as_str(), *port))
///             ).await;
///             
///             match result {
///                 Ok(Ok(_)) => Ok(HealthCheckResult::healthy()),
///                 Ok(Err(e)) => Ok(HealthCheckResult::unhealthy(
///                     format!("TCP connection failed: {}", e)
///                 )),
///                 Err(_) => Ok(HealthCheckResult::unhealthy("Health check timeout")),
///             }
///         },
///         HealthCheckType::Http { url, expected_status } => {
///             // Implement HTTP health check
///             // ...
///         },
///         // Handle other check types
///     }
/// }
/// ```
///
/// ## Common Failure Modes
///
/// ### 1. Resource Exhaustion
/// - **Symptom**: create_vm() fails with resource errors
/// - **Recovery**: Implement resource admission control and queueing
/// - **Monitoring**: Track resource utilization metrics
///
/// ### 2. Network Connectivity
/// - **Symptom**: Operations timeout or fail with network errors
/// - **Recovery**: Implement retry logic with exponential backoff
/// - **Monitoring**: Track operation success/failure rates
///
/// ### 3. Hypervisor Issues
/// - **Symptom**: VMs fail to start or respond to commands
/// - **Recovery**: Restart hypervisor service or failover to other nodes
/// - **Monitoring**: Track hypervisor health and VM success rates
///
/// ### 4. Configuration Errors
/// - **Symptom**: VMs fail to create with validation errors
/// - **Recovery**: Validate configuration before submission
/// - **Monitoring**: Track configuration validation failure rates
///
/// ## Performance Considerations
///
/// - **Parallel Operations**: Support concurrent VM operations where possible
/// - **Resource Caching**: Cache frequently accessed VM metadata
/// - **Lazy Loading**: Only load VM state when needed
/// - **Connection Pooling**: Reuse connections to hypervisor APIs
/// - **Batch Operations**: Group similar operations for efficiency
///
/// ## Security Considerations
///
/// - **Input Validation**: Sanitize all configuration inputs
/// - **Resource Isolation**: Ensure VMs cannot access unauthorized resources
/// - **Credential Management**: Secure storage of hypervisor credentials
/// - **Network Security**: Proper firewall and network segmentation
/// - **Audit Logging**: Log all VM lifecycle events for security analysis
#[async_trait]
pub trait VmBackend: Send + Sync {
    /// Create a new VM with the given configuration
    ///
    /// This operation allocates resources and prepares the VM for execution,
    /// but does not start it. The VM will be in `Creating` status initially,
    /// transitioning to `Stopped` when creation completes successfully.
    ///
    /// # Arguments
    /// * `config` - VM configuration including resource requirements and hypervisor settings
    /// * `node_id` - Target node where the VM should be created
    ///
    /// # Errors
    /// * `BlixardError::ResourceExhausted` - Insufficient resources (CPU, memory, disk)
    /// * `BlixardError::ConfigError` - Invalid VM configuration
    /// * `BlixardError::VmError` - VM already exists or hypervisor failure
    /// * `BlixardError::NetworkError` - Network isolation setup failure
    /// * `BlixardError::Timeout` - Operation exceeded reasonable time limit
    ///
    /// # Async Behavior
    /// - **Duration**: Typically 1-5 seconds for resource allocation
    /// - **Cancellation**: Safe to cancel, will clean up partial state
    /// - **Concurrency**: Multiple VMs can be created simultaneously
    ///
    /// # Example
    /// ```rust,no_run
    /// # use blixard_core::{VmBackend, VmConfig};
    /// # async fn example(backend: &dyn VmBackend) -> Result<(), Box<dyn std::error::Error>> {
    /// let config = VmConfig {
    ///     name: "web-server".to_string(),
    ///     vcpus: 2,
    ///     memory: 1024, // MB
    ///     ..Default::default()
    /// };
    /// 
    /// backend.create_vm(&config, 1).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn create_vm(&self, config: &VmConfig, node_id: u64) -> BlixardResult<()>;

    /// Start an existing VM
    ///
    /// Transitions the VM from `Stopped` to `Running` state. The VM must have
    /// been previously created and be in a startable state.
    ///
    /// # Arguments
    /// * `name` - Unique identifier of the VM to start
    ///
    /// # Errors
    /// * `BlixardError::VmError` - VM not found, already running, or failed to start
    /// * `BlixardError::ResourceExhausted` - Resources no longer available
    /// * `BlixardError::NetworkError` - Network configuration failure
    /// * `BlixardError::Timeout` - VM failed to start within timeout period
    ///
    /// # Async Behavior
    /// - **Duration**: Typically 100ms-2s depending on hypervisor and VM size
    /// - **Cancellation**: Unsafe to cancel during critical startup phases
    /// - **Concurrency**: Multiple VMs can be started simultaneously
    /// - **Retry**: Safe to retry if operation fails
    ///
    /// # Example
    /// ```rust,no_run
    /// # use blixard_core::VmBackend;
    /// # async fn example(backend: &dyn VmBackend) -> Result<(), Box<dyn std::error::Error>> {
    /// // Start VM and wait for it to be fully running
    /// backend.start_vm("web-server").await?;
    /// 
    /// // Verify it's running
    /// if let Some(status) = backend.get_vm_status("web-server").await? {
    ///     println!("VM status: {:?}", status);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    async fn start_vm(&self, name: &str) -> BlixardResult<()>;

    /// Stop a running VM
    ///
    /// Gracefully shuts down the VM, transitioning from `Running` to `Stopped`.
    /// This operation attempts a clean shutdown first, falling back to forced
    /// termination if the graceful shutdown times out.
    ///
    /// # Arguments
    /// * `name` - Unique identifier of the VM to stop
    ///
    /// # Errors
    /// * `BlixardError::VmError` - VM not found or already stopped
    /// * `BlixardError::Timeout` - VM failed to stop within timeout period
    /// * `BlixardError::Internal` - Hypervisor communication failure
    ///
    /// # Async Behavior
    /// - **Duration**: Typically 2-30 seconds depending on guest OS and workload
    /// - **Cancellation**: Unsafe to cancel, may leave VM in inconsistent state
    /// - **Concurrency**: Multiple VMs can be stopped simultaneously
    /// - **Graceful Period**: 30 seconds for guest shutdown, then forced termination
    ///
    /// # Example
    /// ```rust,no_run
    /// # use blixard_core::VmBackend;
    /// # async fn example(backend: &dyn VmBackend) -> Result<(), Box<dyn std::error::Error>> {
    /// // Gracefully stop VM
    /// backend.stop_vm("web-server").await?;
    /// 
    /// // VM is now stopped and can be restarted later
    /// # Ok(())
    /// # }
    /// ```
    async fn stop_vm(&self, name: &str) -> BlixardResult<()>;

    /// Delete a VM and its resources
    ///
    /// Permanently removes the VM and releases all associated resources including
    /// storage, network interfaces, and metadata. This operation is irreversible.
    ///
    /// # Arguments
    /// * `name` - Unique identifier of the VM to delete
    ///
    /// # Errors
    /// * `BlixardError::VmError` - VM not found or still running
    /// * `BlixardError::StorageError` - Failed to remove VM storage
    /// * `BlixardError::NetworkError` - Failed to clean up network resources
    /// * `BlixardError::Internal` - Hypervisor or system error
    ///
    /// # Safety
    /// VM must be stopped before deletion. Attempting to delete a running VM
    /// will result in an error to prevent data loss.
    ///
    /// # Async Behavior
    /// - **Duration**: Typically 1-10 seconds depending on storage cleanup
    /// - **Cancellation**: Unsafe to cancel, may leave partial cleanup
    /// - **Concurrency**: Multiple VMs can be deleted simultaneously
    /// - **Atomicity**: Either fully succeeds or rolls back changes
    ///
    /// # Example
    /// ```rust,no_run
    /// # use blixard_core::VmBackend;
    /// # async fn example(backend: &dyn VmBackend) -> Result<(), Box<dyn std::error::Error>> {
    /// // Ensure VM is stopped first
    /// backend.stop_vm("web-server").await?;
    /// 
    /// // Permanently delete VM and all resources
    /// backend.delete_vm("web-server").await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn delete_vm(&self, name: &str) -> BlixardResult<()>;

    /// Update VM status (for health monitoring)
    ///
    /// Updates the backend's internal tracking of VM status. This is typically
    /// called by health monitoring systems to reflect the actual VM state.
    ///
    /// # Arguments
    /// * `name` - Unique identifier of the VM
    /// * `status` - New status to record
    ///
    /// # Errors
    /// * `BlixardError::VmError` - VM not found
    /// * `BlixardError::StorageError` - Failed to persist status update
    ///
    /// # Async Behavior
    /// - **Duration**: Typically <100ms for metadata update
    /// - **Cancellation**: Safe to cancel, operation is idempotent
    /// - **Concurrency**: Status updates can occur concurrently
    /// - **Consistency**: Ensures status changes are persisted
    async fn update_vm_status(&self, name: &str, status: VmStatus) -> BlixardResult<()>;

    /// Get the current status of a VM
    async fn get_vm_status(&self, name: &str) -> BlixardResult<Option<VmStatus>>;

    /// List all VMs managed by this backend
    async fn list_vms(&self) -> BlixardResult<Vec<(VmConfig, VmStatus)>>;

    /// Get the IP address of a VM (optional, not all backends support this)
    async fn get_vm_ip(&self, _name: &str) -> BlixardResult<Option<String>> {
        Ok(None) // Default implementation returns None
    }

    /// Migrate a VM from this node to another (optional, not all backends support this)
    async fn migrate_vm(
        &self,
        _name: &str,
        _target_node_id: u64,
        _live: bool,
    ) -> BlixardResult<()> {
        Err(BlixardError::NotImplemented {
            feature: "VM migration".to_string(),
        })
    }

    /// Perform a health check on a VM
    ///
    /// This method executes the specified health check against the VM and returns
    /// the result. The backend is responsible for implementing the actual health
    /// check logic based on the check type.
    async fn perform_health_check(
        &self,
        _vm_name: &str,
        _check_name: &str,
        check_type: &HealthCheckType,
    ) -> BlixardResult<HealthCheckResult> {
        // Default implementation returns not implemented
        Err(BlixardError::NotImplemented {
            feature: format!("Health check type {:?}", check_type),
        })
    }

    /// Get the current health status of a VM
    ///
    /// This returns the last known health status without performing new checks.
    async fn get_vm_health_status(&self, _vm_name: &str) -> BlixardResult<Option<VmHealthStatus>> {
        // Default implementation returns None
        Ok(None)
    }

    /// Check if the VM's console is accessible
    ///
    /// This is used for console-based health checks and debugging.
    async fn is_console_accessible(&self, _vm_name: &str) -> BlixardResult<bool> {
        // Default implementation returns false
        Ok(false)
    }

    /// Execute a command inside the VM (for script-based health checks)
    ///
    /// This requires guest agent support or SSH access to the VM.
    async fn execute_command_in_vm(
        &self,
        _vm_name: &str,
        _command: &str,
        _args: &[String],
        _timeout_secs: u64,
    ) -> BlixardResult<(i32, String, String)> {
        // Returns (exit_code, stdout, stderr)
        Err(BlixardError::NotImplemented {
            feature: "VM command execution".to_string(),
        })
    }

    /// Update the health status of a VM
    ///
    /// This stores the latest health check results for the VM.
    async fn update_vm_health_status(
        &self,
        _vm_name: &str,
        _health_status: VmHealthStatus,
    ) -> BlixardResult<()> {
        // Default implementation does nothing
        Ok(())
    }
}

/// VM Manager that coordinates between Raft consensus and VM backend
///
/// This structure bridges the distributed systems layer (Raft consensus)
/// with the actual VM implementation. It maintains the same command-based
/// architecture but delegates actual VM operations to the backend.
pub struct VmManager {
    database: Arc<Database>,
    backend: Arc<dyn VmBackend>,
    node_state: Arc<crate::node_shared::SharedNodeState>,
}

impl std::fmt::Debug for VmManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VmManager")
            .field("database", &"<Database>")
            .field("backend", &"<dyn VmBackend>")
            .field("node_state", &self.node_state)
            .finish()
    }
}

impl VmManager {
    /// Create a new VM manager with the given backend
    pub fn new(
        database: Arc<Database>,
        backend: Arc<dyn VmBackend>,
        node_state: Arc<crate::node_shared::SharedNodeState>,
    ) -> Self {
        Self {
            database,
            backend,
            node_state,
        }
    }

    /// Get a reference to the backend
    pub fn backend(&self) -> &Arc<dyn VmBackend> {
        &self.backend
    }

    /// Process a VM command after Raft consensus
    pub async fn process_command(&self, command: VmCommand) -> BlixardResult<()> {
        #[cfg(feature = "observability")]
        let span = tracing::span!(
            tracing::Level::INFO,
            "vm.process_command",
            otel.name = "vm.process_command",
            otel.kind = ?opentelemetry::trace::SpanKind::Internal,
        );
        #[cfg(not(feature = "observability"))]
        let span = tracing::span!(tracing::Level::INFO, "vm.process_command",);
        let _enter = span.enter();

        #[cfg(feature = "observability")]
        let metrics = metrics();

        // All state persistence has already been handled by the RaftStateMachine
        // before this command was forwarded to us. We only execute the actual VM operation.
        match command {
            VmCommand::Create { config, node_id } => {
                // Temporarily disabled: tracing_otel uses tonic
                // tracing_otel::add_attributes(&[
                //     ("vm.name", &config.name),
                //     ("vm.operation", &"create"),
                //     ("node.id", &node_id),
                // ]);

                #[cfg(feature = "observability")]
                let _timer = Timer::with_attributes(
                    metrics.vm_create_duration.clone(),
                    vec![
                        attributes::vm_name(&config.name),
                        attributes::node_id(node_id),
                        attributes::operation("backend_create"),
                    ],
                );

                let result = self.backend.create_vm(&config, node_id).await;

                if result.is_ok() {
                    #[cfg(feature = "observability")]
                    metrics.vm_total.add(1, &[]);
                    // Monitor VM status after creation attempt
                    self.monitor_vm_status_after_operation(&config.name).await;
                } else {
                    #[cfg(feature = "observability")]
                    metrics
                        .vm_create_failed
                        .add(1, &[attributes::vm_name(&config.name)]);
                }

                result
            }
            VmCommand::Start { name } => {
                let result = self.backend.start_vm(&name).await;

                if result.is_ok() {
                    #[cfg(feature = "observability")]
                    metrics.vm_running.add(1, &[]);
                    // Monitor VM status after start attempt
                    self.monitor_vm_status_after_operation(&name).await;
                }

                result
            }
            VmCommand::Stop { name } => {
                let result = self.backend.stop_vm(&name).await;

                if result.is_ok() {
                    #[cfg(feature = "observability")]
                    metrics.vm_running.add(-1, &[]);
                    // Monitor VM status after stop attempt
                    self.monitor_vm_status_after_operation(&name).await;
                }

                result
            }
            VmCommand::Delete { name } => {
                let result = self.backend.delete_vm(&name).await;

                if result.is_ok() {
                    #[cfg(feature = "observability")]
                    metrics.vm_total.add(-1, &[]);
                }

                result
            }
            VmCommand::UpdateStatus { name, status } => {
                self.backend.update_vm_status(&name, status).await
            }
            VmCommand::Migrate { task } => {
                let node_id = self.node_state.get_id();

                // Handle migration based on whether we're source or target
                if task.source_node_id == node_id {
                    // We're the source node - initiate migration
                    tracing::info!(
                        "Starting migration of VM '{}' from node {} to node {}",
                        task.vm_name,
                        node_id,
                        task.target_node_id
                    );

                    // For now, we'll do a simple stop-and-start migration
                    // In the future, this could be enhanced with live migration
                    if !task.live_migration {
                        // Stop the VM on this node
                        self.backend.stop_vm(&task.vm_name).await?;
                    }

                    // The actual VM will be started on the target node
                    self.backend
                        .migrate_vm(&task.vm_name, task.target_node_id, task.live_migration)
                        .await
                } else if task.target_node_id == node_id {
                    // We're the target node - prepare to receive the VM
                    tracing::info!(
                        "Preparing to receive VM '{}' from node {} to node {}",
                        task.vm_name,
                        task.source_node_id,
                        node_id
                    );

                    // For now, just acknowledge - the VM will be started when we get a Start command
                    // In a real implementation, this would set up resources for the incoming VM
                    Ok(())
                } else {
                    // We're neither source nor target - shouldn't happen
                    tracing::warn!("Received migration command for VM '{}' but we're neither source ({}) nor target ({})", 
                        task.vm_name, task.source_node_id, task.target_node_id);
                    Ok(())
                }
            }
        }
    }

    /// List all VMs (reads from Raft-managed database for consistency)
    pub async fn list_vms(&self) -> BlixardResult<Vec<(VmConfig, VmStatus)>> {
        // Read from database instead of backend to ensure Raft consistency
        let read_txn = self.database.begin_read()?;

        if let Ok(table) = read_txn.open_table(crate::raft_storage::VM_STATE_TABLE) {
            // Pre-allocate with estimated capacity
            let mut result = Vec::with_capacity(16); // Reasonable default for most clusters

            for entry in table.iter()? {
                let (_key, value) = entry?;
                let vm_state: crate::types::VmState = bincode::deserialize(value.value())?;
                result.push((vm_state.config, vm_state.status));
            }

            Ok(result)
        } else {
            Ok(Vec::new())
        }
    }

    /// Get status of a specific VM (reads from Raft-managed database)
    pub async fn get_vm_status(&self, name: &str) -> BlixardResult<Option<(VmConfig, VmStatus)>> {
        // Read from database instead of backend to ensure Raft consistency
        let read_txn = self.database.begin_read()?;

        if let Ok(table) = read_txn.open_table(crate::raft_storage::VM_STATE_TABLE) {
            if let Ok(Some(data)) = table.get(name) {
                let vm_state: crate::types::VmState = bincode::deserialize(data.value())?;
                Ok(Some((vm_state.config, vm_state.status)))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Get the IP address of a VM from the backend
    pub async fn get_vm_ip(&self, name: &str) -> BlixardResult<Option<String>> {
        self.backend.get_vm_ip(name).await
    }

    /// Monitor VM status after an operation and trigger status updates through Raft
    ///
    /// This method polls the actual VM status from the backend and compares it with
    /// the status stored in the database. If there's a difference, it triggers a
    /// status update through the Raft consensus mechanism.
    async fn monitor_vm_status_after_operation(&self, vm_name: &str) {
        // Spawn a background task to avoid blocking the main operation
        let backend = Arc::clone(&self.backend);
        let database = Arc::clone(&self.database);
        let node_state = Arc::clone(&self.node_state);
        let name = vm_name.to_string();

        tokio::spawn(async move {
            // Wait a moment for the VM operation to take effect
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            // Poll for status changes with timeout
            let mut attempts = 0;
            const MAX_ATTEMPTS: u32 = 20; // 10 seconds total (500ms * 20)

            while attempts < MAX_ATTEMPTS {
                // Get current status from backend (actual VM state)
                let actual_status = match backend.get_vm_status(&name).await {
                    Ok(Some(status)) => status,
                    Ok(None) => {
                        tracing::warn!(
                            "VM '{}' not found in backend during status monitoring",
                            name
                        );
                        return;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to get VM '{}' status from backend: {}", name, e);
                        return;
                    }
                };

                // Get stored status from database (distributed state)
                let stored_status = {
                    let read_txn = match database.begin_read() {
                        Ok(txn) => txn,
                        Err(e) => {
                            tracing::warn!(
                                "Failed to read database during status monitoring: {}",
                                e
                            );
                            return;
                        }
                    };

                    if let Ok(table) = read_txn.open_table(crate::raft_storage::VM_STATE_TABLE) {
                        if let Ok(Some(data)) = table.get(name.as_str()) {
                            match bincode::deserialize::<crate::types::VmState>(data.value()) {
                                Ok(vm_state) => Some(vm_state.status),
                                Err(e) => {
                                    tracing::warn!(
                                        "Failed to deserialize VM state during monitoring: {}",
                                        e
                                    );
                                    return;
                                }
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };

                // Check if status has changed and needs updating
                if let Some(stored) = stored_status {
                    if actual_status != stored {
                        tracing::info!(
                            "VM '{}' status changed: {:?} -> {:?} (triggering Raft update)",
                            name,
                            stored,
                            actual_status
                        );

                        // Trigger Raft status update
                        match node_state
                            .update_vm_status_through_raft(&name, format!("{:?}", actual_status))
                            .await
                        {
                            Ok(_) => {
                                tracing::info!(
                                    "Successfully triggered status update for VM '{}' to {:?}",
                                    name,
                                    actual_status
                                );
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to trigger status update for VM '{}': {}",
                                    name,
                                    e
                                );
                            }
                        }
                        return;
                    } else if matches!(
                        actual_status,
                        VmStatus::Running | VmStatus::Stopped | VmStatus::Failed
                    ) {
                        // VM has reached a stable state, stop monitoring
                        tracing::debug!(
                            "VM '{}' status monitoring complete: {:?}",
                            name,
                            actual_status
                        );
                        return;
                    }
                }

                attempts += 1;
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }

            tracing::warn!(
                "VM '{}' status monitoring timed out after {} attempts",
                name,
                MAX_ATTEMPTS
            );
        });
    }

    /// Schedule VM placement using the intelligent scheduler
    ///
    /// This method uses the VM scheduler to determine the best node for VM placement
    /// based on resource requirements and placement strategy. The actual VM creation
    /// still goes through Raft consensus for distributed coordination.
    pub async fn schedule_vm_placement(
        &self,
        vm_config: &VmConfig,
        strategy: PlacementStrategy,
    ) -> BlixardResult<PlacementDecision> {
        let scheduler = VmScheduler::new(Arc::clone(&self.database));
        scheduler.schedule_vm_placement(vm_config, strategy).await
    }

    /// Get cluster-wide resource summary
    ///
    /// This provides visibility into cluster resource utilization for
    /// management and monitoring purposes.
    pub async fn get_cluster_resource_summary(
        &self,
    ) -> BlixardResult<crate::vm_scheduler::ClusterResourceSummary> {
        let scheduler = VmScheduler::new(Arc::clone(&self.database));
        scheduler.get_cluster_resource_summary().await
    }

    /// Create a VM with automatic placement using default strategy
    ///
    /// This is a convenience method that creates a VM using the default
    /// MostAvailable placement strategy.
    pub async fn create_vm(&self, vm_config: VmConfig) -> BlixardResult<PlacementDecision> {
        self.create_vm_with_scheduling(vm_config, PlacementStrategy::MostAvailable).await
    }

    /// Create a VM with automatic placement
    ///
    /// This is a convenience method that combines scheduling and VM creation.
    /// It automatically selects the best node using the specified strategy,
    /// then creates the VM through the normal Raft consensus process.
    pub async fn create_vm_with_scheduling(
        &self,
        vm_config: VmConfig,
        strategy: PlacementStrategy,
    ) -> BlixardResult<PlacementDecision> {
        // Schedule placement
        let placement = self.schedule_vm_placement(&vm_config, strategy).await?;

        tracing::info!(
            "Scheduled VM '{}' for placement on node {}: {}",
            vm_config.name,
            placement.target_node_id,
            placement.reason
        );

        // Propose VM creation through Raft consensus
        let vm_command = VmCommand::Create {
            config: vm_config,
            node_id: placement.target_node_id,
        };

        // Submit the command through Raft for distributed consensus
        let proposal_data = crate::raft_manager::ProposalData::CreateVm(vm_command);
        let proposal = crate::raft_manager::RaftProposal {
            id: uuid::Uuid::new_v4().as_bytes().to_vec(),
            data: proposal_data,
            response_tx: None, // Fire-and-forget for scheduling
        };

        self.node_state.send_raft_proposal(proposal).await?;

        Ok(placement)
    }

    /// Recover VMs from persisted state after node restart
    ///
    /// This method is called during node initialization to restore VMs that
    /// were running before the node was restarted. It reads the persisted
    /// VM state from the database and attempts to restart VMs that were
    /// previously running.
    pub async fn recover_persisted_vms(&self) -> BlixardResult<()> {
        use crate::vm_state_persistence::{VmPersistenceConfig, VmStatePersistence};

        tracing::info!("Starting VM recovery from persisted state");

        // Create persistence manager with default config
        let persistence_config = VmPersistenceConfig {
            auto_recovery_enabled: true,
            skip_failed_vms: true,
            max_parallel_recovery: 3,
            recovery_delay_secs: 2,
        };

        let persistence = VmStatePersistence::new(self.database.clone(), persistence_config);

        // Call the recovery method with the Arc<dyn VmBackend>
        let recovery_report = persistence.recover_vms(self.backend.clone()).await?;

        tracing::info!(
            "VM recovery completed - Total: {}, Recovered: {}, Failed: {}, Skipped: {}",
            recovery_report.total_vms,
            recovery_report.recovered_vms.len(),
            recovery_report.failed_recoveries.len(),
            recovery_report.skipped_vms.len()
        );

        if !recovery_report.failed_recoveries.is_empty() {
            tracing::warn!(
                "Some VMs failed to recover: {:?}",
                recovery_report.failed_recoveries
            );
        }

        Ok(())
    }
}

/// Mock VM backend for testing
///
/// This implementation logs operations without actually managing VMs,
/// useful for testing the distributed systems logic without VM dependencies.
pub struct MockVmBackend {
    database: Arc<Database>,
    // Track simulated VM states for realistic status transitions
    simulated_states:
        std::sync::RwLock<std::collections::HashMap<String, (VmStatus, std::time::Instant)>>,
}

impl MockVmBackend {
    pub fn new(database: Arc<Database>) -> Self {
        Self {
            database,
            simulated_states: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }
}

#[async_trait]
impl VmBackend for MockVmBackend {
    async fn create_vm(&self, config: &VmConfig, node_id: u64) -> BlixardResult<()> {
        #[cfg(feature = "failpoints")]
        fail_point!("vm::create");

        tracing::info!("Mock: Creating VM '{}' on node {}", config.name, node_id);

        // Simulate VM creation with status transition
        // Start with "Creating", will transition to "Running" after a short delay
        {
            let mut states = self
                .simulated_states
                .write()
                .map_err(|_| BlixardError::lock_poisoned_internal("Mock VM backend"))?;
            states.insert(
                config.name.clone(),
                (crate::types::VmStatus::Creating, std::time::Instant::now()),
            );
        }

        Ok(())
    }

    async fn start_vm(&self, name: &str) -> BlixardResult<()> {
        #[cfg(feature = "failpoints")]
        fail_point!("vm::start");

        tracing::info!("Mock: Starting VM '{}'", name);

        // Simulate VM start with status transition to Running
        {
            let mut states = self
                .simulated_states
                .write()
                .map_err(|_| BlixardError::lock_poisoned_internal("Mock VM backend"))?;
            states.insert(
                name.to_string(),
                (crate::types::VmStatus::Running, std::time::Instant::now()),
            );
        }

        Ok(())
    }

    async fn stop_vm(&self, name: &str) -> BlixardResult<()> {
        #[cfg(feature = "failpoints")]
        fail_point!("vm::stop");

        tracing::info!("Mock: Stopping VM '{}'", name);

        // Simulate VM stop with status transition to Stopped
        {
            let mut states = self
                .simulated_states
                .write()
                .map_err(|_| BlixardError::lock_poisoned_internal("Mock VM backend"))?;
            states.insert(
                name.to_string(),
                (crate::types::VmStatus::Stopped, std::time::Instant::now()),
            );
        }

        Ok(())
    }

    async fn delete_vm(&self, name: &str) -> BlixardResult<()> {
        tracing::info!("Mock: Deleting VM '{}'", name);
        Ok(())
    }

    async fn update_vm_status(&self, name: &str, status: VmStatus) -> BlixardResult<()> {
        tracing::info!("Mock: Updating VM '{}' status to {:?}", name, status);
        Ok(())
    }

    async fn get_vm_status(&self, name: &str) -> BlixardResult<Option<VmStatus>> {
        // Check simulated states for realistic status transitions
        {
            let states = self
                .simulated_states
                .read()
                .map_err(|_| BlixardError::lock_poisoned_internal("Mock VM backend"))?;
            if let Some((status, created_at)) = states.get(name) {
                let elapsed = created_at.elapsed();

                // Simulate realistic status transitions based on time
                let current_status = match status {
                    crate::types::VmStatus::Creating => {
                        // After 1 second, transition from Creating to Running
                        if elapsed.as_secs() >= 1 {
                            crate::types::VmStatus::Running
                        } else {
                            *status
                        }
                    }
                    _ => *status, // Running, Stopped, etc. stay as they are
                };

                tracing::debug!(
                    "Mock: VM '{}' status: {:?} (elapsed: {:?})",
                    name,
                    current_status,
                    elapsed
                );
                return Ok(Some(current_status));
            }
        }

        // Fallback to database if no simulated state (for existing VMs)
        let read_txn = self.database.begin_read()?;

        if let Ok(table) = read_txn.open_table(crate::raft_storage::VM_STATE_TABLE) {
            if let Ok(Some(data)) = table.get(name) {
                let vm_state: crate::types::VmState = bincode::deserialize(data.value())?;
                Ok(Some(vm_state.status))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    async fn list_vms(&self) -> BlixardResult<Vec<(VmConfig, VmStatus)>> {
        // Read from Raft-managed database for consistency
        let read_txn = self.database.begin_read()?;
        let mut result = Vec::new();

        if let Ok(table) = read_txn.open_table(crate::raft_storage::VM_STATE_TABLE) {
            for entry in table.iter()? {
                let (_key, value) = entry?;
                let vm_state: crate::types::VmState = bincode::deserialize(value.value())?;
                result.push((vm_state.config, vm_state.status));
            }
        }

        Ok(result)
    }
}

/// Factory trait for creating VM backends
///
/// This trait enables the plugin architecture for VM backends.
/// Different implementations (microvm.nix, Docker, Firecracker, etc.)
/// can register factories to create their specific backend instances.
pub trait VmBackendFactory: Send + Sync {
    /// Create a new VM backend instance
    fn create_backend(
        &self,
        config_dir: PathBuf,
        data_dir: PathBuf,
        database: Arc<Database>,
    ) -> BlixardResult<Arc<dyn VmBackend>>;

    /// Get the name of this backend type
    fn backend_type(&self) -> &'static str;

    /// Get a description of this backend
    fn description(&self) -> &'static str;
}

/// Registry for VM backend factories
///
/// This provides a centralized way to register and discover VM backends.
/// Backends register themselves at startup, and the Node can create
/// the appropriate backend based on configuration.
#[derive(Clone)]
pub struct VmBackendRegistry {
    factories: HashMap<String, Arc<dyn VmBackendFactory>>,
}

impl VmBackendRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    /// Register a backend factory
    pub fn register(&mut self, factory: Arc<dyn VmBackendFactory>) {
        let backend_type = factory.backend_type().to_string();
        tracing::info!(
            "Registering VM backend: {} ({})",
            backend_type,
            factory.description()
        );
        self.factories.insert(backend_type, factory);
    }

    /// Create a backend of the specified type
    pub fn create_backend(
        &self,
        backend_type: &str,
        config_dir: PathBuf,
        data_dir: PathBuf,
        database: Arc<Database>,
    ) -> BlixardResult<Arc<dyn VmBackend>> {
        let factory = self.factories.get(backend_type).ok_or_else(|| {
            BlixardError::ConfigError(format!(
                "Unknown VM backend type: '{}'. Available backends: {:?}",
                backend_type,
                self.list_available_backends()
            ))
        })?;

        factory.create_backend(config_dir, data_dir, database)
    }

    /// List all available backend types
    pub fn list_available_backends(&self) -> Vec<&str> {
        self.factories.keys().map(|s| s.as_str()).collect()
    }

    /// Get information about all registered backends
    pub fn get_backend_info(&self) -> Vec<(String, String)> {
        self.factories
            .values()
            .map(|f| (f.backend_type().to_string(), f.description().to_string()))
            .collect()
    }
}

impl Default for VmBackendRegistry {
    fn default() -> Self {
        let mut registry = Self::new();

        // Register the built-in mock backend
        registry.register(Arc::new(MockVmBackendFactory));

        // NOTE: Additional backends (like microvm) must be registered by the main binary
        // to avoid circular dependencies

        registry
    }
}

/// Factory for the mock VM backend (built-in for testing)
pub struct MockVmBackendFactory;

impl VmBackendFactory for MockVmBackendFactory {
    fn create_backend(
        &self,
        _config_dir: PathBuf,
        _data_dir: PathBuf,
        database: Arc<Database>,
    ) -> BlixardResult<Arc<dyn VmBackend>> {
        Ok(Arc::new(MockVmBackend::new(database)))
    }

    fn backend_type(&self) -> &'static str {
        "mock"
    }

    fn description(&self) -> &'static str {
        "Mock VM backend for testing (no actual VMs created)"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_vm_backend_registry() {
        let mut registry = VmBackendRegistry::new();

        // Should start empty (no default mock in new())
        assert!(registry.list_available_backends().is_empty());

        // Register mock backend
        registry.register(Arc::new(MockVmBackendFactory));
        assert_eq!(registry.list_available_backends(), vec!["mock"]);

        // Test default() includes mock
        let default_registry = VmBackendRegistry::default();
        assert!(default_registry.list_available_backends().contains(&"mock"));
    }

    #[tokio::test]
    async fn test_mock_backend_creation() {
        let registry = VmBackendRegistry::default();
        let temp_dir = TempDir::new().expect("Failed to create temporary directory for test");
        let db_path = temp_dir.path().join("test.db");
        let database = Arc::new(redb::Database::create(db_path).expect("Failed to create test database"));

        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");

        // Should successfully create mock backend
        let backend = registry
            .create_backend("mock", config_dir, data_dir, database)
            .unwrap();

        // Test basic operations
        let vm_config = crate::types::VmConfig {
            name: "test-vm".to_string(),
            config_path: "/tmp/test.nix".to_string(),
            vcpus: 1,
            memory: 512,
            tenant_id: "default".to_string(),
            ip_address: None,
            metadata: None,
            anti_affinity: None,
            ..Default::default()
        };

        backend.create_vm(&vm_config, 1).await.unwrap();
        backend.start_vm("test-vm").await.unwrap();
        backend.stop_vm("test-vm").await.unwrap();
        backend.delete_vm("test-vm").await.unwrap();
    }

    #[test]
    fn test_unknown_backend_error() {
        let registry = VmBackendRegistry::default();
        let temp_dir = TempDir::new().expect("Failed to create temporary directory for test");
        let db_path = temp_dir.path().join("test.db");
        let database = Arc::new(redb::Database::create(db_path).expect("Failed to create test database"));

        let result = registry.create_backend("unknown", PathBuf::new(), PathBuf::new(), database);

        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.to_string().contains("Unknown VM backend type"));
    }
}
