# Blixard Development Plan

## Current State Summary

### ✅ What's Working Well:
1. **Raft Consensus**: Basic three-node cluster formation works reliably
2. **ReDB Integration**: Database persistence and locking issues have been resolved
3. **Snapshot Support**: Raft snapshots are implemented for state transfer
4. **Peer Management**: Dynamic peer connections with automatic reconnection
5. **Configuration Changes**: Nodes can join/leave clusters successfully

### ❌ What's Not Working:
1. **Task/Worker System**: Completely unimplemented - all task operations panic with "Not implemented"
2. **Test Reliability**: 25+ hardcoded `sleep()` calls causing flaky tests
3. **Membership Edge Cases**: Node ID conflicts in some test scenarios
4. **Raft Proposal Pipeline**: Task proposals hang indefinitely waiting for responses
5. **Worker Tables**: Not initialized in database, causing scheduling failures

## Recommended Plan

### Phase 1: Stabilize Current Foundation (Priority: HIGH)
1. **Fix Test Infrastructure**
   - Replace all 25+ hardcoded `sleep()` calls with `wait_for_condition()` using the existing timing utilities
   - Fix the `wait_for_condition` function that currently always returns `true`
   - Add proper state verification in tests instead of time-based waiting
   - Create `TEST_RELIABILITY_ISSUES.md` to track ongoing issues

2. **Fix Membership Management Bug**
   - Investigate why Node 4 is reported as already existing in membership change tests
   - Ensure proper cleanup between test phases
   - Add validation to prevent duplicate node IDs

3. **Complete Raft Proposal Pipeline**
   - Fix the hanging task submission proposals
   - Ensure proposals get proper responses
   - Add timeout handling for proposals

### Phase 2: Implement Core Missing Features (Priority: HIGH)
1. **Worker Registration System**
   - Create worker tables in database initialization
   - Implement worker registration on node startup
   - Add worker health monitoring with heartbeats
   - Track worker capacity and availability

2. **Task Execution Engine**
   - Implement `WorkerExecutor` background service
   - Create `TaskRunner` for actual process execution
   - Add resource monitoring and enforcement
   - Implement task completion flow through Raft

3. **Task Scheduling Logic**
   - Complete the `schedule_task` function
   - Implement worker selection algorithm
   - Handle task assignment and reassignment
   - Add failure handling and retries

### Phase 3: Production Readiness (Priority: MEDIUM)
1. **Enhanced Testing**
   - Add integration tests for full task lifecycle
   - Create chaos tests for network partitions
   - Add performance benchmarks
   - Implement deterministic simulation tests with MadSim

2. **Observability**
   - Add comprehensive logging for debugging
   - Implement metrics collection
   - Create health check endpoints
   - Add distributed tracing support

3. **Error Handling**
   - Review all `todo!()` and `unimplemented!()` calls
   - Add proper error recovery mechanisms
   - Implement graceful degradation
   - Add circuit breakers for external dependencies

### Phase 4: VM Integration (Priority: LOW - After Core is Stable)
1. **MicroVM Integration**
   - Integrate with microvm.nix
   - Implement VM lifecycle management
   - Add resource allocation for VMs
   - Create VM health monitoring

2. **Advanced Features**
   - Log compaction using snapshot infrastructure
   - Dynamic cluster reconfiguration
   - Multi-region support
   - Advanced scheduling policies

## Immediate Next Steps (Do These First)

1. **Fix the test infrastructure** - This will give us confidence that our fixes actually work
2. **Implement worker registration** - This unblocks all task-related functionality
3. **Complete the Raft proposal pipeline** - This fixes the hanging tests
4. **Add proper database table initialization** - Ensures worker tables exist

## Success Metrics

- All tests pass reliably (no flaky tests)
- Task submission and execution works end-to-end
- Three-node clusters can handle node failures gracefully
- No hardcoded sleeps in test code
- Clear documentation of remaining limitations

## Technical Details

### Worker Registration Implementation
- Add `WORKER_TABLE` and `WORKER_STATUS_TABLE` creation in `Storage::new()`
- Register worker on node startup with capacity and capabilities
- Background task for periodic health updates
- Clean up stale workers on timeout

### Task Execution Implementation
- Create `src/task_executor.rs` for task execution logic
- Use tokio::process for command execution
- Capture stdout/stderr and exit codes
- Submit results through Raft consensus
- Handle task timeouts and resource limits

### Test Infrastructure Fixes
- Update `tests/common/test_timing.rs` to properly check conditions
- Create helper functions for common test scenarios
- Add deterministic waiting for Raft state changes
- Remove all direct `tokio::time::sleep` calls

### Database Schema Updates
```rust
// Worker table schema
struct Worker {
    node_id: u64,
    capacity: u32,
    available_capacity: u32,
    last_heartbeat: u64,
    status: WorkerStatus,
}

// Worker status table schema  
struct WorkerStatus {
    node_id: u64,
    cpu_usage: f32,
    memory_usage: f32,
    task_count: u32,
}
```