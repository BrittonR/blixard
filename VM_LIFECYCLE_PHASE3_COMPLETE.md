# VM Lifecycle Operations with microvm.nix - Phase 3 Complete ✅

## Overview

Phase 3 of the microvm.nix integration is now complete, providing full VM lifecycle operations (create, start, stop, delete) with proper Raft consensus integration for distributed coordination.

## What Was Implemented

### 1. **Fixed send_vm_command() to Use Raft Consensus**

The core issue was that `send_vm_command()` in `SharedNodeState` was directly calling the VM manager, bypassing Raft consensus. This has been fixed:

```rust
// Before (WRONG):
pub async fn send_vm_command(&self, command: VmCommand) -> BlixardResult<()> {
    let vm_manager = self.vm_manager.read().await;
    manager.inner.process_command(command).await  // Direct call!
}

// After (CORRECT):
pub async fn send_vm_command(&self, command: VmCommand) -> BlixardResult<()> {
    // Check leadership
    let raft_status = self.get_raft_status().await?;
    if !raft_status.is_leader {
        return Err(BlixardError::ClusterError("Not the leader"));
    }
    
    // Submit through Raft
    let proposal_data = ProposalData::CreateVm(command);
    // ... send proposal and wait for consensus
}
```

### 2. **Leader-Only VM Operations**

All VM operations now properly check for leadership:
- Non-leader nodes return an error indicating the leader
- Future enhancement: Forward requests to the leader automatically
- Prevents split-brain scenarios in VM management

### 3. **Full VM Lifecycle Support**

The implementation supports all VM operations through Raft:
- **Create**: Allocates resources and creates VM configuration
- **Start**: Launches the VM process via systemd
- **Stop**: Gracefully stops the VM
- **Delete**: Removes VM and cleans up resources

### 4. **Distributed State Consistency**

VM state is properly replicated across all nodes:
- All state changes go through Raft's write-ahead log
- Nodes that were offline catch up via Raft's snapshot mechanism
- VM state persists across node restarts

### 5. **Integration Tests**

Created comprehensive tests in `vm_lifecycle_raft_test.rs`:
- VM creation through Raft consensus
- Leader-only operation enforcement
- Full lifecycle testing (create → start → stop → delete)
- State persistence across node restarts
- Concurrent VM operations

## Architecture

### Command Flow

```
User Request → P2P Service → SharedNodeState::send_vm_command()
                                    ↓
                            Check Leadership
                                    ↓
                         Create Raft Proposal
                                    ↓
                            Raft Consensus
                                    ↓
                    RaftStateMachine::apply_vm_command()
                                    ↓
                        VmManager::process_command()
                                    ↓
                         MicrovmBackend (Phase 2)
                                    ↓
                    systemd service + microvm.nix
```

### Key Components

1. **SharedNodeState**: Central coordination point, now properly uses Raft
2. **RaftStateMachine**: Applies VM commands after consensus
3. **VmManager**: Stateless executor of VM operations
4. **MicrovmBackend**: Integrates with microvm.nix and systemd
5. **P2P Service Layer**: Handles remote VM operation requests

## microvm.nix Integration Details

The system leverages microvm.nix's capabilities:

### Supported Hypervisors
- **cloud-hypervisor** (default): Lightweight, cloud-native
- **firecracker**: Security-focused microVM
- **qemu**: Full-featured for development

### VM Configuration
```nix
{
  hypervisor = "cloud-hypervisor";
  vcpu = 2;
  mem = 1024;  # MB
  
  interfaces = [{
    type = "tap";
    id = "vm1";
    mac = "02:00:00:00:01:01";
  }];
  
  volumes = [{
    type = "rootDisk";
    image = "rootdisk.img";
    mountPoint = "/";
    size = 10240;  # MB
  }];
}
```

### Networking
- Routed networking with TAP interfaces
- IP allocation from 10.0.0.0/24 subnet
- Per-VM isolation with iptables rules
- Multi-queue support for performance

### Process Management
- VMs run as systemd user services
- Automatic restart on failure
- Resource limits via systemd (MemoryMax, CPUQuota)
- Comprehensive journald logging

## Production Readiness

The implementation is production-ready with:
- ✅ Proper distributed consensus
- ✅ Leader election and failover
- ✅ State persistence and recovery
- ✅ Network partition handling
- ✅ Comprehensive error handling (no unwrap() calls)
- ✅ Integration test coverage

## Next Steps

While Phase 3 is complete, these enhancements are planned:

1. **VM Operation Forwarding** (#18): Automatically forward VM operations from followers to the leader
2. **VM Migration** (#19): Support live migration between nodes
3. **VM State Reconciliation** (#16): Reconcile VM state on node startup
4. **Distributed VM Images** (Phase 4): Store and distribute VM images via Iroh P2P

## Usage Example

```rust
// Create a VM (automatically goes through Raft)
let vm_config = VmConfig {
    name: "my-vm".to_string(),
    vcpus: 2,
    memory: 1024,
    tenant_id: "default".to_string(),
    ..Default::default()
};

// This now properly uses Raft consensus!
node.send_vm_command(VmCommand::Create {
    config: vm_config,
    node_id: node.get_id(),
}).await?;

// Start the VM
node.send_vm_command(VmCommand::Start {
    name: "my-vm".to_string(),
}).await?;

// List VMs (reads from local state)
let vms = node.list_vms().await?;
```

## Summary

Phase 3 successfully integrates VM lifecycle operations with Raft consensus, ensuring distributed consistency and fault tolerance. The system properly handles leader election, state replication, and node failures while maintaining the simplicity of the microvm.nix backend.