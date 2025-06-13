# Cluster Formation Debugging Summary

## Update: Join Request Implementation (Partial Fix)

The cluster formation is now partially working after implementing join requests:

### Progress Made
1. **Fixed TCP Listener Conflict**: Removed duplicate TCP listener in node.start() that was conflicting with gRPC
2. **Fixed CLI Parsing**: `--peers` flag now properly sets `join_addr` in NodeConfig  
3. **Added Join Request Call**: Nodes now call `send_join_request()` during initialization

### Current Status
- ✅ Node 2 successfully joins the cluster
- ✅ Node 1 accepts node 2 and updates configuration (voters: [1, 2])
- ❌ Node 3's join request times out after 5 seconds

### Remaining Issue
When node 3 tries to join, the configuration change times out. This appears to be because:
1. Node 2 is receiving heartbeats but not fully participating in consensus
2. Node 2 starts with empty configuration (voters: {}) and doesn't update it
3. Without node 2's vote, the configuration change for node 3 can't reach majority in a 2-node cluster

### Next Steps
1. Ensure node 2 receives and applies the configuration change via log replication
2. Check why node 2 has empty voter configuration after joining
3. Verify AppendEntries messages carry configuration changes properly

---

## Previous Fix: Applied Index Management ✅

The initial cluster formation issue was resolved. There were two root causes:

1. **Applied Index Management**: `advance_apply()` was automatically advancing the applied index to match the committed index, preventing any entries from appearing in `committed_entries()`.
2. **Node Bootstrap**: Nodes with a `join_addr` were still bootstrapping themselves as single-node clusters, creating split-brain scenarios.

### Solution Summary

1. **Storage Initialization**: Initialize storage with a ConfState for single-node clusters before creating RawNode
2. **Manual Applied Index Tracking**: Use `advance_apply_to()` with explicit index tracking instead of `advance_apply()`
3. **Fast-Path for Single Node**: For single-node clusters, manually apply conf changes after verifying they're committed
4. **Debug Logging**: Added extensive logging to track Raft state transitions

### Key Fixes Applied

1. **src/storage.rs**: Added `initialize_single_node()` method to pre-populate ConfState
2. **src/node.rs**: 
   - Initialize storage before creating RaftManager for bootstrap scenarios  
   - Added check to prevent storage initialization when `join_addr` is specified
3. **src/raft_manager.rs**:
   - Changed `advance_apply()` to `advance_apply_to(last_applied_index)`
   - Added fast-path for single-node conf changes
   - Added check to prevent double-application of conf changes
   - Enhanced debug logging throughout
   - Added check for `join_addr` to prevent bootstrap when joining existing cluster
4. **src/test_helpers.rs**: 
   - Added workaround to pre-populate peer list for nodes joining a cluster
   - This helps the Raft layer know it's not alone from the start

### Final Solution

The complete fix involved:

1. **Preventing Split-Brain**: Nodes with a `join_addr` no longer bootstrap themselves as single-node clusters
2. **Peer Pre-Population**: In test helpers, we pre-populate the peer list for joining nodes
3. **Proper Log Replication**: With the above fixes, node 1 properly sends MsgAppend messages to replicate the configuration

The tests now pass consistently. The cluster formation process works as follows:
1. Node 1 bootstraps as a single-node cluster leader
2. Node 2 starts knowing about node 1 (via the pre-populated peer)
3. Node 1 adds node 2 to the configuration via Raft
4. Node 1 replicates the configuration change to node 2
5. Both nodes converge to see the full cluster membership

### Future Improvements

While the current solution works, a more robust implementation could:
1. **Implement learner nodes**: Use ConfChangeV2 to add nodes as non-voting members first
2. **Dynamic peer discovery**: Allow nodes to discover peers without pre-configuration
3. **Snapshot support**: Implement InstallSnapshot for faster synchronization of new nodes

---

## Original Investigation

### Problem Statement

When attempting to form a multi-node cluster, the configuration change (adding a new node) times out after 5 seconds. The root cause appears to be that Raft is not committing entries in a single-node cluster.

### Symptoms
- `committed_entries` count remains 0 even after proposing configuration changes
- Configuration changes are proposed successfully but never committed
- The Raft log shows `committed: 2, applied: 1` but no entries appear in `committed_entries()`
- Join cluster operations fail with "Configuration change timed out"

## Investigation Timeline

### 1. Initial Analysis
- **Finding**: The test `test_two_node_cluster_formation` was failing with a panic during shutdown
- **Action**: Identified that the actual issue was the join operation timing out, not the shutdown

### 2. Raft Message Serialization Fix
- **Issue**: Custom binary serialization was being used instead of protobuf
- **Fix**: Updated `raft_codec.rs` to use proper protobuf methods:
  ```rust
  // Before
  Message::new_()  // Incorrect method name
  
  // After  
  Message::new()
  msg.write_to_bytes()
  msg.merge_from_bytes()
  ```
- **Result**: Compilation fixed, but cluster formation still times out

