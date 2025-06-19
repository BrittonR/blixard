# Raft Consensus Enforcement Fix

## Problem
Multiple components were bypassing Raft consensus and writing directly to the database, which could cause split-brain scenarios and data inconsistency across the cluster.

## Root Causes
1. **VM Manager**: Maintained a local cache (`vm_states` HashMap) and wrote directly to database
2. **Worker Registration**: Non-bootstrap nodes were writing directly to database
3. **Lack of Documentation**: No clear guidance on what state must go through Raft

## Solution

### Phase 1: VM Manager Transformation
- Removed `vm_states` HashMap cache entirely
- Deleted `load_from_database()` method
- Deleted `persist_vm_state()` method
- Transformed `process_command()` to only execute VM operations
- VM Manager is now a stateless executor

### Phase 2: Worker Registration Fix
- Added `register_worker_through_raft()` method to SharedNodeState
- Updated worker registration to use Raft proposals for multi-node clusters
- Preserved direct DB writes only during bootstrap (necessary for initialization)

### Phase 3: Documentation
- Added comprehensive state management documentation to `node_shared.rs`
- Documented the distinction between distributed state (via Raft) and local state
- Added detailed documentation to VmManager about its stateless design
- Added inline comments explaining bootstrap exceptions

## Key Design Principles

### Distributed State (Must use Raft)
- VM configurations and status
- Worker registrations and capabilities
- Task assignments
- Cluster configuration changes

### Local State (Managed locally)
- Peer addresses for message routing
- Active gRPC connections
- Runtime handles and channels
- Cached Raft status information

### Bootstrap Exception
Direct database writes are allowed ONLY during single-node cluster bootstrap. After bootstrap completes, ALL state changes must go through Raft consensus.

## Testing
- `test_vm_read_after_write_consistency` - Passes ✅
- `test_three_node_cluster_manual_approach` - Passes ✅
- Created `worker_registration_tests.rs` for worker consistency testing

## Benefits
1. **Eliminated Split-Brain Risk**: All authoritative state now goes through Raft
2. **Simplified Architecture**: Removed unnecessary caching layer
3. **Better Documentation**: Future developers will understand the state management model
4. **Maintained Performance**: ReDB's memory-mapped files provide sufficient caching

## Files Changed
- `src/vm_manager.rs` - Removed cache and direct DB writes
- `src/node_shared.rs` - Added worker registration through Raft and documentation
- `src/node.rs` - Updated to remove VM loading and clarified bootstrap exceptions
- `tests/worker_registration_tests.rs` - New test file for worker consistency
- `plan.md` - Updated with implementation status
- `CLAUDE.md` - Updated with fix summary

## Issues Fixed

### Non-Leader Configuration State Updates (FIXED)
There was an issue where non-leader nodes didn't properly update their configuration state after a RemoveNode operation. This happened because:

1. Non-leader nodes have an incomplete view of the Raft configuration
2. When they try to apply RemoveNode, they get a "removed all voters" error
3. The error handling was skipping saving the updated configuration state

**Fix implemented**:
1. Non-leaders now detect when they have an empty Raft configuration and update their state directly from the log entry
2. The gRPC server's `get_cluster_status` was fixed to return membership from the authoritative Raft configuration instead of the local peers list
3. `SharedNodeState::get_cluster_status` now uses `get_current_voters()` to read from Raft storage

The `test_node_failure_handling` now passes reliably.