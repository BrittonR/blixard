# Timing Improvements for Test Reliability

This document summarizes the timing improvements made to resolve test flakiness in the distributed system tests.

## Problem Statement

The tests were experiencing flakiness due to:
- Fixed sleep durations that were insufficient under high system load
- No consideration for slower CI environments
- Lack of retry logic for network operations
- Fixed polling intervals that wasted CPU time
- Insufficient timeouts for distributed operations like leader election

## Solutions Implemented

### 1. **Robust Timing Utilities** (`tests/common/test_timing.rs`)

Created a comprehensive timing utility module with:

- **Environment-aware timeout multipliers**: Automatically 3x timeouts in CI environments
- **Condition-based waiting**: Replace fixed sleeps with condition checks
- **Exponential backoff**: Reduce CPU usage during long waits
- **Connection retry logic**: Robust connection establishment with retries

Key functions:
- `wait_for_condition()` - Wait for a condition with exponential backoff
- `wait_for_async()` - Retry async operations with backoff
- `connect_with_retry()` - Connect to services with automatic retry
- `wait_for_service_ready()` - Wait for service health checks
- `scaled_timeout()` - Apply environment multipliers to timeouts

### 2. **Updated Cluster Integration Tests**

Modified `tests/cluster_integration_tests.rs` to use the new timing utilities:

- Replaced all fixed `sleep()` calls with condition-based waiting
- Added retry logic for all gRPC client connections
- Use `wait_for_service_ready()` instead of fixed delays
- Apply timeout scaling for CI environments

### 3. **Simulation Test Improvements**

Enhanced `simulation/tests/test_util.rs` with:

- Exponential backoff in `wait_for_leader()` and `wait_for_leader_among()`
- Environment-aware test runner timeout
- Better logging of timing information
- Retry logic for client connections

### 4. **Centralized Timing Configuration**

Created `simulation/tests/timing_config.rs` for MadSim tests with:

- Centralized timeout constants
- Environment-based scaling
- Separate configurations for Raft, gRPC, and cluster tests

### 5. **Test Verification Script**

Added `scripts/test-timing.sh` to:

- Run tests multiple times to detect flakiness
- Report stable vs flaky tests
- Support CI environments with appropriate settings
- Generate detailed logs for debugging

### 6. **CI/CD Configuration**

Created `.github/workflows/test.yml` with:

- CI environment variable to trigger timeout multipliers
- Test timing stability verification step
- Log artifact upload on failure
- Separate jobs for regular and simulation tests

## Usage

### Local Development

```bash
# Run tests normally - uses standard timeouts
cargo test

# Run with custom timeout multiplier
TEST_TIMEOUT_MULTIPLIER=2 cargo test

# Verify timing stability
./scripts/test-timing.sh
```

### CI Environment

The CI environment automatically gets 3x timeout multipliers. To customize:

```yaml
env:
  TEST_TIMEOUT_MULTIPLIER: 5  # Use 5x instead of default 3x
```

### Debugging Flaky Tests

1. Run the timing verification script:
   ```bash
   TEST_ITERATIONS=20 ./scripts/test-timing.sh
   ```

2. Check logs in `test-timing-logs/` directory

3. Look for patterns in failures

4. Adjust timeouts in the relevant timing configuration

## Benefits

1. **Reduced flakiness**: Tests now adapt to system load and environment
2. **Better CI reliability**: Automatic timeout scaling for resource-constrained environments
3. **Improved debugging**: Clear logs and timing information
4. **CPU efficiency**: Exponential backoff reduces busy-waiting
5. **Maintainability**: Centralized timing configuration

## Future Improvements

1. Add metrics collection for actual wait times
2. Implement adaptive timeouts based on historical data
3. Add network simulation for more realistic testing
4. Create dashboards for test timing trends