### 3. Configuration Change Response Handling
- **Issue**: Configuration changes were "fire and forget"
- **Fix**: Updated `node_shared.rs` to wait for configuration change completion:
  ```rust
  // Now waits for response with 5-second timeout
  match tokio::time::timeout(Duration::from_secs(5), response_rx).await {
      Ok(Ok(result)) => result,
      Ok(Err(_)) => Err("Configuration change response channel closed"),
      Err(_) => Err("Configuration change timed out"),
  }
  ```
- **Result**: Now properly reports timeout, but doesn't fix the underlying issue

### 4. Ready State Processing Investigation
- **Finding**: Raft tracks `committed` and `applied` indices separately
- **Observation**: After proposing conf change, we see:
  - `committed: 2, applied: 1` 
  - But `committed_entries()` returns empty vector
- **Attempted fixes**:
  
  a. **Process ready after every event**:
  ```rust
  // Added after each select branch
  loop {
      if !self.on_ready().await? {
          break;
      }
  }
  ```
  
  b. **Tick when unapplied entries exist**:
  ```rust
  if committed > applied {
      node.tick();
      continue;
  }
  ```
  **Result**: Created infinite loop - tick doesn't produce ready with committed entries
  
  c. **Manual entry application**:
  ```rust
  // Attempted to manually fetch and apply entries
  if node.raft.raft_log.committed > node.raft.raft_log.applied {
      let entries = node.mut_store().entries(start, end, std::u64::MAX)?;
      // Apply entries manually...
  }
  ```
  **Result**: No `set_applied` method available in storage trait

### 5. Bootstrap Process Improvements
- **Theory**: Need to process ready immediately after bootstrap
- **Changes**:
  ```rust
  pub async fn bootstrap_single_node(&self) -> BlixardResult<()> {
      // ... bootstrap code ...
      
      // Process ready immediately
      self.on_ready().await?;
      
      // Propose empty entry to establish leadership
      if node.raft.state == StateRole::Leader {
          node.propose(vec![], vec![])?;
      }
      
      // Process ready again
      self.on_ready().await?;
  }
  ```
- **Result**: No improvement - still 0 committed entries

### 6. Logging Enhancements
Added extensive logging to trace the flow:
- Ready state details (entries, committed_entries, messages)
- Raft state after processing (term, commit, applied, last_index)
- Configuration change flow
- Entry commitment tracking

**Key findings from logs**:
```
[RAFT-READY] Ready state details, snapshot: true, entries: 1, committed_entries: 0
[RAFT-READY] After processing - raft state, term: 1, commit: 2, applied: 1, last_index: 2
[RAFT-READY] Committed entries count: 0
```

## Current Understanding

1. **The Raft library is committing entries** (commit index advances from 1 to 2)
2. **But not providing them in `committed_entries()`** (always returns empty)
3. **This suggests a mismatch between our `applied` configuration and Raft's expectations**

### Raft Library Behavior
- The library only returns entries in `committed_entries()` that are > the configured `applied` index
- We initialize with `applied: 0` in the Config
- After the first ready cycle, the library might be updating its internal tracking

### Single-Node Specifics
- Single-node clusters should commit immediately without waiting for replication
- We're successfully becoming leader
- But configuration changes aren't being processed through the normal committed entries flow

## Remaining Theories

1. **Applied Index Tracking**: The Raft library might expect us to track applied index differently
2. **Initial Configuration**: The bootstrap process might not be setting up the initial configuration correctly
3. **Ready State Timing**: We might need to process ready at different points in the cycle
4. **Storage Implementation**: Our storage might not be properly supporting the Raft library's expectations

## Next Steps

1. Study the Raft library examples more carefully for single-node cluster patterns
2. Check if we need to initialize with a different `applied` value
3. Investigate if there's a special API for single-node clusters
4. Consider looking at how other projects (TiKV, etcd) handle this scenario
5. Debug the Raft library internals to understand when `committed_entries()` returns values

## Code References

- Main issue: `src/raft_manager.rs:620-627` - Empty committed_entries
- Configuration change: `src/raft_manager.rs:517-572` - handle_conf_change
- Bootstrap: `src/raft_manager.rs:409-449` - bootstrap_single_node
- Ready processing: `src/raft_manager.rs:575-725` - on_ready

## External References

### Raft-rs Documentation
- **Official Docs**: https://docs.rs/raft/latest/raft/
- Key sections to review:
  - `RawNode` - The main interface we're using
  - `Storage` trait - Understanding storage expectations
  - `Ready` struct - Understanding when committed_entries are available
  - Configuration changes - Proper patterns for membership changes

### Raft-rs Examples
- **GitHub Examples**: https://github.com/tikv/raft-rs/tree/master/examples
- Particularly relevant examples:
  - `single_mem_node` - Single node in-memory example
  - `five_mem_node` - Multi-node cluster example
  - Configuration change handling patterns
  - Ready state processing loops

### Key Patterns to Study from Examples

1. **Single Node Bootstrap** (from single_mem_node):
   - How they initialize the node
   - How they handle the first ready state
   - When committed_entries become available

2. **Ready State Processing** (from five_mem_node):
   - The exact loop structure for processing ready
   - How they track applied index
   - When to call `advance()`

3. **Configuration Changes**:
   - How examples handle `propose_conf_change`
   - The relationship between proposing and committing
   - Single-node vs multi-node differences

These examples should provide the canonical patterns for using the raft-rs library correctly.