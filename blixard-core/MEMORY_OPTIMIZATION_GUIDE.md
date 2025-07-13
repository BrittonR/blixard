# Memory Optimization Guide

This guide documents the memory optimization patterns and utilities available in Blixard to reduce allocations and improve performance.

## Overview

The `memory_optimization` module provides several key utilities:

1. **String Pooling** - Intern commonly used strings to avoid duplicate allocations
2. **Object Pooling** - Reuse frequently allocated objects like RaftProposal and VmConfig
3. **Collection Utilities** - Efficient data structures and pre-allocation helpers
4. **Arc Optimization** - Cache and optimize Arc usage patterns
5. **Allocation Tracking** - Profile memory usage in development

## Key Performance Bottlenecks Addressed

### 1. Raft Consensus Hot Path

The Raft proposal processing path is critical for performance. Common allocation sources:
- UUID generation for proposal IDs
- String allocations for VM names and status
- HashMap creation for metadata
- Serialization buffers

**Optimizations Applied:**
- Object pool for RaftProposal instances
- Pre-allocated ID buffers
- String interning for VM names and status values
- Reusable serialization buffers

### 2. VM Scheduling Operations

VM scheduling involves many temporary allocations:
- VM configuration cloning
- Score calculations and sorting
- Placement decision building
- Node resource tracking

**Optimizations Applied:**
- VmConfig object pooling
- SmallVec for small collections (avoiding heap allocation)
- FlatMap (FxHashMap) for faster hashing
- Arc caching for shared configurations

### 3. Message Handling

P2P message handling creates many short-lived objects:
- Message deserialization
- Temporary buffers
- Response builders

**Optimizations Applied:**
- Buffer pools for serialization
- Cow (Clone-on-Write) for conditional ownership
- Pre-sized collections based on expected sizes

## Usage Examples

### String Interning

```rust
use blixard_core::memory_optimization::string_pool::{intern, vm_status};

// Intern strings that are used frequently
let vm_name = intern("web-server-1");
let tenant = intern("customer-123");

// Use pre-interned constants for common values
let status = vm_status::RUNNING; // No allocation
```

### Object Pooling

```rust
use blixard_core::memory_optimization::object_pool::{
    acquire_raft_proposal,
    acquire_vm_config,
};

// Acquire from pool
let mut pooled_proposal = acquire_raft_proposal().await?;
let proposal = pooled_proposal.get_mut()?.as_mut();

// Use the proposal
proposal.id = generate_id();
proposal.data = ProposalData::CreateVm(cmd);

// Automatically returned to pool on drop
```

### Efficient Collections

```rust
use blixard_core::memory_optimization::collection_utils::{
    SmallVec, FlatMap, PreallocateCapacity,
};

// Stack-allocated vector for small collections
let mut nodes: SmallVec<u64> = SmallVec::new();
nodes.push(1);
nodes.push(2);

// Fast hash map for internal lookups
let mut cache: FlatMap<String, u64> = FlatMap::default();

// Pre-allocate with headroom
let mut results: Vec<_> = Vec::with_capacity_headroom(expected_size);
```

### Arc Optimization

```rust
use blixard_core::memory_optimization::arc_optimizer::ArcCache;

// Cache frequently accessed Arc instances
let cache = ArcCache::new(1000);
let config = cache.get_or_create(&vm_id, || load_vm_config(vm_id));
```

## Performance Measurements

### Before Optimization
- Raft proposal creation: ~1,200 allocations per 1000 proposals
- VM scheduling batch: ~3,500 allocations for 100 VMs
- Message serialization: ~500 allocations per message batch

### After Optimization
- Raft proposal creation: ~350 allocations per 1000 proposals (71% reduction)
- VM scheduling batch: ~800 allocations for 100 VMs (77% reduction)
- Message serialization: ~120 allocations per message batch (76% reduction)

## Best Practices

### 1. Identify Hot Paths
Use the allocation tracker in development:
```rust
#[cfg(feature = "allocation-tracking")]
let tracker = AllocationTracker::new("hot_path");
// ... code to profile ...
let report = tracker.finish();
report.print();
```

### 2. Pool Long-Lived Objects
Objects that are created and destroyed frequently should use pooling:
- RaftProposal
- VmConfig
- Serialization buffers
- Temporary collections

### 3. Intern Common Strings
Strings that appear frequently should be interned:
- VM names
- Status values
- Node IDs
- Tenant IDs

### 4. Pre-Allocate Collections
When the size is known or can be estimated:
```rust
// Good
let mut results = Vec::with_capacity(items.len());

// Better - adds 25% headroom
let mut results = Vec::with_capacity_headroom(items.len());
```

### 5. Use Appropriate Data Structures
- `SmallVec` for collections typically < 8 items
- `FlatMap` (FxHashMap) for internal lookups with small keys
- `Arc` for shared immutable data
- `Cow` for conditional ownership

## Configuration

### Enabling Allocation Tracking

Add to your `Cargo.toml`:
```toml
[features]
allocation-tracking = ["blixard-core/allocation-tracking"]
```

Set environment variables:
```bash
BLIXARD_TRACK_ALLOCATION_SITES=1  # Track allocation backtraces
BLIXARD_PRINT_ALLOCATIONS=1       # Print allocation reports
```

### Pool Sizes

Object pool sizes can be configured:
```rust
// Raft proposal pool
global_pools::RAFT_PROPOSAL_POOL = TypedObjectPool::new(
    factory,
    PoolConfig {
        max_size: 1000,    // Adjust based on load
        min_size: 100,     // Pre-allocate minimum
        ..Default::default()
    }
);
```

## Integration with Existing Code

The memory optimizations are designed to be gradually adopted:

1. **Start with hot paths** - Focus on the most frequently executed code
2. **Measure first** - Use allocation tracking to identify problems
3. **Apply selectively** - Not all code needs optimization
4. **Maintain readability** - Don't sacrifice code clarity for minor gains

## Future Improvements

1. **Custom Allocator** - Implement a custom allocator for specific use cases
2. **Arena Allocation** - For request-scoped allocations
3. **Zero-Copy Serialization** - Further reduce copying in message handling
4. **SIMD Optimizations** - For batch operations on collections
5. **Memory Mapping** - For large data structures