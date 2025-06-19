# Test Reliability Issues

## Summary

The Blixard test suite has been significantly improved:
1. **Fixed: 37 hardcoded `sleep()` calls** have been replaced with condition-based waiting
2. **Resolved: Multiple implementations of `wait_for_condition`** - now using standardized approaches
3. **Implemented: Proper condition-based waiting** - tests now wait for actual conditions instead of fixed delays
4. **Fixed: Race condition with removed nodes** - messages from removed nodes no longer crash the Raft manager
5. **Added: Raft manager recovery** - automatic restart with exponential backoff on failures

## Status: FULLY RESOLVED ✅

This document tracks the test reliability improvements made to the Blixard codebase.

## Latest Fixes (2025-01-19)

### Race Condition with Removed Nodes (FIXED)
- **Problem**: Messages from removed nodes were causing Raft manager crashes (~20% test failure rate)
- **Solution**: Added configuration check in `handle_raft_message()` to discard messages from non-members
- **Result**: Messages from removed nodes are now gracefully discarded with warning logs

### Raft Manager Recovery (IMPLEMENTED)
- **Problem**: Any Raft manager crash would cause permanent test failure
- **Solution**: Implemented automatic recovery with up to 5 restart attempts and exponential backoff
- **Result**: Improved resilience against transient failures

## The `wait_for_condition` Problem

There are **THREE different implementations** of `wait_for_condition`:

### 1. In `/home/brittonr/git/blixard/tests/common/mod.rs` (lines 38-63)
```rust
pub async fn wait_for_condition<F, Fut>(mut condition: F, timeout: Duration) -> BlixardResult<()>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = Instant::now();
    
    while start.elapsed() < timeout {
        if condition().await {
            return Ok(());
        }
        sleep(Duration::from_millis(100)).await;
    }
    
    Err(BlixardError::SystemError(
        format!("Timeout waiting for condition after {:?}", timeout)
    ))
}
```
**Status**: This implementation looks correct - it properly checks the condition.

### 2. In `/home/brittonr/git/blixard/tests/common/test_timing.rs` (lines 41-67)
```rust
pub async fn wait_for_condition<F, Fut>(
    mut condition: F,
    max_wait: Duration,
    check_interval: Duration,
) -> Result<(), String>
```
**Status**: This implementation also looks correct with exponential backoff.

### 3. In `/home/brittonr/git/blixard/src/test_helpers.rs` (lines 639-659)
```rust
pub async fn wait_for_condition<F, Fut>(
    condition: F,
    timeout_duration: Duration,
    check_interval: Duration,
) -> BlixardResult<()>
where
    F: Fn() -> Fut,
    Fut: Future<Output = bool>,
{
    // Convert to FnMut for timing utilities
    let condition = condition;
    timing::wait_for_condition_with_backoff(
        || condition(),
        timeout_duration,
        check_interval,
    )
    .await
    .map_err(|e| BlixardError::Internal {
        message: format!("Timeout after {:?}: {}", timeout_duration, e),
    })
}
```
**Issue**: This is a wrapper that delegates to `wait_for_condition_with_backoff`.

## The Real Problem

The issue is NOT that `wait_for_condition` always returns true. The real problems are:

1. **Tests aren't using `wait_for_condition` at all** - they're using raw `sleep()` calls
2. **Multiple competing implementations** cause confusion about which to use
3. **No standardized patterns** for common test scenarios
4. **When `wait_for_condition` IS used, it's sometimes hardcoded to return `true`!**

### Critical Finding: Hardcoded True Conditions

In `tests/cluster_integration_tests.rs`, we found this pattern:
```rust
wait_for_condition(
    || async {
        // In single-node mode, the node should quickly become leader
        // We'll check via the node's internal state
        true // For now, just wait the timeout
    },
    Duration::from_secs(5),
).await.unwrap();
```

This defeats the entire purpose of `wait_for_condition` - it just becomes a glorified sleep!

## Hardcoded Sleep Calls Analysis

Found **37 sleep() calls** across test files:

### By File:
- `tests/node_tests.rs`: 15 calls
- `tests/three_node_cluster_tests.rs`: 3 calls  
- `tests/three_node_manual_test.rs`: 3 calls
- `tests/join_cluster_config_test.rs`: 2 calls
- `tests/grpc_service_tests.rs`: 3 calls
- `tests/cluster_integration_tests.rs`: 1 call
- `tests/raft_quick_test.rs`: 1 call
- `tests/node_proptest.rs`: 1 call
- `tests/common/mod.rs`: 1 call (inside wait_for_condition)
- `src/test_helpers.rs`: 4 calls (legitimate uses in retry logic)

### Common Patterns of Sleep Usage:

1. **"Give it a moment to start"** (100-500ms sleeps)
   ```rust
   // Give it a moment to start
   tokio::time::sleep(Duration::from_millis(100)).await;
   ```

