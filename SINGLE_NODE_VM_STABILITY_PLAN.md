# Single-Node VM Orchestration Stability Plan

## Overview

This plan addresses critical stability issues for single-node microvm orchestration in Blixard. The primary goal is to ensure VMs survive node restarts and operate reliably in production environments.

## Critical Issues Identified

1. **No State Persistence** - VM configurations and state are lost on node restart
2. **No Bootstrap Recovery** - VMs don't automatically restart after node reboot
3. **Missing Resource Management** - No CPU/memory limits or overcommit protection
4. **Incomplete Process Monitoring** - Crashed VMs aren't detected or recovered
5. **Limited Health Checking** - Some health check types are stubs

## Implementation Phases

### Phase 1: VM State Persistence (CRITICAL - Implement First)

#### 1.1 Database Schema for VM State
```rust
// New tables in storage.rs
const VM_CONFIG_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("vm_configs");
const VM_STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("vm_states");
const VM_RESOURCE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("vm_resources");

// VM state to persist
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedVmState {
    pub config: VmConfig,
    pub status: VmStatus,
    pub last_health_check: Option<DateTime<Utc>>,
    pub restart_count: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

#### 1.2 MicrovmBackend Persistence Methods
- `persist_vm_state()` - Save VM configuration and status
- `load_persisted_vms()` - Load all VMs from database
- `update_vm_status()` - Update VM status in database
- `delete_vm_state()` - Remove VM from database

#### 1.3 Integration Points
- Modify `create_vm()` to persist configuration
- Modify `start_vm()` to update status
- Modify `stop_vm()` to update status
- Modify `delete_vm()` to remove from database

### Phase 2: Node Bootstrap and Recovery

#### 2.1 Bootstrap Manager
```rust
pub struct VmBootstrapManager {
    backend: Arc<MicrovmBackend>,
    recovery_policy: BootstrapPolicy,
}

pub enum BootstrapPolicy {
    RestartAll,           // Restart all VMs that were running
    RestartHealthy,       // Only restart VMs that were healthy
    Manual,               // Don't auto-restart, wait for operator
}
```

#### 2.2 Node Initialization
- Add `bootstrap_vms()` to Node initialization
- Load persisted VM state from database
- Restart VMs based on policy
- Verify network configuration (tap interfaces)

#### 2.3 State Reconciliation
- Compare persisted state with actual system state
- Clean up orphaned resources (tap interfaces, flake dirs)
- Report discrepancies in logs/metrics

### Phase 3: Resource Management

#### 3.1 Resource Tracker
```rust
pub struct ResourceTracker {
    // Node capacity
    total_cpus: u32,
    total_memory_mb: u32,
    total_disk_mb: u64,
    
    // Current allocations
    allocated_cpus: u32,
    allocated_memory_mb: u32,
    allocated_disk_mb: u64,
    
    // Per-VM reservations
    vm_reservations: HashMap<String, ResourceReservation>,
    
    // Overcommit ratios
    cpu_overcommit_ratio: f32,    // e.g., 4.0 for 4:1
    memory_overcommit_ratio: f32,  // e.g., 1.5 for 1.5:1
}

pub struct ResourceReservation {
    cpus: u32,
    memory_mb: u32,
    disk_mb: u64,
    reserved_at: DateTime<Utc>,
}
```

#### 3.2 Admission Control
- Check available resources before VM creation
- Enforce resource limits considering overcommit
- Persist resource reservations to database
- Release resources on VM deletion

#### 3.3 Resource Monitoring
- Track actual resource usage vs reserved
- Alert on resource exhaustion
- Implement resource reclamation for idle VMs

### Phase 4: Enhanced Process Monitoring

#### 4.1 Process Health Monitor
```rust
pub struct ProcessHealthMonitor {
    check_interval: Duration,
    process_checks: HashMap<String, ProcessCheck>,
}

pub struct ProcessCheck {
    vm_name: String,
    systemd_service: String,
    pid: Option<u32>,
    last_check: DateTime<Utc>,
    consecutive_failures: u32,
}
```

#### 4.2 Health Check Enhancements
- Implement console health check via serial console
- Add QEMU guest agent support for internal metrics
- Monitor systemd service status AND process existence
- Check for zombie processes

#### 4.3 Crash Detection and Recovery
- Detect VM process crashes via systemd and /proc
- Trigger auto-recovery based on policy
- Maintain crash history for analysis
- Implement crash dump collection

### Phase 5: Integration and Testing

#### 5.1 Integration with Existing Components
- Ensure health monitor uses persisted state
- Connect auto-recovery to bootstrap manager
- Update metrics with resource usage
- Add distributed tracing for debugging

#### 5.2 Comprehensive Testing
- Unit tests for each new component
- Integration tests for full lifecycle
- Chaos testing (kill processes, restart node)
- Performance testing with many VMs

#### 5.3 Documentation
- Operator guide for single-node deployment
- Troubleshooting guide for common issues
- Configuration reference
- Architecture decisions record (ADR)

## Implementation Order

1. **Week 1-2**: Phase 1 (State Persistence)
   - Critical for production use
   - Enables all other features
   - Relatively straightforward

2. **Week 3**: Phase 2 (Bootstrap/Recovery)
   - Depends on Phase 1
   - Essential for operational stability
   - Moderate complexity

3. **Week 4**: Phase 3 (Resource Management)
   - Prevents resource exhaustion
   - Important for multi-VM nodes
   - Can be basic initially

4. **Week 5**: Phase 4 (Process Monitoring)
   - Improves reliability
   - Catches edge cases
   - Can iterate on implementation

5. **Week 6**: Phase 5 (Integration/Testing)
   - Ensures production readiness
   - Documents for operators
   - Validates design decisions

## Success Criteria

1. VMs automatically restart after node reboot
2. VM state persists across node restarts
3. Resource limits prevent overcommit
4. Crashed VMs are detected and recovered
5. All VM operations work reliably
6. Comprehensive test coverage (>80%)
7. Production-ready documentation

## Risk Mitigation

1. **Data Loss** - Use write-ahead logging for state changes
2. **Resource Leaks** - Implement cleanup on all error paths
3. **Startup Storms** - Stagger VM restarts on bootstrap
4. **Network Issues** - Verify tap interfaces before VM start
5. **Disk Space** - Monitor flake directory growth

## Monitoring and Alerting

1. VM availability metrics
2. Resource utilization alerts
3. Bootstrap success/failure rates
4. Health check pass/fail ratios
5. Recovery attempt metrics

This plan provides a solid foundation for reliable single-node VM orchestration while maintaining compatibility with future multi-node features.