# Test Reliability Issues

This document tracks known test reliability issues and flakiness in the Blixard test suite.

## Status Summary

- **Three-node clusters**: ❌ BROKEN (tests disabled)
- **Two-node clusters**: ⚠️ FLAKY (requires workarounds)
- **Single-node**: ✅ RELIABLE
- **Overall reliability**: ~70% (based on validation script runs)

## Critical Issues

### 1. Three-Node Cluster Formation (HIGH PRIORITY)
- **Status**: Tests disabled with `#[ignore]`
- **Issue**: Log replication timing causes nodes to not recognize each other
- **Impact**: Cannot test multi-node scenarios properly
- **Files**: 
  - `tests/cluster_formation_tests.rs`
  - `tests/cluster_integration_tests.rs`
- **Related**: `THREE_NODE_CLUSTER_FIX.md` (attempted fix didn't fully resolve)

### 2. Hardcoded Sleep Delays (HIGH PRIORITY)
- **Count**: 25+ instances across test files
- **Issue**: Fixed delays don't account for system load or CI environments
- **Impact**: Tests fail under load or in CI
- **Solution**: Use `test_helpers::timing` utilities instead

### 3. Raft Log Replication Workarounds (MEDIUM PRIORITY)
- **Issue**: Nodes don't automatically replicate logs after configuration changes
- **Workaround**: Send dummy health checks to trigger replication
- **Location**: `tests/cluster_formation_tests.rs:106-108`
- **Impact**: Tests are fragile and may fail if timing changes

### 4. Peer Connection Retry Logic (MEDIUM PRIORITY)
- **Issue**: `BufferedMessage` struct has unused fields (compiler warnings)
- **Location**: `src/peer_connector.rs:15-19`
- **Impact**: Messages may be lost during connection establishment

## Test Flakiness Patterns

### Race Conditions
1. **Cluster membership propagation**
   - Nodes join but don't immediately see each other
   - Configuration changes take unpredictable time
   
2. **Leader election timing**
   - Tests assume leader election completes quickly
   - No proper waiting for stable leader

3. **State synchronization**
   - Raft log and state machine can be out of sync
   - Nodes report different cluster states temporarily

### Environmental Factors
- CI environments need 3x timeout multiplier
- Resource-constrained systems cause timing variations
- Network latency affects gRPC connections

## Improvements Made

### ✅ Enhanced Test Helpers (`src/test_helpers.rs`)
- `TestCluster` abstraction for multi-node scenarios
- Automatic port allocation to avoid conflicts
- Built-in retry logic for client connections
- Environment-aware timeout scaling
- Condition-based waiting utilities

### ✅ Timing Utilities (`tests/common/test_timing.rs`)
- Exponential backoff for retries
- CI-aware timeout multipliers
- Robust waiting functions

## Remaining Work

1. **Fix three-node cluster formation**
   - Root cause: Raft message delivery timing
   - Need immediate message delivery for protocol messages

2. **Replace all hardcoded sleeps**
   - Migrate to `timing::wait_for_condition_with_backoff()`
   - Use `timing::robust_sleep()` where delays are necessary

3. **Remove workarounds**
   - Fix underlying Raft implementation issues
   - Ensure automatic log replication after config changes

4. **Improve validation**
   - Current validation script runs tests 10x to check reliability
   - Goal: 100% pass rate on first run

## Testing Guidelines

### DO:
- Use `test_helpers::TestNode` and `TestCluster` for node management
- Use `timing::wait_for_condition_with_backoff()` for waiting
- Use automatic port allocation with `.with_auto_port()`
- Add proper error messages to assertions
- Clean up resources with `.shutdown()`

### DON'T:
- Use hardcoded `sleep()` calls
- Assume fixed timing for operations
- Test three-node clusters until fixed
- Ignore compiler warnings about unused code

## Known Workarounds

### Two-Node Cluster Formation
```rust
// WORKAROUND: Node 2 needs to receive log entries
// Send a health check through node 1 to trigger log replication
let _ = client1.health_check(HealthCheckRequest {}).await;
```

### Cluster Convergence Waiting
```rust
// Wait with extended timeout and check both nodes
wait_for_condition(
    || async { /* check both nodes see each other */ },
    Duration::from_secs(10),  // Long timeout
    Duration::from_millis(100),
)
```

## Validation

Run the validation script to check current reliability:
```bash
./scripts/validate-cluster-formation.sh
```

Expected output for reliable tests:
```
Run 1/10: ✓ PASSED
Run 2/10: ✓ PASSED
...
Run 10/10: ✓ PASSED

Success rate: 100% (10/10)
```

Current typical output:
```
Success rate: 70% (7/10)
```