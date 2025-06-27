# TUI VM Migration Feature

## Overview

The Blixard TUI now supports VM migration between nodes, allowing operators to move virtual machines from one node to another with minimal disruption. This feature is essential for load balancing, maintenance operations, and resource optimization.

## Features

### 1. Migration Types
- **Cold Migration**: VM is stopped, moved, and restarted on the target node
- **Live Migration**: VM continues running during migration (future enhancement)
- **Force Migration**: Override resource checks (future enhancement)

### 2. Migration Dialog
- Clear source and target node display
- Available nodes list for reference
- Live migration toggle (when VM is running)
- Input validation for target node

### 3. Safety Checks
- Prevents migration to the same node
- Validates target node exists in cluster
- Checks VM status for live migration eligibility
- Resource availability verification (future)

## Usage

### Initiating a Migration

1. Navigate to the VM list (`Tab` → VMs)
2. Select the VM you want to migrate using arrow keys
3. Press `m` to open the migration dialog
4. Enter the target node ID
5. Toggle live migration if desired (Space/Enter)
6. Press `Enter` to submit the migration

### Migration Dialog Controls
- `Tab`: Switch between fields
- `0-9`: Enter target node ID
- `Space`/`Enter`: Toggle live migration
- `Backspace`: Delete characters
- `Enter`: Submit migration
- `Esc`: Cancel and return to VM list

### Visual Indicators
- Current node ID is displayed
- Available nodes are listed for reference
- Selected field is highlighted
- Live migration checkbox shows current state

## Implementation Details

### Protocol
The migration uses a new gRPC endpoint:
```protobuf
rpc MigrateVm(MigrateVmRequest) returns (MigrateVmResponse);

message MigrateVmRequest {
  string vm_name = 1;
  uint64 target_node_id = 2;
  bool live_migration = 3;
  bool force = 4;
}
```

### Migration Process
1. **Validation**: Check VM exists and target node is valid
2. **Raft Consensus**: Submit migration command through Raft
3. **Source Node**: Stop VM (if cold migration)
4. **State Update**: Update VM's node assignment in distributed state
5. **Target Node**: Prepare resources and start VM
6. **Completion**: Update status and notify success

### Current Implementation
- Basic cold migration (stop-move-start)
- Distributed state consistency via Raft
- Automatic VM list refresh after migration
- Error handling and user feedback

## Future Enhancements

### 1. Live Migration
- Memory page transfer while VM runs
- Minimal downtime (sub-second)
- Network connection preservation
- Storage migration options

### 2. Resource Validation
- Check target node capacity
- Prevent overcommitment
- Resource reservation during migration
- Rollback on failure

### 3. Migration Progress
- Real-time progress indicator
- Transfer speed metrics
- Estimated completion time
- Cancel operation support

### 4. Batch Migration
- Migrate multiple VMs at once
- Dependency-aware ordering
- Load balancing automation
- Maintenance mode support

### 5. Advanced Features
- Cross-datacenter migration
- Storage migration options
- Network reconfiguration
- Pre/post migration hooks

## Benefits

1. **Zero Downtime Maintenance**: Move VMs away from nodes requiring updates
2. **Load Balancing**: Redistribute VMs for optimal resource utilization
3. **Hardware Lifecycle**: Migrate VMs off aging hardware
4. **Performance Optimization**: Move VMs closer to their workloads
5. **Disaster Recovery**: Quick evacuation of failing nodes

## Troubleshooting

### Common Issues

1. **"Target node must be different from source node"**
   - You're trying to migrate to the same node
   - Select a different target node

2. **"Node X not found in cluster"**
   - The target node ID doesn't exist
   - Check available nodes in the list

3. **"VM must be running for live migration"**
   - Live migration requires the VM to be in Running state
   - Either start the VM first or disable live migration

4. **"Failed to migrate VM"**
   - Network or permission issues
   - Check cluster connectivity and logs

### Best Practices

1. **Plan Migrations**: Schedule during low-usage periods
2. **Test First**: Try migrations in non-production environments
3. **Monitor Resources**: Ensure target has sufficient capacity
4. **Verify Success**: Check VM status after migration
5. **Document Changes**: Keep track of VM locations