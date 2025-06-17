# Three-Node Cluster Configuration Issue Debug

## Current Status

✅ **Configuration Issue Fixed**: Configuration reconstruction logic added
✅ **Snapshot Implementation**: Full snapshot support implemented  
✅ **Join Issue Fixed**: Changed join_addr type from SocketAddr to String, fixed gRPC server startup order
✅ **Panic Fixed**: Resolved "not leader but has new msg after advance" by properly handling LightReady messages
✅ **Leader Detection Fixed**: SharedNodeState now properly updated with Raft status
✅ **Three-Node Cluster Working**: Basic three-node cluster test now passes

## Problem Summary

The three-node cluster test is failing because:
1. Nodes 2 and 3 are not successfully joining the cluster
2. The join request appears to be sent but the configuration change is not being applied
3. Nodes 2 and 3 remain with 0 peers while expecting 3 peers

## Root Cause Analysis

1. **Entry Processing Order**:
   - Entry 3: AddNode(2) - committed before Node 2 joins
   - Entry 5: AddNode(3) - committed after Node 2 joins
   - Node 2 only processes entries starting from its join point (entry 4+)
   - Node 2 never sees or applies entry 3 (its own addition)

2. **Raft Library Behavior**:
   - `apply_conf_change` returns the result of applying that specific change
   - It doesn't return the full cluster configuration
   - When Node 2 applies AddNode(3), it starts from empty config and adds Node 3
   - Result: Node 2's configuration only contains Node 3

## Attempted Solutions

1. **Remove "already_applied" check** ✅ - Fixed double-application issue
2. **Add voters to JoinResponse** ❌ - Timing issue, config applied after join
3. **Update configuration on join** ❌ - Race condition with Raft processing

## Implemented Solution

Added configuration reconstruction logic in `raft_manager.rs`:
- When applying AddNode configuration changes
- If the returned configuration only contains the new node
- Load previous configuration and add the new node to it
- This ensures nodes maintain complete cluster configuration

## Fixed Issues

### 1. Raft Panic: "not leader but has new msg after advance"
- **Root Cause**: Messages were being generated during ready state processing after advance()
- **Fix**: 
  - Handle messages from LightReady after advance()
  - Remove send_append() call during configuration change processing
  - Move check_and_send_snapshots() to after ready processing

### 2. Leader Detection Failure
- **Root Cause**: SharedNodeState wasn't being updated with current Raft status
- **Fix**: Added update_raft_status() call after each ready state processing

## Implemented Solutions

### ✅ Snapshot Support (COMPLETED)
- Implemented full snapshot creation and restoration
- Added SnapshotData structure with all state machine tables
- Implemented snapshot() method in Storage trait
- Added check_and_send_snapshots() to detect lagging nodes (matched=0)
- InstallSnapshot messages are logged and handled
- Snapshots sent to new nodes automatically

### Current Issue: Join Request Not Processing

The snapshot implementation is complete, but the test is failing earlier - nodes are not successfully joining the cluster. The issue appears to be:
1. Node 2 and 3 send join requests to Node 1
2. Node 1 receives the request but the configuration change is not being applied/committed
3. Nodes 2 and 3 never see the expected 3 peers, causing timeout

## Current Implementation

### ✅ Completed:
1. **Snapshot Support** - Full implementation including:
   - `SnapshotData` structure with all state machine tables
   - `create_snapshot_data()` and `restore_from_snapshot()` methods
   - Automatic snapshot sending to lagging nodes (matched=0)
   - InstallSnapshot message handling and logging

2. **Configuration Reconstruction** - Workaround for raft-rs behavior:
   - When applying AddNode changes, detect incomplete configurations
   - Reconstruct full voter list by loading previous state and merging

3. **Type Fixes**:
   - Changed `join_addr` from `Option<SocketAddr>` to `Option<String>`
   - Fixed gRPC server startup order in test_helpers

### ✅ All Core Issues Resolved:
- Three-node cluster formation now works correctly
- All nodes properly recognize the leader
- Configuration changes are properly replicated to all nodes
- Basic three-node cluster test passes reliably

## Remaining Work

The following tests still fail due to missing task/worker functionality (not cluster formation issues):
- `test_three_node_cluster_task_submission` - Needs task/worker implementation
- `test_three_node_cluster_fault_tolerance` - Needs proper node removal implementation
- `test_three_node_cluster_concurrent_operations` - Needs task/worker implementation
- `test_three_node_cluster_membership_changes` - Needs proper node removal implementation

## References
- https://docs.rs/raft/latest/raft/
- https://github.com/tikv/raft-rs/tree/master/examples
- Commit: e2945f9 - feat(raft): implement snapshot support and fix configuration reconstruction