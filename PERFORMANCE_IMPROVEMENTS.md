# Blixard Performance Improvements

## Overview

This document outlines systematic performance improvements for the Blixard codebase, focusing on hot paths in distributed systems components.

## Key Performance Issues Identified

### 1. Excessive Cloning in Hot Paths

#### Problem Areas:
- **metrics_otel.rs**: Clone operations in metric recording (line 134: `node_attr.clone()`)
- **node_shared/mod.rs**: Full HashMap clones in `cluster_members()` method (line 127)
- **vopr/operation_generator.rs**: String clones in operation generation
- **raft/handlers.rs**: Proposal ID cloning in pending proposals map

#### Solutions:
1. **Use Arc<str> for shared strings** instead of String clones
2. **Return references or iterators** instead of cloning collections
3. **Use Cow<'_, str>** for strings that are rarely modified
4. **Pre-allocate and reuse buffers** for temporary strings

### 2. String Allocations in Loops

#### Problem Areas:
- **resource_managers/**: Multiple `format!()` calls inside loops
- **raft/snapshot.rs**: String formatting for table names in loops
- **tracing_otel.rs**: String concatenation in attribute building

#### Solutions:
1. **Use write!() to a pre-allocated buffer** instead of format!()
2. **Move string constants outside loops**
3. **Use static strings where possible**
4. **Batch string operations**

### 3. Lock Contention in SharedNodeState

#### Problem Areas:
- Multiple RwLock acquisitions for related operations
- Full collection clones under locks
- No use of try_read/try_write for non-critical paths

#### Solutions:
1. **Batch related operations** under a single lock acquisition
2. **Use parking_lot RwLock** for better performance
3. **Implement read-through caching** for frequently accessed data
4. **Use dashmap for concurrent collections**

### 4. Inefficient Metrics Recording

#### Problem Areas:
- Label allocation on every metric recording
- No caching of metric handles
- String allocations for labels

#### Solutions:
1. **Pre-compute and cache metric handles** with common label combinations
2. **Use &'static str for labels** where possible
3. **Batch metric updates**

## Specific Optimizations

### 1. SharedNodeState Improvements

```rust
// Before: 
pub fn cluster_members(&self) -> HashMap<u64, PeerInfo> {
    self.cluster_members.read().unwrap().clone()
}

// After:
pub fn with_cluster_members<F, R>(&self, f: F) -> R 
where 
    F: FnOnce(&HashMap<u64, PeerInfo>) -> R
{
    let members = self.cluster_members.read().unwrap();
    f(&*members)
}

// For iteration:
pub fn iter_cluster_members(&self) -> impl Iterator<Item = (u64, PeerInfo)> + '_ {
    // Use dashmap for lock-free iteration
}
```

### 2. Metrics Optimization

```rust
// Before:
self.raft_proposals_total.add(1, &[node_attr.clone()]);

// After:
// Pre-compute labels at initialization
struct MetricsCache {
    node_labels: Vec<KeyValue>,
    proposal_counter: Counter<u64>,
}

// Use cached handles
self.metrics_cache.proposal_counter.add(1, &[]);
```

### 3. String Handling in Hot Paths

```rust
// Before:
for entry in entries {
    let table_name = format!("{}_table", entry.name);
    // ...
}

// After:
use std::fmt::Write;
let mut buffer = String::with_capacity(64);
for entry in entries {
    buffer.clear();
    write!(&mut buffer, "{}_table", entry.name).unwrap();
    // Use &buffer instead of allocating new string
}
```

### 4. Proposal Handling Optimization

```rust
// Before:
pending.insert(proposal.id.clone(), response_tx);

// After:
// Use Arc<[u8]> for proposal IDs that are shared
pub struct RaftProposal {
    pub id: Arc<[u8]>,
    // ...
}
```

### 5. Collection Pre-allocation

```rust
// In resource analysis and other places
// Before:
let mut results = Vec::new();

// After:
let mut results = Vec::with_capacity(estimated_size);
```

## Implementation Priority

### Phase 1: Quick Wins (High Impact, Low Risk)
1. **String allocation fixes** in loops
2. **Pre-allocate collections** with known sizes
3. **Cache metric handles**
4. **Use Cow<str> for rarely-modified strings**

### Phase 2: Medium Complexity
1. **Replace HashMap clones** with iterator/closure patterns
2. **Implement metrics caching layer**
3. **Optimize lock usage patterns**
4. **Batch related operations**

### Phase 3: Architectural Changes
1. **Switch to dashmap** for concurrent collections
2. **Implement read-through caching**
3. **Use Arc<str> pervasively for shared strings**
4. **Connection pooling for Iroh P2P**

## Benchmarking Plan

1. **Micro-benchmarks** for critical functions:
   - SharedNodeState operations
   - Metrics recording
   - Proposal handling
   
2. **Load tests** for distributed operations:
   - Raft consensus throughput
   - VM operation latency
   - Network message processing

3. **Memory profiling** to verify allocation reductions

## Success Metrics

- **50% reduction** in allocations per second in hot paths
- **30% improvement** in Raft proposal throughput
- **25% reduction** in p99 latency for VM operations
- **40% reduction** in lock contention time

## Notes

- All optimizations must maintain correctness
- Add benchmarks before optimizing to measure impact
- Consider using `perf` and `flamegraph` for profiling
- Test under realistic distributed system loads