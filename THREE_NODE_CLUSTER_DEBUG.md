# Three-Node Cluster Configuration Issue Debug

## Current Status

✅ **Configuration Issue Fixed**: Configuration reconstruction logic added
✅ **Snapshot Implementation**: Full snapshot support implemented  
✅ **Join Issue Fixed**: Changed join_addr type from SocketAddr to String, fixed gRPC server startup order
❌ **Test Still Fails**: Node 3 has no leader - seems to not be receiving/processing Raft messages correctly

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

## Remaining Issues

1. **Cluster Convergence Timeout**: Test still fails with "Nodes failed to join cluster: Condition not met within 30s"
2. **Need to Debug**: What condition is not being met in wait_for_convergence?

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

## Next Steps

1. Check how to trigger snapshot sending in raft-rs
2. Implement InstallSnapshot message handling
3. Ensure new nodes receive current configuration state

## References
- https://docs.rs/raft/latest/raft/
- https://github.com/tikv/raft-rs/tree/master/examples