# VM Implementation Plan

This document tracks the pending implementation work for the VM management functionality in Blixard.

## Current Status

All VM operations currently return `NotImplemented` errors. The infrastructure for managing VMs through Raft consensus is in place, but the actual microVM integration is pending.

## Implementation Tasks

### 1. MicroVM.nix Integration (Priority: High)
**File**: `src/vm_manager.rs`

- [ ] Line 115-117: Interface with microvm.nix to create VM
  - Research microvm.nix API/CLI interface
  - Implement VM creation with proper configuration
  - Handle VM image management
  
- [ ] Line 121-125: Interface with microvm.nix to start VM
  - Implement VM startup sequence
  - Handle network configuration
  - Monitor startup success/failure
  
- [ ] Line 127-131: Interface with microvm.nix to stop VM
  - Implement graceful shutdown
  - Handle force stop scenarios
  - Clean up resources
  
- [ ] Line 134-139: Interface with microvm.nix to delete VM
  - Remove VM configuration
  - Clean up storage/images
  - Update state tracking

### 2. VM State Tracking (Priority: High)
- [ ] Implement VM status monitoring
- [ ] Track resource usage per VM
- [ ] Handle VM migration between nodes

### 3. Network Management (Priority: Medium)
- [ ] Configure VM networking
- [ ] Implement isolation between VMs
- [ ] Handle inter-VM communication

### 4. Storage Management (Priority: Medium)
- [ ] VM image storage and distribution
- [ ] Persistent volume management
- [ ] Snapshot functionality

### 5. Resource Scheduling (Priority: Medium)
- [ ] CPU/Memory allocation tracking
- [ ] VM placement optimization
- [ ] Resource overcommit policies

### 6. Monitoring and Metrics (Priority: Low)
**File**: `src/grpc_server.rs`
- [ ] Line 692: Track task execution time
- [ ] Implement VM performance metrics
- [ ] Resource usage reporting

## Technical Considerations

### MicroVM.nix Integration Options

1. **CLI Wrapper Approach**
   - Use `tokio::process::Command` to call microvm.nix CLI
   - Parse JSON output for status/configuration
   - Simple but may have performance overhead

2. **Direct Nix Integration**
   - Use Nix evaluation to manage VM configurations
   - More complex but potentially more powerful
   - Better for declarative VM management

3. **Systemd Integration**
   - Use systemd units for VM lifecycle
   - Leverage systemd's process management
   - Good for production deployments

### Error Handling

All VM operations should handle:
- Resource exhaustion (CPU, memory, disk)
- Network failures
- VM startup failures
- Image corruption/missing images
- Node failures during VM operations

### Testing Strategy

1. Unit tests with mock microvm.nix responses
2. Integration tests with test VMs
3. Chaos testing for failure scenarios
4. Performance benchmarks for VM operations

## Dependencies

- microvm.nix installation and configuration
- Nix package manager
- KVM/QEMU for virtualization
- Bridge networking setup
- Storage backend (local or distributed)

## References

- [microvm.nix documentation](https://github.com/astro/microvm.nix)
- [Firecracker MicroVM](https://firecracker-microvm.github.io/)
- [Cloud Hypervisor](https://www.cloudhypervisor.org/)

## Next Steps

1. Set up development environment with microvm.nix
2. Create proof-of-concept for VM creation
3. Design VM configuration schema
4. Implement basic create/start/stop operations
5. Add comprehensive error handling
6. Write integration tests