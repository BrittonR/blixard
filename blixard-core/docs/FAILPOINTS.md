# Failpoint System Documentation

## Overview

Blixard includes a comprehensive failpoint system for fault injection testing, inspired by TigerBeetle's approach to deterministic testing and production debugging. This system allows you to inject failures at specific points in the code to test error handling, recovery mechanisms, and distributed system behavior under adverse conditions.

## Architecture

### Core Components

1. **Failpoint Macros** (`fail_point!`, `fail_point_action!`)
   - Inject failure points in production code
   - Compile to no-ops when `failpoints` feature is disabled
   - Support custom error types and actions

2. **Failpoint Scenarios** 
   - Pre-configured failure patterns
   - Probability-based failures
   - Deterministic sequences
   - Timing-based failures

3. **Test Infrastructure**
   - Integration with test helpers
   - Async-compatible helpers
   - Metrics integration for observability

## Injected Failpoints

### Storage Layer (`src/storage.rs`)
- `storage::append_entries` - Fail when appending Raft log entries
- `storage::create_snapshot` - Fail during snapshot creation
- `storage::commit_transaction` - Fail when committing database transactions

### Raft Manager (`src/raft_manager.rs`)
- `raft::propose` - Fail when proposing new entries
- `raft::apply_entry` - Fail when applying committed entries to state machine

### Network Transport (`src/iroh_transport_v2.rs`)
- `network::send_message` - Fail when sending P2P messages

### VM Operations (`src/vm_backend.rs`)
- `vm::create` - Fail during VM creation
- `vm::start` - Fail when starting VMs
- `vm::stop` - Fail when stopping VMs

### Worker Management (`src/node_shared.rs`)
- `worker::register` - Fail during worker registration

## Usage Examples

### Basic Failpoint Injection

```rust
use crate::fail_point;

pub async fn critical_operation() -> BlixardResult<()> {
    #[cfg(feature = "failpoints")]
    fail_point!("operation::critical");
    
    // Actual operation code
    perform_work().await
}
```

### Custom Error Injection

```rust
#[cfg(feature = "failpoints")]
fail_point!("storage::write", BlixardError::Storage {
    operation: "write".to_string(),
    source: Box::new(std::io::Error::new(
        std::io::ErrorKind::Other,
        "Simulated disk failure"
    )),
});
```

### Test Usage

```rust
#[tokio::test]
async fn test_storage_resilience() {
    failpoints::init();
    
    // Configure 20% failure rate
    scenarios::fail_with_probability("storage::write", 0.2);
    
    // Run test operations
    let results = run_storage_operations().await;
    
    // Verify system handles failures gracefully
    assert!(results.success_rate() > 0.7);
    
    scenarios::disable("storage::write");
}
```

### Deterministic Failure Sequences

```rust
// Fail on exactly the 3rd, 7th, and 11th calls
fail::cfg("raft::propose", "2*off->1*return->4*off->1*return->3*off->1*return->off").unwrap();
```

### Production Debugging

```bash
# Set failpoints via environment variable
export FAILPOINTS="storage::write=1%return;network::send=5%delay(100)"

# Run the application with failures injected
cargo run --features failpoints

# Disable all failpoints
export FAILPOINTS="off"
```

## Configuration Syntax

### Basic Patterns
- `return` - Return error immediately
- `panic(msg)` - Panic with message
- `delay(ms)` - Delay execution by milliseconds
- `print(msg)` - Print debug message
- `off` - Disable failpoint

### Probability
- `20%return` - 20% chance of failure
- `5%delay(100)` - 5% chance of 100ms delay

### Sequences
- `3*off->1*return->off` - Skip 3, fail once, then disable
- `1*return->4*off->1*return` - Fail 1st, skip 4, fail 6th

## Best Practices

### 1. Production Safety
- Always wrap failpoints in `#[cfg(feature = "failpoints")]`
- Ensure failpoints compile to no-ops in release builds
- Use meaningful failpoint names following the pattern `component::operation`

### 2. Test Design
- Test both individual failures and cascading failures
- Verify system recovery after failures
- Test probabilistic failures for realistic scenarios
- Use deterministic sequences for reproducible bugs

### 3. Debugging Production Issues
- Enable specific failpoints to reproduce issues
- Use low probability rates to avoid disrupting service
- Monitor metrics while failpoints are active
- Document which failpoints helped diagnose issues

### 4. Common Patterns

#### Testing Leader Election
```rust
// Inject network partition
scenarios::fail_with_probability("network::send_to_node_3", 1.0);
// Verify new leader is elected from nodes 1-2
```

#### Testing Storage Resilience
```rust
// Occasional write failures
scenarios::fail_with_probability("storage::write", 0.1);
// Verify data consistency across retries
```

#### Testing VM Lifecycle
```rust
// Fail VM starts occasionally
scenarios::fail_after_n("vm::start", 5);
// Verify VM manager handles failures gracefully
```

## Integration with Observability

Failpoints automatically integrate with the metrics system:

- `failpoint_triggered_total` - Counter of failpoint activations
- `failpoint_injection_duration` - Histogram of delay injections
- `failpoint_errors_total` - Counter of errors injected

Use Grafana dashboards to visualize failpoint activity during tests.

## Running Failpoint Tests

```bash
# Run all failpoint tests
cargo test --features "test-helpers failpoints" failpoint

# Run specific test
cargo test --features "test-helpers failpoints" test_storage_write_failures

# Run with verbose output
cargo test --features "test-helpers failpoints" -- --nocapture

# Run failpoint demo
cargo run --example failpoint_demo --features failpoints
```

## Future Enhancements

1. **Failure Scenarios Library**
   - Pre-built scenarios for common distributed system failures
   - Network partition patterns
   - Clock skew simulations
   - Resource exhaustion scenarios

2. **Chaos Engineering Integration**
   - Automated failure injection in staging
   - Gameday scenarios
   - Failure budget enforcement

3. **Advanced Patterns**
   - Conditional failures based on system state
   - Coordinated multi-component failures
   - Time-based failure windows

4. **Tooling**
   - CLI for runtime failpoint management
   - Web UI for failure injection control
   - Integration with load testing tools