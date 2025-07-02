# VOPR Fuzzer for Blixard

The VOPR (Viewstamped Operation Replication) fuzzer is a TigerBeetle-inspired testing framework designed to thoroughly test Blixard's distributed consensus system through deterministic, accelerated time fuzzing.

## Features

### 1. **Time Acceleration**
- Simulated time runs up to 1000x faster than real time
- Deterministic time progression for reproducible tests
- Controlled clock skew injection between nodes
- Time dilation for stress testing specific operations

### 2. **Comprehensive Fuzzing**
- Generate random but valid operation sequences
- Coverage-guided fuzzing to explore new code paths
- Automatic test case minimization (shrinking)
- Reproducible failures with seed-based generation

### 3. **Safety and Liveness Checking**
- **Safety invariants:**
  - Single leader per term
  - Monotonic term progression
  - Log matching properties
  - Commit safety
  - VM uniqueness
  - Resource limits
  - Consistent views

- **Liveness invariants:**
  - System makes progress
  - Leader election eventually succeeds
  - Client requests eventually complete

### 4. **Fault Injection**
- Network partitions and healing
- Message drops, duplicates, and reordering
- Clock jumps and drift
- Byzantine node behaviors:
  - Equivocation (conflicting messages)
  - Multiple votes in same term
  - Fake leader claims
  - Message corruption
  - Replay attacks
  - Identity forgery

### 5. **State Tracking and Visualization**
- Complete state history tracking
- Operation latency monitoring
- Resource usage tracking
- Anomaly detection
- Human-readable reports
- Timeline visualization

## Usage

### Running the Fuzzer

```bash
# Run with default settings
cargo run --example vopr_demo --features vopr

# Run with a specific seed for reproduction
cargo run --example vopr_demo --features vopr -- 12345

# Run the test suite
cargo test --features vopr vopr_test
```

### Configuration

```rust
let config = VoprConfig {
    seed: 42,                      // Random seed for reproducibility
    time_acceleration: 1000,       // 1000x time acceleration
    max_clock_skew_ms: 5000,       // Max ±5 seconds clock skew
    check_liveness: true,          // Enable liveness checking
    check_safety: true,            // Enable safety checking
    max_operations: 10000,         // Max operations to generate
    enable_shrinking: true,        // Enable test case minimization
    enable_visualization: true,    // Generate visual reports
    coverage_guided: true,         // Use coverage feedback
};
```

### Understanding the Output

When a failure is found, VOPR generates:

1. **Failure Summary** - Basic information about the violation
2. **Operation Sequence** - The exact operations that led to the failure
3. **State Progression** - How the system state evolved
4. **Minimal Reproducer** - Shrunk test case with minimal operations
5. **Rust Code** - Copy-paste ready test case

Example output:
```
❌ INVARIANT VIOLATION DETECTED!
  - SingleLeader: Multiple leaders in term 5: [1, 3]

Shrinking test case...
Shrunk from 147 to 23 operations

Operation Sequence:
--------------------
   0: StartNode(1)
   1: StartNode(2)
   2: StartNode(3)
  ...
  22: NetworkPartition([1] | [2, 3])

State Progression:
----------------------------------
Time  | Nodes | Leader | MaxView | MaxCommit
------|-------|--------|---------|----------
    0 |     3 |      1 |       1 |         0
   10 |     3 |      1 |       1 |         5
   22 |     3 |   1,3  |       5 |        10
      ⚠️  Multiple leaders in term 5: [1, 3]
```

## Architecture

### Components

1. **Time Accelerator** (`time_accelerator.rs`)
   - Manages simulated time for all nodes
   - Injects clock skew and drift
   - Provides deterministic time progression

2. **Fuzzer Engine** (`fuzzer_engine.rs`)
   - Generates test cases
   - Tracks code coverage
   - Manages test corpus
   - Implements mutation strategies

3. **Operation Generator** (`operation_generator.rs`)
   - Creates valid operation sequences
   - Supports weighted operation selection
   - Implements Byzantine behaviors

4. **Invariant Checker** (`invariants.rs`)
   - Validates safety properties
   - Checks liveness conditions
   - Reports violations

5. **State Tracker** (`state_tracker.rs`)
   - Records all state transitions
   - Tracks resource usage
   - Detects anomalies

6. **Test Harness** (`test_harness.rs`)
   - Interfaces with actual Blixard system
   - Executes operations
   - Captures state snapshots

7. **Shrinker** (`shrink.rs`)
   - Minimizes failing test cases
   - Uses multiple strategies
   - Preserves failure conditions

8. **Visualizer** (`visualizer.rs`)
   - Generates human-readable reports
   - Creates timeline visualizations
   - Produces reproducer code

### How It Works

1. **Initialization**
   - Create time accelerator with desired speed
   - Initialize fuzzer engine with seed
   - Set up test harness with empty cluster

2. **Operation Generation**
   - Use weighted random selection
   - Ensure valid operation sequences
   - Apply coverage feedback

3. **Execution**
   - Execute each operation via test harness
   - Advance simulated time
   - Capture state snapshot

4. **Invariant Checking**
   - Check safety invariants after each operation
   - Track liveness over time windows
   - Report any violations

5. **Failure Processing**
   - Shrink test case to minimal reproducer
   - Generate detailed report
   - Save reproducer code

## Writing Custom Invariants

To add a new invariant:

```rust
struct MyInvariant;

impl Invariant for MyInvariant {
    fn name(&self) -> &str {
        "MyInvariant"
    }
    
    fn check(&self, state: &StateSnapshot) -> Result<(), String> {
        // Check your invariant
        if some_condition {
            return Err("Invariant violated: explanation".to_string());
        }
        Ok(())
    }
    
    fn invariant_type(&self) -> InvariantType {
        InvariantType::Safety  // or Liveness
    }
}
```

## Best Practices

1. **Reproducibility**
   - Always save the seed that caused a failure
   - Use the same seed to reproduce issues
   - Include seeds in bug reports

2. **Coverage**
   - Enable coverage-guided fuzzing for better exploration
   - Monitor corpus growth over time
   - Add new operation types for uncovered code

3. **Performance**
   - Start with smaller operation counts
   - Increase time acceleration gradually
   - Use shrinking to minimize test cases

4. **Debugging**
   - Use visualization to understand failures
   - Check timeline for anomalies
   - Examine state progression carefully

## Future Enhancements

- Integration with continuous fuzzing infrastructure
- Distributed fuzzing across multiple machines
- Property-based testing integration
- Model checking integration
- Automatic invariant discovery
- Performance regression detection