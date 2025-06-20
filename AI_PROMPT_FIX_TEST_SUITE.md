# AI Assistant Prompt: Fix Blixard Test Suite

## Context
You are tasked with fixing a distributed systems test suite (Blixard) that has significant quality issues. A comprehensive audit found that ~40% of tests are "hollow" - they pass without actually testing anything meaningful. Your goal is to transform these tests into robust validations of distributed system correctness.

## ACTUAL STATE DISCOVERED
**Initial assessment was partially inaccurate:**
- **Total tests**: 190+ (99.5% pass rate, but misleading due to hollow tests)
- **Comprehensive tests found**: `grpc_mock_consensus_tests.rs` (114 assertions), `network_partition_storage_tests.rs` (825 lines with 30 assertions)
- **Actual hollow tests**: `raft_quick_test.rs` and parts of `storage_edge_case_tests.rs`
- **Sleep calls**: 32+ hardcoded sleep() calls instead of proper synchronization
- **Missing coverage**: No Byzantine failure tests, clock skew tests, or chaos testing

## Your Mission

### Phase 1: Fix Hollow Tests ✅ COMPLETED
**CORRECTION**: `simulation/tests/grpc_mock_consensus_tests.rs` has 114 assertions - NOT hollow.

**Actual hollow tests found and fixed:**
- `tests/raft_quick_test.rs`: **FIXED** - now verifies Raft leader election instead of just completion
- `tests/storage_edge_case_tests.rs`: **FIXED** - added assertions for memory pressure and input validation

For each test:
1. ✅ Identified what the test name claims to verify
2. ✅ Added proper assertions that actually verify this behavior
3. ✅ Used proper APIs and meaningful verification
4. ✅ Replaced sleep() calls with condition-based waiting

Example transformation:
```rust
// BEFORE: No assertions, just sleeps
async fn test_three_node_leader_election() {
    create_nodes();
    sleep(2 seconds);
    // Test ends!
}

// AFTER: Verify leader election actually happens
async fn test_three_node_leader_election() {
    let cluster = TestCluster::new(3).await;
    let leader_id = wait_for_leader(&cluster).await.expect("Should elect leader");
    assert_eq!(count_leaders(&cluster), 1, "Exactly one leader");
    assert!(all_nodes_agree_on_leader(&cluster, leader_id));
}
```

### Phase 2: Replace Placeholder Tests ✅ INVESTIGATED
**CORRECTION**: `tests/network_partition_storage_tests.rs` is NOT a placeholder file.
- **825 lines** of comprehensive network partition testing code
- **30 assertions** verifying partition behavior
- Real implementation of partition creation, split-brain prevention, and healing
- Tests minority/majority partition behavior and data reconciliation

This phase was not needed - the tests are already comprehensive.

### Phase 3: Eliminate Sleep Calls 🔄 STARTED
Found and catalogued 32+ sleep() calls across multiple test files:
1. ✅ **Semantic wait functions already exist** in `tests/common/test_timing.rs`:
   - `wait_for_condition_with_backoff()` ✅ Available
   - `timing::robust_sleep()` - Used in some places
2. ✅ **Started replacement**: `tests/three_node_cluster_tests.rs` - replaced 1 sleep() call with condition-based waiting
3. 🔄 **Remaining work**: 30+ sleep() calls across:
   - `three_node_cluster_tests.rs`: 10+ calls
   - `storage_performance_benchmarks.rs`: 5+ calls  
   - `node_lifecycle_integration_tests.rs`: 4+ calls
   - `cli_cluster_commands_test.rs`: 2+ calls
   - Various other files: 10+ calls

### Phase 4: Add Missing Critical Tests 📋 IDENTIFIED
Create new test files for scenarios currently not covered:
1. **Byzantine failures** (`tests/byzantine_failure_tests.rs`) - **NOT YET IMPLEMENTED**:
   - Nodes claiming false leadership
   - Corrupted log entries
   - Selective message dropping
2. **Clock skew** (`tests/time_synchronization_tests.rs`) - **NOT YET IMPLEMENTED**:
   - Elections with skewed clocks
   - Lease expiration with time drift
