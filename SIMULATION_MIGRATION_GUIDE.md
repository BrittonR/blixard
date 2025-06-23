# Simulation Test Migration Guide

## Problem

The simulation tests are currently trying to import from `blixard_core`, which causes conflicts between `tonic` and `madsim-tonic`. The simulation workspace uses MadSim's deterministic versions of tokio and tonic, which are incompatible with the regular versions used in the main workspace.

## Solution

Create a self-contained simulation test infrastructure within the `simulation` crate that doesn't depend on `blixard_core`. This maintains the separation between workspaces while providing all necessary test utilities.

## What's Been Created

### 1. Test Helpers (`simulation/src/test_helpers.rs`)
- `TestNode` - Simplified test node wrapper for simulation tests
- `TestCluster` - Multi-node test cluster management
- `PortAllocator` - Thread-safe port allocation
- `timing` module - Environment-aware timing utilities
- `wait_for_condition` - Condition-based waiting

### 2. Types (`simulation/src/types.rs`)
- `SharedNodeState` - Simulated shared node state
- `PeerInfo`, `RaftStatus` - Cluster state types
- `TaskSpec`, `ResourceRequirements` - Task scheduling types
- `BlixardError`, `BlixardResult` - Error types for tests

### 3. Core Types (in `simulation/src/lib.rs`)
- `NodeConfig`, `VmConfig`, `VmStatus` - Already existed
- Proto types - Re-exported from generated code

## Migration Steps

### Step 1: Update Imports

Replace all `blixard_core` imports with `blixard_simulation`:

```rust
// Before
use blixard_core::{
    test_helpers::{TestNode, TestCluster, timing},
    proto::{HealthCheckRequest},
    error::{BlixardError, BlixardResult},
};

// After
use blixard_simulation::*;  // Imports all re-exported types
```

### Step 2: Adapt Test Logic

The simulation test helpers are simplified compared to the full implementation. You'll need to adapt tests in one of these ways:

#### Option A: Use Mock Behavior
```rust
// Instead of creating a real Node, use TestNode
let mut test_node = TestNode::new(1, &port_allocator).await;
test_node.start().await?;

// Use the gRPC client to test behavior
if let Some(client) = test_node.client() {
    // Test via gRPC API
}
```

#### Option B: Create Test-Specific Mocks
```rust
// Create a mock implementation for what you're testing
struct MockRaftManager {
    // ... fields needed for testing
}

impl MockRaftManager {
    // Implement just the behavior you need to test
}
```

#### Option C: Test at the Protocol Level
```rust
// Test using proto types directly
let request = JoinRequest {
    node_id: 2,
    node_addr: "127.0.0.1:7002".to_string(),
};

// Simulate the behavior you want to test
```

### Step 3: Run the Migration Script

A migration script is provided to automate the import updates:

```bash
cd simulation
./migrate_tests.sh
```

This script will:
- Replace `blixard_core::` with `blixard_simulation::`
- Fix import paths for test helpers, types, and errors
- Remove unnecessary module prefixes

### Step 4: Manual Fixes

After running the script, you'll need to:

1. **Review generated imports** - Ensure the imports are correct
2. **Adapt test logic** - Update tests to use the simplified test helpers
3. **Add missing functionality** - If tests need specific behavior, add it to the simulation crate
4. **Fix compilation errors** - Run `cargo check` and fix any remaining issues

## Example Migration

### Before:
```rust
use blixard_core::{
    node::Node,
    types::NodeConfig,
    test_helpers::{PortAllocator, timing},
};

#[tokio::test]
async fn test_node_behavior() {
    let config = NodeConfig { /* ... */ };
    let mut node = Node::new(config);
    node.initialize().await.unwrap();
    // ... test node directly
}
```

### After:
```rust
use blixard_simulation::*;

#[tokio::test]
async fn test_node_behavior() {
    let port_allocator = PortAllocator::new(30000);
    let mut test_node = TestNode::new(1, &port_allocator).await;
    test_node.start().await.unwrap();
    
    // Test via gRPC client or mock the behavior
    if let Some(client) = test_node.client() {
        // ... test via API
    }
}
```

## Design Principles

1. **Isolation** - Simulation tests should be self-contained
2. **Determinism** - Use MadSim's deterministic runtime features
3. **Simplicity** - Test helpers should be minimal but sufficient
4. **Protocol-First** - Test behavior through the gRPC API when possible

## Adding New Functionality

If you need additional types or utilities:

1. Add them to `simulation/src/types.rs` or `simulation/src/test_helpers.rs`
2. Re-export from `simulation/src/lib.rs` if commonly used
3. Keep them independent of `blixard_core` to avoid conflicts

## Troubleshooting

### "Type not found" errors
- Check if the type is re-exported in `simulation/src/lib.rs`
- Add missing types to `simulation/src/types.rs`

### "Method not found" errors
- The simulation test helpers are simplified
- Either mock the behavior or test via gRPC API

### Tonic version conflicts
- Ensure you're not importing anything from `blixard_core`
- Use only types defined in the simulation crate

## Benefits

1. **Clean separation** - No tonic/madsim-tonic conflicts
2. **Faster compilation** - Simulation tests compile independently
3. **Better determinism** - All tests use MadSim's deterministic runtime
4. **Clearer dependencies** - Easy to see what each test actually needs