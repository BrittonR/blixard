# Raft Voters Configuration Fix

## Problem
The code was using `node.raft.prs().votes()` to get the list of voter node IDs, but this method was returning an empty HashMap, causing configuration changes to fail with a "removed all voters" error.

## Investigation
1. The `prs().votes()` method from the raft-rs library appears to not maintain the voter list in the expected way
2. The raft-rs examples show that voter information should be tracked in the `ConfState` structure
3. Our code was already saving and loading `ConfState` to/from storage correctly

## Solution
Replace all occurrences of `node.raft.prs().votes()` with loading the voter list from the stored `ConfState`:

```rust
// Old code:
let voters: Vec<u64> = node.raft.prs().votes().keys().cloned().collect();

// New code:
let voters: Vec<u64> = match self.storage.load_conf_state() {
    Ok(conf_state) => conf_state.voters,
    Err(e) => {
        warn!(self.logger, "[RAFT-CONF] Failed to load conf state"; "error" => ?e);
        Vec::new()
    }
};
```

## Changes Made
Modified three locations in `src/raft_manager.rs`:
1. Line 772: Getting current voters when handling configuration changes
2. Line 1200: Getting voters before applying a configuration change from committed entries
3. Line 1281: Getting voters after applying a configuration change (using the already-loaded `corrected_cs`)

## Result
- The `test_join_cluster_configuration_update` test now passes successfully
- Nodes can properly join clusters without encountering the "removed all voters" error
- The voter list is correctly maintained through configuration changes

## Technical Details
The raft-rs library's `ProgressTracker` (accessed via `prs()`) doesn't expose voter information through a public API in the way we expected. Instead, the library expects applications to track voter information through the `ConfState` structure, which is returned from `apply_conf_change()` and should be persisted by the application.

This is consistent with the raft-rs design where the library provides the core Raft algorithm but expects the application to handle state persistence and configuration tracking.