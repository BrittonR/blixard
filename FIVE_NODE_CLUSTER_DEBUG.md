# Five Node Cluster Formation Debug

## Problem Summary
The `test_split_brain_prevention` test fails during 5-node cluster formation. Nodes end up with incorrect voter configurations, preventing message acceptance between nodes.

## Observed Issues

### Issue 1: Configuration Corruption
- Node 2 ends up with voters: [4, 3] instead of [1, 2, 3, 4]
- Node 3 sometimes has voters: [4, 5] in 5-node tests
- This causes nodes to reject messages from node 1 (the leader)

### Issue 2: Raft's apply_conf_change Behavior
Research shows that Raft's `apply_conf_change` sometimes returns only the changed node in the voters list, not the complete configuration. For example:
- When adding node 2 to a cluster with node 1, it may return voters: [2] instead of [1, 2]
- This is inconsistent and depends on Raft's internal state

## Root Cause Analysis

After extensive debugging and research:

1. **Raft Library Behavior**: The `apply_conf_change` method doesn't always return the complete voter configuration. Sometimes it returns just the change.

2. **Configuration Merge Logic**: We added logic to detect when Raft returns only the new node and merge with existing configuration, but this isn't sufficient for all cases.

3. **Race Conditions**: When multiple nodes join rapidly, configuration changes may be processed out of order or incompletely.

4. **Snapshot Issues**: Configuration can be overwritten when applying snapshots that contain incomplete or outdated voter lists.

## Attempted Fixes

1. **Configuration Merging** (Partially Successful):
   - Added logic to detect when `apply_conf_change` returns only the new node
   - Merges with existing configuration from storage
   - Works for 3-node clusters but fails for larger clusters

2. **Sequential Joins with Delays**:
   - Large clusters (>3 nodes) use sequential joins
   - Added delays and health checks between joins
   - Added verification that all nodes see consistent configuration
   - Still experiencing race conditions

## Current Theory

The configuration corruption ([4, 3] for node 2) suggests that:
1. Node 2 is processing configuration changes out of order
2. OR Node 2 is receiving a snapshot with incorrect configuration
3. OR There's a bug in how we reconstruct configuration for non-leaders

## Next Steps

1. **Verify Raft Behavior**: Create isolated test to verify what `apply_conf_change` returns in different scenarios
2. **Track Configuration Changes**: Add more logging to track every configuration change and its source
3. **Review Raft Examples**: The five_mem_node example simply saves whatever `apply_conf_change` returns without modification
4. **Consider Alternative Approach**: Instead of trying to be smart about configuration, trust Raft's returned values more directly

## References
- https://docs.rs/raft/latest/raft/
- https://github.com/tikv/raft-rs/tree/master/examples/five_mem_node