3. **Complex networks** (`tests/advanced_network_tests.rs`) - **NOT YET IMPLEMENTED**:
   - Asymmetric partitions (can send but not receive)
   - Rolling network failures
   - Cascading failures

**Note**: These would be valuable additions for production confidence.

### Phase 5: Fix Property Tests 📋 IDENTIFIED
Review PropTest files and strengthen weak properties:
1. `error_proptest.rs` - **NEEDS REVIEW** - May test trivial properties
2. `proptest_example.rs` - **NEEDS REVIEW** - May test that positive numbers are positive
3. **Future additions needed**:
   - After any sequence of operations, committed state is consistent
   - No two leaders in the same term
   - Linearizability of operations

**Note**: Property tests found had 0 assertions, suggesting they may need strengthening.

## Guidelines

### What Makes a Good Distributed Systems Test
1. **Tests actual distribution**: Multi-node scenarios with network effects
2. **Verifies consensus**: All nodes agree on system state
3. **Handles failures**: Injects and recovers from various failure modes
4. **No timing assumptions**: Uses condition-based waiting, not sleep()
5. **Clear assertions**: Explicitly verifies expected behavior

### What to Avoid
1. **Hollow tests**: Creating infrastructure without verifying behavior
2. **Tautological tests**: Testing that `true == true`
3. **Sleep-and-hope**: Using sleep() instead of waiting for conditions
4. **Skipping on failure**: Tests that return success when they encounter issues
5. **Surface-level checks**: Only verifying operations don't error

### Test Infrastructure to Use
- `TestCluster`: Provides Raft-aware cluster management
- `wait_for_condition_with_backoff()`: Instead of sleep()
- `TestNode`: Provides test-friendly node interface
- MadSim: For deterministic simulation with failure injection

## Success Criteria
1. All tests have meaningful assertions that would fail if the system was broken
2. Zero sleep() calls remain in the test suite
3. Network partition tests actually partition the network
4. Property tests verify real invariants, not trivial properties
5. New tests for Byzantine failures, clock skew, and chaos scenarios
6. Test suite catches real distributed systems bugs

## Priority Order
1. Fix tests with no assertions (CRITICAL - these are actively harmful)
2. Replace placeholder tests (HIGH - false sense of coverage)
3. Remove sleep() calls (HIGH - source of flakiness)
4. Add missing test scenarios (MEDIUM - needed for production confidence)
5. Strengthen property tests (MEDIUM - better invariant checking)

## Validation
After making changes:
```bash
# Ensure no empty test bodies remain
rg "async \{[^}]*\}" tests/ | grep -v "assert"

# Ensure no sleep calls remain
rg "sleep\(Duration" tests/

# Run stress tests to find flakiness
cargo nextest run --profile stress --runs 100

# Verify tests actually fail when system is broken
# (Temporarily break Raft implementation and ensure tests catch it)
```

Begin with Phase 1 and work systematically through each phase. Ask clarifying questions if you need more context about the distributed system's expected behavior.

---

## COMPLETED WORK SUMMARY

### ✅ Achievements
1. **Corrected Assessment**: Found most tests are actually comprehensive, not hollow
2. **Fixed Actual Hollow Tests**: 
   - `raft_quick_test.rs`: Now verifies Raft leader election and consensus
   - `storage_edge_case_tests.rs`: Added assertions for resource limits and input validation
3. **Started Sleep Replacement**: Replaced 1 sleep() call with condition-based waiting
4. **Found Real Bugs**: Improved tests caught Raft leader election timing issues
5. **Updated Documentation**: CLAUDE.md and TEST_SUITE_IMPROVEMENTS_SUMMARY.md

### 🔄 Remaining Work
1. **Replace 30+ remaining sleep() calls** with condition-based waiting
2. **Add Byzantine failure tests** for malicious node behavior
3. **Add clock skew tests** for time-based edge cases
4. **Review and strengthen property tests** for better invariant checking

### 📊 Impact
- **Before**: Tests could pass with broken Raft consensus
- **After**: Tests verify actual distributed system behavior and catch real bugs
- **Quality**: Moved from completion checking to correctness verification