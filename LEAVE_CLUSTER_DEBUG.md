# Leave Cluster Configuration Change Debug

## Summary of Debugging Enhancements

I've added detailed logging to help debug why configuration changes are failing in the leave_cluster implementation. The changes focus on tracking the entire lifecycle of a configuration change from proposal to completion.

## Key Logging Points Added

### 1. In `grpc_server.rs` - `leave_cluster` method:
- Log when leave request is received
- Log current cluster state before attempting change
- Log whether node is leader
- Log peer information and addresses
- Log Raft status before and after configuration change
- Log exact error messages if proposal fails

### 2. In `node_shared.rs` - `propose_conf_change` method:
- Check if node is initialized
- Verify node is the leader (only leaders can propose conf changes)
- Log when sending to Raft manager channel
- Log detailed timeout information
- Log current Raft status on timeout

### 3. In `raft_manager.rs` - `handle_conf_change` method:
- Log current Raft state before handling
- Log current voters list
- Check leader status before proposing
- Log when storing response channel
- Log proposal ID for tracking

### 4. In `raft_manager.rs` - Configuration change processing:
- Log when EntryConfChange is processed
- Log parsing success/failure
- Log configuration state before and after applying
- Track pending proposals and match them with committed entries
- Log when sending response back to waiting channel

## What to Look For in Logs

When debugging leave_cluster failures, look for these patterns:

1. **Leader Check Failures**:
   ```
   [NODE-SHARED] Cannot propose conf change - not the leader. Current leader: <id>
   ```

2. **Timeout Issues**:
   ```
   [NODE-SHARED] Conf change timed out after 5 seconds
   ```

3. **Context Mismatch**:
   ```
   [RAFT-CONF] No pending conf change found for context
   ```

4. **Parse Failures**:
   ```
   [RAFT-CONF] Failed to parse ConfChange from entry data
   ```

## Next Steps

1. Run the leave_cluster test with these enhanced logs
2. Check if configuration changes are being proposed but not committed
3. Verify the context/proposal ID is being preserved through the Raft log
4. Check if the node receiving the leave request is actually the leader
5. Ensure the response channel isn't being dropped prematurely

## Common Issues to Check

1. **Not Leader**: Configuration changes can only be proposed by the leader
2. **Context Lost**: The Raft library might not preserve the context field for conf changes
3. **Timing**: The 5-second timeout might be too short for some scenarios
4. **Single vs Multi-Node**: Different code paths for single-node clusters

## Test Command

To test with detailed logging:
```bash
RUST_LOG=blixard=debug cargo test leave_cluster -- --nocapture
```