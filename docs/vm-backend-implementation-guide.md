# VM Backend Implementation Guide

This guide provides comprehensive documentation for implementing VM backends in Blixard, extracted from the VmBackend trait documentation to improve code maintainability.

## Overview

The VmBackend trait defines the contract that all VM backends must implement, allowing the core distributed systems logic to be decoupled from specific VM implementations (microvm.nix, Docker, Firecracker, etc.)

## Design Principles

### 1. Async-First Architecture

All operations are async to handle potentially long-running VM operations without blocking the event loop. Implementations must be careful about:

- **Cancellation Safety**: Operations should handle task cancellation gracefully
- **Resource Cleanup**: Failed operations must clean up partial state
- **Timeout Handling**: Long operations should respect reasonable timeouts

### 2. Error Recovery

Operations may fail at any point due to:

- **Resource Exhaustion**: Out of memory, disk space, or CPU cores
- **Network Issues**: Unable to reach hypervisor or VM
- **Configuration Errors**: Invalid VM configuration or missing files
- **Hardware Failures**: Host system issues or hardware faults

### 3. Thread Safety

Implementations must be `Send + Sync` for multi-threaded access:

- **Concurrent Operations**: Multiple VMs can be managed simultaneously
- **Shared State**: Backend state must be protected with appropriate synchronization
- **No Blocking**: Must not perform blocking I/O in async contexts

### 4. Observability

All operations should provide comprehensive logging and metrics:

- **Structured Logging**: Use `tracing` for contextual log messages
- **Error Context**: Include detailed error information for debugging
- **Performance Metrics**: Track operation latency and resource usage

## Implementation Guidelines

### Resource Management

```rust
use blixard_core::{VmBackend, VmConfig, BlixardResult};
use async_trait::async_trait;

struct MyVmBackend {
    // Keep resource tracking state
    resource_tracker: Arc<ResourceTracker>,
}

#[async_trait]
impl VmBackend for MyVmBackend {
    async fn create_vm(&self, config: &VmConfig, node_id: u64) -> BlixardResult<()> {
        // 1. Validate configuration
        self.validate_vm_config(config).await?;
        
        // 2. Check resource availability
        self.resource_tracker.reserve_resources(config).await?;
        
        // 3. Create VM with rollback on failure
        match self.do_create_vm(config, node_id).await {
            Ok(()) => Ok(()),
            Err(e) => {
                // Clean up on failure
                self.resource_tracker.release_resources(config).await?;
                Err(e)
            }
        }
    }
}
```

### Error Handling Patterns

```rust
use blixard_core::{BlixardError, BlixardResult};

async fn start_vm_example(&self, name: &str) -> BlixardResult<()> {
    // Check if VM exists
    let vm_config = self.get_vm_config(name).await
        .map_err(|e| BlixardError::VmError(format!("VM '{}' not found: {}", name, e)))?;
    
    // Validate VM can be started
    if !self.can_start_vm(&vm_config).await? {
        return Err(BlixardError::VmError(
            format!("VM '{}' cannot be started (insufficient resources)", name)
        ));
    }
    
    // Start with timeout
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        self.hypervisor_start_vm(name)
    ).await
    .map_err(|_| BlixardError::Timeout {
        operation: format!("start VM '{}'", name),
        timeout_secs: 30,
    })?
}
```

### Health Check Implementation

```rust
use blixard_core::{HealthCheckType, HealthCheckResult};

async fn perform_health_check_example(
    &self,
    vm_name: &str,
    check_name: &str,
    check_type: &HealthCheckType,
) -> BlixardResult<HealthCheckResult> {
    match check_type {
        HealthCheckType::Tcp { host, port } => {
            // Try TCP connection with timeout
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                tokio::net::TcpStream::connect((host.as_str(), *port))
            ).await;
            
            match result {
                Ok(Ok(_)) => Ok(HealthCheckResult::healthy()),
                Ok(Err(e)) => Ok(HealthCheckResult::unhealthy(
                    format!("TCP connection failed: {}", e)
                )),
                Err(_) => Ok(HealthCheckResult::unhealthy("Health check timeout")),
            }
        },
        HealthCheckType::Http { url, expected_status } => {
            // Implement HTTP health check
            // ...
        },
        // Handle other check types
    }
}
```

## Common Failure Modes

### 1. Resource Exhaustion

- **Symptom**: create_vm() fails with resource errors
- **Recovery**: Implement resource admission control and queueing
- **Monitoring**: Track resource utilization metrics

### 2. Network Connectivity

- **Symptom**: Operations timeout or fail with network errors
- **Recovery**: Implement retry logic with exponential backoff
- **Monitoring**: Track operation success/failure rates

### 3. Hypervisor Issues

- **Symptom**: VMs fail to start or respond to commands
- **Recovery**: Restart hypervisor service or failover to other nodes
- **Monitoring**: Track hypervisor health and VM success rates

### 4. Configuration Errors

- **Symptom**: VMs fail to create with validation errors
- **Recovery**: Validate configuration before submission
- **Monitoring**: Track configuration validation failure rates

## Performance Considerations

- **Parallel Operations**: Support concurrent VM operations where possible
- **Resource Caching**: Cache frequently accessed VM metadata
- **Lazy Loading**: Only load VM state when needed
- **Connection Pooling**: Reuse connections to hypervisor APIs
- **Batch Operations**: Group similar operations for efficiency

## Security Considerations

- **Input Validation**: Sanitize all configuration inputs
- **Resource Isolation**: Ensure VMs cannot access unauthorized resources
- **Credential Management**: Secure storage of hypervisor credentials
- **Network Security**: Proper firewall and network segmentation
- **Audit Logging**: Log all VM lifecycle events for security analysis

## See Also

- [VmBackend trait definition](../blixard-core/src/vm_backend.rs) - The actual trait interface
- [microvm.nix backend implementation](../blixard-vm/) - Reference implementation using microvm.nix
- [VM Health Monitoring](./vm-health-monitoring.md) - Health check system documentation
- [Resource Management](./resource-management.md) - Resource allocation and tracking