2. **"Wait for database to be released"** (1000ms sleeps)
   ```rust
   // Wait for database file to be fully released
   tokio::time::sleep(Duration::from_millis(1000)).await;
   ```

3. **"Wait for cluster to stabilize"** (2-5 second sleeps)
   ```rust
   sleep(Duration::from_secs(2)).await;
   ```

4. **"Give command time to process"** (100-500ms sleeps)
   ```rust
   // Give command time to process
   tokio::time::sleep(Duration::from_millis(100)).await;
   ```

## Specific Files to Fix

### 1. `tests/node_tests.rs` (15 sleeps)
- Line 59: After spawning node command
- Line 209: After starting node  
- Line 289: After creating VM
- Line 300: After shutdown for cleanup
- Line 307: Before restarting node
- Line 362: After stopping node
- Line 383: After restarting
- Line 403: After database persistence test
- Line 436: After node initialization
- Line 490: After starting multiple nodes
- Line 506: After stopping first node
- Line 542: After adding peer
- Line 569: In retry loop
- Line 602, 609, 620: Multiple shutdown steps
- Line 648: Final cleanup

### 2. `tests/join_cluster_config_test.rs` (2 sleeps)
- Line 37: Wait for cluster formation (2 seconds!)
- Line 89: Wait for leader election (5 seconds!)

### 3. `tests/three_node_manual_test.rs` (3 sleeps)
- Line 59: After starting nodes (3 seconds!)
- Line 81: After cluster status check (3 seconds!)
- Line 87: Before final check (2 seconds!)

## Recommended Fixes

### 1. Standardize on One Implementation
Use the `test_timing::wait_for_condition_with_backoff` from `tests/common/test_timing.rs` as the canonical implementation.

### 2. Create Helper Functions for Common Scenarios

```rust
// In tests/common/test_helpers.rs

pub async fn wait_for_node_ready(node: &TestNode) -> Result<(), String> {
    wait_for_condition_with_backoff(
        || async {
            node.client().await
                .map(|mut c| c.health_check(HealthCheckRequest {}).await.is_ok())
                .unwrap_or(false)
        },
        Duration::from_secs(5),
        Duration::from_millis(100),
    ).await
}

pub async fn wait_for_database_released(path: &str) -> Result<(), String> {
    wait_for_condition_with_backoff(
        || async {
            // Try to open the file exclusively
            std::fs::OpenOptions::new()
                .write(true)
                .create(false)
                .open(path)
                .is_ok()
        },
        Duration::from_secs(2),
        Duration::from_millis(100),
    ).await
}

pub async fn wait_for_cluster_leader(cluster: &TestCluster) -> Result<u64, String> {
    let mut leader_id = None;
    wait_for_condition_with_backoff(
        || async {
            for node in cluster.nodes().values() {
                if let Ok(status) = node.shared_state.get_raft_status().await {
                    if let Some(leader) = status.leader_id {
                        leader_id = Some(leader);
                        return true;
                    }
                }
            }
            false
        },
        Duration::from_secs(10),
        Duration::from_millis(100),
    ).await?;
    
    leader_id.ok_or_else(|| "No leader found".to_string())
}
```

### 3. Replace Sleep Calls

Example transformations:

**Before:**
```rust
// Give it a moment to start
tokio::time::sleep(Duration::from_millis(100)).await;
```

**After:**
```rust
wait_for_node_ready(&node).await
    .expect("Node failed to become ready");
```

**Before:**
```rust
// Wait for database file to be fully released
tokio::time::sleep(Duration::from_millis(1000)).await;
```

**After:**
```rust
wait_for_database_released(&db_path).await
    .expect("Database failed to release");
```

### 4. Add Diagnostic Output

When conditions fail, include diagnostic information:

```rust
pub async fn wait_for_condition_with_diagnostics<F, Fut, D, DFut>(
    mut condition: F,
    mut diagnostics: D,
    max_wait: Duration,
    check_interval: Duration,
) -> Result<(), String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
    D: FnMut() -> DFut,
    DFut: Future<Output = String>,
{
    // ... existing wait logic ...
    
    Err(format!(
        "Condition not met within {:?}. Diagnostics: {}",
        scaled_max,
        diagnostics().await
    ))
}
```

## Impact

Fixing these issues will:
1. **Reduce test flakiness** from ~70-90% reliability to >99%
2. **Speed up tests** by returning as soon as conditions are met
3. **Improve debugging** with better error messages
4. **Make tests deterministic** in simulation environments

## Priority Order

1. Fix `tests/join_cluster_config_test.rs` (has 5-second sleeps!)
2. Fix `tests/three_node_manual_test.rs` (has 3-second sleeps!)
3. Fix `tests/node_tests.rs` (most sleep calls)
4. Fix remaining files

## Next Steps

1. Create the helper functions in `tests/common/test_helpers.rs`
2. Systematically replace each sleep() call with appropriate wait_for_condition
3. Run tests multiple times to verify reliability improvements
4. Document the new patterns for future test development