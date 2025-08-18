# Restart Candidate Bug Fix

## Problem Description

When nodes restart and rejoin the cluster using `--join-ticket`, they become **candidates** instead of properly joining as **followers**. This disrupts cluster stability by triggering unnecessary elections.

### Root Cause Analysis

1. **Persistent State Issue**: Restarting nodes load their previous Raft state from storage
2. **Voter Status Preserved**: The loaded state includes their node ID in the `voters` list
3. **Candidacy Triggered**: Raft sees the node as a voter → eligible to become candidate → starts elections
4. **Cluster Disruption**: Unnecessary elections (terms 36, 37) disrupt the stable leader

### Key Problem Areas

- `initialize_joining_node()` in `raft_storage.rs` returned early if state existed
- Join process didn't reset voter status for restarting nodes
- No distinction between fresh join vs restart scenario

## The Fix

### 1. Storage State Reset (`raft_storage.rs`)

**Before**: 
```rust
pub fn initialize_joining_node(&self) -> BlixardResult<()> {
    if let Ok(Some(_)) = table.get("conf_state") {
        return Ok(()); // ❌ Returns early, keeps old state
    }
    // Reset logic never reached for restarting nodes
}
```

**After**:
```rust
pub fn initialize_joining_node(&self) -> BlixardResult<()> {
    // ✅ ALWAYS reset state for joining nodes
    tracing::info!("[STORAGE] Initializing joining node - resetting Raft state to follower mode");
    
    let conf_state = raft::prelude::ConfState::default(); // Empty voters
    let hard_state = raft::prelude::HardState {
        term: 0, vote: 0, commit: 0  // Clean slate
    };
    
    // Always overwrite previous state
    self.save_conf_state(&conf_state)?;
    self.save_hard_state(&hard_state)?;
}
```

### 2. Initialization Flow Fix (`initialization.rs`)

**Before**:
```rust
} else {
    tracing::info!("Preparing to join existing cluster");
    // ❌ No state reset called
}
```

**After**:
```rust
} else {
    tracing::info!("Preparing to join existing cluster - resetting Raft state");
    storage.initialize_joining_node()?;  // ✅ Always reset
}
```

### 3. Enhanced Join Process (`cluster.rs`)

- Added voter status checking in `update_cluster_configuration()`
- Clear logging to distinguish voter vs non-voter status
- Safe application of configuration without triggering candidacy

### 4. Safety Checks (`raft/core.rs`)

- `apply_initial_conf_state()` checks if node is actually a voter
- `bootstrap_single_node()` prevents bootstrap when `join_addr` is set
- Enhanced logging for debugging join behavior

## Verification

### Test Script: `test-restart-fix.sh`

1. **Start 3-node cluster**: Node 1 (bootstrap) + Nodes 2,3 (join)
2. **Restart Node 3**: Kill and restart with same join ticket
3. **Verify behavior**: Node 3 should join as follower, not candidate
4. **Check logs**: Look for state reset and join processing messages

### Expected Behavior After Fix

```bash
✅ Node 3 restarted as follower (bug fixed!)
✅ Found expected storage reset message
✅ Found expected join processing messages  
✅ All nodes still running - cluster is stable
```

### Key Log Messages

- `[STORAGE] Initializing joining node - resetting Raft state to follower mode`
- `[JOIN-CONFIG] Our node 3 is NOT a voter in this configuration`
- `[RAFT-JOIN] Node 3 will remain follower until explicitly added as voter`

## Implementation Summary

This fix ensures that:

1. **Restarting nodes always reset to follower state** when using `--join-ticket`
2. **No accidental candidacy** - only nodes explicitly in voters list can become candidates
3. **Cluster stability preserved** - no unnecessary elections from restarting nodes
4. **Clear join semantics** - join means "become follower until added as voter"

The fix is **backward compatible** and **safe** - it only affects the join path and makes behavior more predictable.

## Files Modified

- `blixard-core/src/raft_storage.rs` - State reset logic
- `blixard-core/src/node/initialization.rs` - Always call state reset  
- `blixard-core/src/node/cluster.rs` - Enhanced join configuration
- `blixard-core/src/raft/core.rs` - Safety checks and voter validation

## Testing

Run the test:
```bash
./test-restart-fix.sh
```

This comprehensively tests the restart scenario and verifies the fix works correctly.