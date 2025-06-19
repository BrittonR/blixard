# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Blixard is a distributed microVM orchestration platform being built in Rust. The project has been reset to a clean state and is currently in early development.

## Current Project State

### Technology Stack
- **Rust** - Core implementation language
- **Tonic + gRPC** - Communication protocol (proto definitions ready)
- **Clap** - CLI argument parsing
- **Raft** - Distributed consensus using [raft crate](https://crates.io/crates/raft)
- **microvm.nix** - VM management via [microvm.nix](https://github.com/astro/microvm.nix)
- **Comprehensive Dependencies** - Ready for distributed systems development

### Current Architecture
- **CLI with gRPC** - Full node command with gRPC server in `src/main.rs`
- **Error Types** - Comprehensive error handling defined in `src/error.rs`
- **Domain Types** - Core types (NodeConfig, VmConfig, VmStatus) in `src/types.rs`
- **gRPC Protocol** - Service definitions in `proto/blixard.proto`
- **gRPC Server** - Basic implementation in `src/grpc_server.rs`
- **Node Structure** - Node management and coordination in `src/node.rs`
- **Node Shared State** - Thread-safe shared state with peer management in `src/node_shared.rs`
- **Storage Layer** - Raft storage implementation in `src/storage.rs`
- **VM Manager** - VM lifecycle and state management in `src/vm_manager.rs`
- **Peer Connector** - Manages gRPC connections to cluster peers in `src/peer_connector.rs`

### Implementation Status
Recent progress:
- ✅ Node CLI command with gRPC server startup
- ✅ Basic gRPC service implementation
- ✅ MadSim integration for deterministic testing
- ✅ Tonic 0.12 upgrade for compatibility
- ✅ Raft consensus implementation (complete with state machine)
- ✅ Distributed storage (redb integrated)
- ✅ Task scheduling with resource requirements
- ✅ Worker management and health monitoring
- ✅ Multi-node cluster formation (JoinCluster/LeaveCluster RPCs)
- ✅ Peer management and dynamic configuration changes
- ✅ Peer connection management with automatic reconnection
- ✅ Single-node cluster bootstrap with proper Raft initialization
- ✅ **Raft snapshot support** - Full implementation for state transfer to lagging nodes
- ✅ **Configuration reconstruction** - Fixes for joining nodes with incomplete state
- ✅ **Test reliability improvements** - Replaced 37 sleep() calls with condition-based waiting
- ✅ **Worker registration system** - Automatic registration and capacity tracking
- ✅ **Raft proposal pipeline** - Fixed hanging task submissions
- ✅ **State machine snapshot application** - Implemented missing `apply_snapshot()` method
- ✅ **Snapshot testing** - Comprehensive test coverage for snapshot functionality
- 🔧 VM lifecycle management (stubs only)

## Development Commands

### Build and Test
```bash
cargo build                # Build the project
cargo test                 # Run all tests
cargo test --features test-helpers  # Run integration tests that use test infrastructure
cargo test --features simulation    # Run with madsim deterministic simulation
cargo test --features failpoints    # Run with failpoint injection
cargo test --features all-tests     # Run all testing frameworks
cargo run -- --help       # Show CLI help

# Cargo Nextest - improved test runner with better parallelization and retry capabilities
cargo nextest run          # Run all tests with nextest (faster, better isolation)
cargo nextest run --profile ci  # Run with CI profile (more retries, JUnit output)
cargo nt                   # Short alias for nextest run
cargo nt-all              # Run all tests with test-helpers feature
cargo nt-ci               # Run with CI profile
cargo nt-stress           # Run stress test profile (no retries, find real failure rate)
cargo nt-retry            # Run with retries enabled, no fail-fast
cargo nt-verbose          # Run with output capture disabled
```

### Nextest Configuration
Cargo nextest is configured in `.config/nextest.toml` with:
- **Test groups**: Limits concurrent execution for resource-intensive tests
- **Per-test retries**: Automatic retries for known flaky tests (e.g., three_node_cluster)
- **Profiles**: `default` (local dev), `ci` (CI/CD), `stress` (reliability testing)

To install nextest:
```bash
cargo install cargo-nextest --locked
```

### Testing Frameworks
```bash
# Comprehensive test suite - run all tests
cargo test                          # All unit and integration tests
cargo test --all-targets           # Include all test types

# Specific test categories
cargo test --test cli_tests         # CLI command parsing tests
cargo test --test error_tests       # Error handling tests  
cargo test --test node_tests        # Node functionality tests
cargo test --test types_tests       # Type serialization tests
cargo test --test storage_tests     # Database persistence tests

# Property-based testing with PropTest
cargo test --test error_proptest    # Error property validation
cargo test --test types_proptest    # Type constraint properties
cargo test --test node_proptest     # Node behavior properties
cargo test --test proptest_example  # Original domain properties (7 tests)

# Model checking with stateright (WORKING - 2 tests)  
cargo test --test stateright_simple_test

# MadSim deterministic simulation (FULLY DETERMINISTIC)
./scripts/sim-test.sh  # Run all simulation tests
MADSIM_TEST_SEED=12345 ./scripts/sim-test.sh  # Reproduce specific run

# Determinism verification tools
./scripts/verify-determinism.sh  # Comprehensive determinism audit
./scripts/demo-determinism.sh    # Simple determinism demonstration

# Note: For MadSim API reference, always check https://docs.rs/madsim/latest/madsim/
# ✅ FIXED: MadSim now properly implemented with modern API
# Working: node creation, network control, packet loss config, partitions

# Failpoint injection (PREPARED)
# - Dependencies added to Cargo.toml
# - Example test file created
# - Needs error type updates to compile
```

### Test Infrastructure Status
- **✅ Comprehensive Unit Tests**: 51 traditional unit tests covering core functionality
  - CLI command parsing and validation (`tests/cli_tests.rs`)
  - Error handling and type conversions (`tests/error_tests.rs`)
  - Node lifecycle and VM management (`tests/node_tests.rs`)
  - Type serialization and validation (`tests/types_tests.rs`)
  - Database persistence and transactions (`tests/storage_tests.rs`)
- **✅ Property-Based Testing**: 30+ property tests using PropTest
  - Error property validation (`tests/error_proptest.rs`)
  - Type constraint and serialization properties (`tests/types_proptest.rs`)
  - Node behavior properties across configurations (`tests/node_proptest.rs`)
  - Original domain property tests (`tests/proptest_example.rs`)
- **✅ Stateright**: Fully working with 2 model checking tests
- **✅ MadSim**: Fully deterministic simulation framework
  - Workspace member `simulation/` for deterministic tests
  - Conditional compilation with `cfg(madsim)`
  - **DETERMINISTIC**: Same seeds produce identical microsecond-precision timing
  - Test harness with cluster, network, and Raft test suites
  - Fixed seed default (was $RANDOM, now 12345)
- **🔧 Failpoints**: Dependencies ready, needs error type updates
- **✅ Common Test Utilities**: Shared helpers with deterministic time abstractions

### CLI Structure
```bash
# Node management (implemented with gRPC server)
cargo run -- node --id 1 --bind 127.0.0.1:7001 --data-dir ./data

# VM management (CLI stubs, use gRPC API)
cargo run -- vm create --name my-vm
cargo run -- vm start my-vm
cargo run -- vm list
```

## Key Files

### Core Implementation
- `src/main.rs` - CLI entry point and command parsing
- `src/lib.rs` - Library root
- `src/error.rs` - Error type definitions
- `src/types.rs` - Domain types and data structures
- `src/node.rs` - Node lifecycle and coordination
- `src/grpc_server.rs` - gRPC service implementation
- `src/storage.rs` - Raft storage backend
- `src/vm_manager.rs` - VM state management
- `proto/blixard.proto` - gRPC service definitions

### Configuration
- `Cargo.toml` - Dependencies configured for distributed systems
- `rust-toolchain.toml` - Rust version specification
- `build.rs` - Protocol buffer compilation
- `flake.nix` - Nix development environment

## Development Guidelines

### Core Rules
1. Follow existing error handling patterns in `src/error.rs`
2. Use structured types from `src/types.rs`
3. Implement gRPC services according to `proto/blixard.proto`
4. Maintain CLI consistency with the patterns in `src/main.rs`

### Architecture Principles
- Runtime abstraction for testability
- Comprehensive error handling with context
- Type-safe configuration and state management
- Clean separation between CLI, library, and protocol layers

### Debugging Distributed Systems Issues

When debugging complex distributed systems issues (especially Raft/consensus problems), follow this systematic approach:

1. **Start with Official Documentation**
   - Check the library's official docs first (e.g., https://docs.rs/raft/latest/raft/)
   - Review canonical examples from the library repository
   - Look for similar issues in the library's GitHub issues

2. **Create a Debugging Document**
   - Create a `*_DEBUG.md` file to track the investigation
   - Document each hypothesis and test result
   - Include relevant log outputs and error messages
   - Add external references and documentation links

3. **Systematic Investigation**
   - Start with the simplest test case (e.g., single node before multi-node)
   - Add extensive logging at key points
   - Check assumptions about library behavior
   - Compare your implementation with working examples

4. **Common Raft Debugging Patterns**
   - Verify initial configuration and bootstrap process
   - Check ready state processing loops
   - Ensure proper handling of committed vs applied indices
   - Verify message serialization/deserialization
   - Check for timing issues in single-node vs multi-node scenarios

5. **Document Failed Attempts**
   - Keep track of what doesn't work and why
   - This prevents repeating the same attempts
   - Helps identify patterns in the failures

### Enhanced Todo Usage for Complex Tasks

When working on complex debugging or multi-step implementation:

1. **Create Hypothesis-Based Todos**
   - Each todo should represent a testable hypothesis
   - Include expected outcome in the todo description
   - Example: "Check if Raft entries are committed in single-node - expect commit index > 0"

2. **Track Investigation Results**
   - Update todo descriptions with findings
   - Mark as completed even if hypothesis was wrong
   - Create follow-up todos based on results

3. **Maintain Context Across Sessions**
   - Use todos to record current understanding
   - Include references to relevant files/lines
   - Note which approaches have been tried

4. **Example Debugging Todo Structure**
   ```
   - [ ] Review library examples for single-node bootstrap pattern
   - [ ] Test if adding empty proposal after bootstrap helps
   - [ ] Check if applied index needs different initialization
   - [ ] Compare our ready loop with canonical example
   ```

### Test-Driven Debugging

When debugging issues, use tests to guide your investigation:

1. **Start with Minimal Reproduction**
   - Create the smallest possible test that reproduces the issue
   - Test single components before testing integration
   - Use `--nocapture` to see all output during test runs

2. **Interpret Test Output Systematically**
   - Look for patterns in test failures
   - Use grep/rg to filter relevant log lines
   - Pay attention to timing and ordering of events
   - Save test outputs for comparison across attempts

3. **Create Focused Test Cases**
   - Write specific tests for each hypothesis
   - Use property-based tests to find edge cases
   - Add logging/tracing to existing tests rather than only adding new tests

4. **Test Output as Investigation Guide**
   ```bash
   # Example: Filter and analyze test output
   cargo test test_name --features test-helpers -- --nocapture 2>&1 | grep -E "pattern1|pattern2" | tail -50
   
   # Save outputs for comparison
   cargo test > output1.log 2>&1
   # Make changes
   cargo test > output2.log 2>&1
   diff output1.log output2.log
   ```

### Dependencies Available
The project includes dependencies for:
- **Distributed Systems**: tokio, async-trait, futures
- **Consensus**: raft, raft-proto
- **Storage**: redb, serde
- **Testing**: proptest, stateright, fail (fault injection), madsim (deterministic simulation)
- **Networking**: tonic, prost, hyper, madsim-tonic
- **Observability**: tracing, metrics

### Testing Infrastructure
- **MadSim**: Deterministic simulation testing with controlled time and network
- **Proptest**: Property-based testing with automatic input generation  
- **Stateright**: Model checking for distributed system properties
- **Failpoints**: Fault injection for testing error handling and recovery
- **Common utilities**: Shared test helpers in `tests/common/`

## Recently Fixed Issues

1. **Three-Node Cluster Formation** (✅ FULLY FIXED)
   - Fixed: Configuration state persistence after applying changes
   - Fixed: Storage initialization for joining nodes
   - Fixed: Message buffering during peer connection establishment
   - Fixed: Peer discovery - joining nodes now learn about the leader
   - Fixed: Replication triggering after configuration changes
   - Fixed: Raft panic "not leader but has new msg after advance"
   - Fixed: Leader detection in SharedNodeState
   - Basic three-node cluster test now passes reliably
   - See `THREE_NODE_CLUSTER_DEBUG.md` for detailed investigation and solutions

2. **Database Persistence Test Cleanup** (✅ FIXED)
   - Fixed: Database file locking issues during node restarts
   - Fixed: Incomplete component shutdown leaving database references
   - Fixed: Property tests using hardcoded paths instead of temporary directories
   - Solution: Added `shutdown_components()` method to properly clean up all database references
   - Solution: Updated `stop()` method to ensure complete shutdown with file lock release
   - Test `test_database_persistence_across_restarts` now passes consistently

3. **Test-Helpers Feature Compilation** (✅ FIXED)
   - Fixed: Three-node cluster tests failing to compile due to missing test-helpers feature
   - Fixed: Integration tests couldn't access `test_helpers` module
   - Solution: Added `#![cfg(feature = "test-helpers")]` to test files
   - Solution: Configured example to require feature in Cargo.toml
   - Tests must be run with `--features test-helpers` flag

4. **PropTest Failures and Node Lifecycle** (✅ FIXED)
   - Fixed: `test_vm_commands_across_node_states` expecting VM commands to work after `stop()`
   - Fixed: `test_cluster_operations_uninitialized` expecting some operations to succeed
   - Solution: Added `is_initialized` state tracking to `SharedNodeState`
   - Solution: Added initialization checks to `join_cluster()`, `leave_cluster()`, and `get_cluster_status()`
   - Solution: Set `is_initialized` to true at the end of `Node::initialize()`
   - Design clarification: `stop()` is a complete shutdown, not a pause - clears all components to release resources
   - All PropTest suites now pass reliably

## Known Test Reliability Issues

See `TEST_RELIABILITY_ISSUES.md` for full details. Major issues:

1. **Three-node cluster tests are flaky** - Timing-dependent convergence issues
2. **25+ hardcoded sleep() calls** - Should use condition-based waiting
3. **Test workarounds required** - E.g., sending health checks to trigger log replication
4. **~70-90% test reliability** - Tests may fail intermittently under load

When working on tests:
- Use `test_helpers::TestNode` and `TestCluster` abstractions
- Use `timing::wait_for_condition_with_backoff()` instead of `sleep()`
- All tests are enabled - fix failures rather than disabling tests
- Be aware that multi-node tests may fail intermittently

## Future Implementation Areas

Based on the current foundation, the following areas need implementation:
1. **Multi-Node Clustering** - Debug remaining join request processing issues
2. **VM Orchestration** - MicroVM lifecycle management via microvm.nix
3. **Log Compaction** - Implement Raft log compaction using snapshot infrastructure
4. **Metrics & Observability** - Performance monitoring and tracing
5. **Network Partition Handling** - Production-grade fault tolerance
6. **Dynamic Membership** - Safe cluster reconfiguration

## AI Development Guidelines

### What to Build On
- Use existing error types from `src/error.rs`
- Follow CLI patterns from `src/main.rs`
- Implement gRPC services per `proto/blixard.proto`
- Use domain types from `src/types.rs`
- Extend storage functionality in `src/storage.rs`
- Add VM operations to `src/vm_manager.rs`

### What AI Must NEVER Do
- Change the gRPC protocol without approval
- Remove existing error handling patterns
- Alter the CLI structure without discussion
- Commit secrets or configuration files with sensitive data

### External Reference Usage

When working with external libraries and debugging issues:

1. **Consult Documentation Early**
   - Check official docs before implementing features
   - Review library examples before writing code
   - Look for migration guides when updating dependencies

2. **Reference Canonical Implementations**
   - Study how the library authors use their own code
   - Check test files in the library repository
   - Look for production usage in well-known projects

3. **Include References in Code and Docs**
   - Add links to relevant documentation in comments
   - Include example references in debugging documents
   - Link to specific versions to avoid confusion with API changes

4. **Common External References for This Project**
   - Raft: https://docs.rs/raft/latest/raft/
   - Raft Examples: https://github.com/tikv/raft-rs/tree/master/examples
   - Tonic/gRPC: https://docs.rs/tonic/
   - MadSim: https://docs.rs/madsim/latest/madsim/

### Error Handling Pattern
```rust
use crate::error::{BlixardError, BlixardResult};

fn example_operation() -> BlixardResult<String> {
    // Use structured error types with context
    Err(BlixardError::NotImplemented {
        feature: "example operation".to_string(),
    })
}
```

### Commit Messages for Debugging Work

When committing debugging efforts that don't result in a complete fix:

1. **Structure for Investigation Commits**
   ```
   debug(component): investigate issue description
   
   - What was tried
   - What was discovered
   - What didn't work and why
   - Next steps to try
   
   Created ISSUE_DEBUG.md to track investigation
   References: [relevant docs/issues]
   ```

2. **Document Partial Progress**
   - Use `debug:` or `wip:` prefix for investigation commits
   - Always reference the debugging document created
   - Include external references consulted

3. **Example Debugging Commit**
   ```
   debug(raft): investigate cluster formation timeout
   
   Attempted fixes:
   - Fixed protobuf serialization for Raft messages
   - Added timeout handling for conf changes
   - Enhanced ready state processing
   
   Findings:
   - Entries are committed (index advances) but not in committed_entries()
   - Appears related to applied index tracking
   
   See CLUSTER_FORMATION_DEBUG.md for full investigation
   Refs: https://docs.rs/raft/latest/raft/
   ```