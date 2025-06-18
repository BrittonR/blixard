# Blixard Development Plan

## Current Project Status

Blixard is a distributed microVM orchestration platform built in Rust. The core distributed infrastructure is implemented and working:

### ✅ Completed
- Raft consensus with full state machine
- Multi-node cluster formation 
- Peer management with automatic reconnection
- gRPC communication layer
- Persistent storage with redb
- Task scheduling and worker management
- Comprehensive test infrastructure (unit, property, deterministic simulation)
- CLI with node management commands

### ❌ Not Implemented
- **MicroVM integration** - Only stubs exist, no actual VM lifecycle management
- **Tailscale integration** - CLI flag exists but no implementation
- **CLI cluster commands** - gRPC endpoints exist but no CLI commands
- **Raft log compaction** - Snapshots exist but no automatic compaction
- **Metrics & observability** - Dependencies exist but no implementation
- **Configuration files** - Only CLI flags supported
- **Production features** - No auth, TLS, rate limiting, backups

## Test Coverage Analysis

### Coverage Matrix

| Component | Unit Tests | PropTest | MadSim | Critical Gaps |
|-----------|------------|----------|---------|---------------|
| CLI | ✅ Yes | ❌ No | ❌ No | ~~Integration with node startup~~ ✅ Fixed |
| gRPC Server | ✅ Partial | ❌ No | ✅ Yes | Many endpoints untested |
| Node | ✅ Yes | ✅ Yes | ✅ Yes | Recovery scenarios |
| SharedNodeState | ✅ Yes | ✅ Yes | ❌ No | ~~Concurrent mutations~~ ✅ Fixed |
| Raft Manager | ✅ Partial | ✅ Yes | ✅ Yes | Snapshot/compaction |
| Peer Connector | ✅ Yes | ❌ No | ❌ No | Retry logic, buffering |
| Storage | ✅ Yes | ❌ No | ❌ No | Corruption, concurrency |
| VM Manager | ✅ Partial | ❌ No | ❌ No | Only stubs tested |
| **Raft Codec** | **✅ Yes** | **✅ Yes** | **❌ No** | ~~**ZERO TESTS!**~~ **✅ FIXED** |

### Critical Test Gaps
1. ~~**Raft Codec** - Message serialization completely untested~~ ✅ FIXED - Added comprehensive tests
2. ~~**Integration tests** - No end-to-end workflows~~ ✅ FIXED - Added lifecycle integration tests
3. **Failure recovery** - Limited crash/corruption testing
4. ~~**Concurrent operations** - Race condition testing missing~~ ✅ FIXED - Added PropTest for SharedNodeState
5. **Performance/load** - No scalability tests

## Development Priorities

### ~~Priority 1: Test Coverage for Critical Infrastructure~~ ✅ COMPLETED
~~Before adding new features, we should ensure existing critical components are properly tested:~~

1. ~~**Add tests for `raft_codec.rs`** (CRITICAL)~~ ✅ DONE
   - ~~Message serialization/deserialization~~ ✅
   - ~~Error handling for malformed data~~ ✅
   - ~~Performance with large messages~~ ✅

2. ~~**Expand PropTest coverage**~~ ✅ PARTIALLY DONE
   - ~~SharedNodeState concurrent operations~~ ✅
   - Peer Connector retry logic ❌ TODO
   - Storage concurrent transactions ❌ TODO
   - gRPC server request handling ❌ TODO

3. ~~**Add integration tests**~~ ✅ DONE
   - ~~Full node lifecycle (start → join → tasks → leave → stop)~~ ✅
   - Multi-node task distribution 🔧 Partial
   - Failure and recovery scenarios 🔧 Partial

### Priority 2: MicroVM Integration
This is the core purpose of Blixard. Without it, the platform cannot manage VMs.

**Implementation tasks:**
1. Research microvm.nix integration patterns
2. Design VM lifecycle state machine
3. Implement VM process management
4. Add network configuration for VMs
5. Handle VM storage allocation
6. Implement console/serial access
7. Add comprehensive tests

### ~~Priority 3: CLI Cluster Management Commands~~ ✅ COMPLETED
~~Already marked "In Progress" in todo.md. Would significantly improve usability.~~

**Commands implemented:**
- ✅ `blixard cluster join --peer <addr> --local-addr <addr>`
- ✅ `blixard cluster leave --local-addr <addr>`
- ✅ `blixard cluster status --addr <addr>`

### Priority 4: Raft Log Compaction
Essential for production use to prevent unbounded log growth.

**Implementation tasks:**
1. Add compaction thresholds configuration
2. Implement periodic compaction trigger
3. Clean up old logs after snapshots
4. Add tests for compaction scenarios

### Priority 5: Tailscale Integration
Prominently featured in README but completely unimplemented.

**Implementation tasks:**
1. Research Tailscale API integration
2. Implement device authentication
3. Add peer discovery via Tailscale
4. Configure secure networking for VMs
5. Add tests with mock Tailscale

## Next Immediate Steps

1. ~~**Fix test coverage gaps** (1-2 days)~~ ✅ COMPLETED
   - ~~Write comprehensive tests for `raft_codec.rs`~~ ✅
   - ~~Add property tests for SharedNodeState~~ ✅
   - ~~Create integration test framework~~ ✅

2. **Research MicroVM integration** (1 day) 🔜 NEXT
   - Study microvm.nix documentation
   - Create proof-of-concept VM creation
   - Design integration architecture

3. **Implement basic MicroVM operations** (3-5 days)
   - Start with create/delete VM
   - Add start/stop functionality
   - Implement status monitoring
   - Add comprehensive tests

4. ~~**Add CLI cluster commands** (1 day)~~ ✅ COMPLETED
   - ~~Implement the three missing commands~~ ✅
   - ~~Add integration tests~~ ✅
   - ~~Update documentation~~ ✅

## Success Metrics

- All components have >80% test coverage
- Zero untested critical paths (like raft_codec)
- Can create, start, stop, and delete VMs reliably
- Three-node clusters form and operate consistently
- All tests pass deterministically with MadSim