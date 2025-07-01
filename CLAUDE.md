# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Blixard is a distributed microVM orchestration platform being built in Rust. The project has been reset to a clean state and is currently in early development.

## Current Project State

### Technology Stack
- **Rust** - Core implementation language
- **Iroh P2P** - Peer-to-peer networking with built-in encryption via QUIC
- **Clap** - CLI argument parsing
- **Raft** - Distributed consensus using [raft crate](https://crates.io/crates/raft)
- **microvm.nix** - VM management via [microvm.nix](https://github.com/astro/microvm.nix)
- **Comprehensive Dependencies** - Ready for distributed systems development

### Current Architecture
- **CLI with Iroh P2P** - Full node command with Iroh P2P server in `src/main.rs`
- **Error Types** - Comprehensive error handling defined in `src/error.rs`
- **Domain Types** - Core types (NodeConfig, VmConfig, VmStatus) in `src/types.rs`
- **Iroh Services** - P2P service definitions with QUIC transport
- **Iroh Server** - P2P implementation in `src/iroh_server.rs`
- **Node Structure** - Node management and coordination in `src/node.rs`
- **Node Shared State** - Thread-safe shared state with peer management in `src/node_shared.rs`
- **Storage Layer** - Raft storage implementation in `src/storage.rs`
- **VM Manager** - VM lifecycle and state management in `src/vm_manager.rs`
- **Peer Connector** - Manages Iroh P2P connections to cluster peers in `src/peer_connector.rs`
- **microvm.nix Integration** - Complete VM backend with Nix flake generation and process management in `blixard-vm/`

### Implementation Status
Recent progress:
- ✅ Node CLI command with Iroh P2P server startup
- ✅ Basic Iroh P2P service implementation
- ✅ **Iroh Transport Migration** - Complete migration from gRPC to Iroh P2P with built-in TLS via QUIC
- ✅ MadSim integration for deterministic testing
- ✅ Tonic 0.12 upgrade for compatibility
- ✅ Raft consensus implementation (complete with state machine)
- ✅ Distributed storage (redb integrated)
- ✅ Task scheduling with resource requirements
- ✅ Worker management and health monitoring
- ✅ Multi-node cluster formation (JoinCluster/LeaveCluster P2P messages)
- ✅ Peer management and dynamic configuration changes
- ✅ Peer connection management with automatic reconnection
- ✅ Single-node cluster bootstrap with proper Raft initialization
- ✅ **Raft snapshot support** - Full implementation for state transfer to lagging nodes
- ✅ **Configuration reconstruction** - Fixes for joining nodes with incomplete state
- ✅ **Test reliability improvements** - **Systematically replaced 43 of 76 sleep() calls with condition-based waiting**
- ✅ **Worker registration system** - Automatic registration and capacity tracking
- ✅ **Raft proposal pipeline** - Fixed hanging task submissions
- ✅ **State machine snapshot application** - Implemented missing `apply_snapshot()` method
- ✅ **Snapshot testing** - Comprehensive test coverage for snapshot functionality
- ✅ **Raft consensus enforcement** - Fixed VM manager and worker registration to use Raft
- ✅ **microvm.nix Integration** - Complete Phase 1 & 2 implementation with working VM builds
- ✅ **VM Networking & Systemd Services** - Full routed networking with multi-queue tap interfaces and user systemd services

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
cargo nt-all              # Run all tests with test-helpers feature (main workspace only)
cargo nt-ci               # Run with CI profile
cargo nt-stress           # Run stress test profile (no retries, find real failure rate)
cargo nt-retry            # Run with retries enabled, no fail-fast
cargo nt-verbose          # Run with output capture disabled

# MadSim Tests (deterministic distributed systems testing)
# Option 1: Shell functions (available in nix develop shell)
mnt-all               # Run all MadSim tests (auto-sets RUSTFLAGS)
mnt-byzantine         # Run Byzantine failure tests  
mnt-clock-skew        # Run clock skew tests
madsim-all            # Alternative name for mnt-all

# Option 2: Using convenience script
./scripts/test-madsim.sh all        # Run all MadSim tests
./scripts/test-madsim.sh byzantine  # Run Byzantine failure tests  
./scripts/test-madsim.sh clock-skew # Run clock skew tests

# Option 3: Direct cargo commands (require RUSTFLAGS="--cfg madsim")
RUSTFLAGS="--cfg madsim" cargo nt-madsim     # Run all MadSim tests
RUSTFLAGS="--cfg madsim" cargo nt-byzantine  # Run Byzantine failure tests
RUSTFLAGS="--cfg madsim" cargo nt-clock-skew # Run clock skew tests
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

The test suite is organized into two distinct categories:

#### Unit Tests (`tests/` directory - 171 tests)
Fast, reliable unit tests for individual components:
```bash
# Run all unit tests
cargo test --features test-helpers      # All unit and component tests
cargo nt-all                           # Using nextest for better output

# Specific test categories
cargo test --test cli_tests         # CLI command parsing tests
cargo test --test error_tests       # Error handling tests  
cargo test --test node_tests        # Single node functionality tests
cargo test --test types_tests       # Type serialization tests
cargo test --test storage_tests     # Database persistence tests

# Property-based testing with PropTest
cargo test --test error_proptest    # Error property validation
cargo test --test types_proptest    # Type constraint properties
cargo test --test node_proptest     # Node behavior properties
cargo test --test proptest_example  # Original domain properties

# Model checking with stateright
cargo test --test stateright_simple_test
```

#### Distributed System Tests (`simulation/tests/` directory - 28 test files)
Deterministic simulation tests for distributed behaviors:
```bash
# MadSim deterministic simulation (FULLY DETERMINISTIC)
mnt-all                            # Run ALL 28 simulation test files (recommended)
./scripts/test-madsim.sh all       # Alternative: run all simulation tests
MADSIM_TEST_SEED=12345 mnt-all    # Reproduce specific test run

# Specific simulation test suites
mnt-byzantine                      # Run Byzantine failure tests only
mnt-clock-skew                     # Run clock skew tests only
cd simulation && cargo test three_node_cluster  # Run specific test file

# Determinism verification tools
./scripts/verify-determinism.sh    # Comprehensive determinism audit
./scripts/demo-determinism.sh      # Simple determinism demonstration
```

**Moved to Simulation** (20 test files for deterministic execution):

First wave (8 files):
- `three_node_cluster_tests.rs` - Multi-node cluster formation
- `distributed_storage_consistency_tests.rs` - Replication consistency  
- `cluster_integration_tests.rs` - Cluster-wide operations
- `node_lifecycle_integration_tests.rs` - Task distribution
- `join_cluster_config_test.rs` - Dynamic membership
- `three_node_manual_test.rs` - Manual cluster testing
- `raft_quick_test.rs` - Basic Raft consensus
- `raft_proptest.rs` - Raft property testing

Second wave (12 files):
- `network_partition_storage_tests.rs` - Network partition scenarios
- `storage_performance_benchmarks.rs` - Distributed storage benchmarks
- `cli_cluster_commands_test.rs` - Cluster CLI command testing
- `iroh_service_tests.rs` - Iroh P2P service interactions
- `snapshot_tests.rs` - Raft snapshot functionality
- `raft_state_machine_tests.rs` - Raft state machine testing
- `port_allocation_stress_test.rs` - Concurrent port allocation
- `test_isolation_verification.rs` - Test resource cleanup
- `peer_management_tests.rs` - Peer connection management
- `peer_connector_tests.rs` - Connection lifecycle testing
- `peer_connector_proptest.rs` - Peer connection properties
- `storage_edge_case_tests.rs` - Distributed edge cases (large state transfer, validation)

These tests benefit from:
- **Deterministic execution** - Same seed produces identical results
- **Controlled timing** - No wall-clock dependencies
- **Network simulation** - Partitions, delays, Byzantine failures
- **Faster execution** - Simulated time runs instantly

### Test Infrastructure Status
- **✅ Clean Separation**: Tests now properly separated by type
  - **Unit Tests** (171 tests): Fast, reliable tests in `tests/` - ALL PASSING ✅
  - **Distributed Tests** (28 test files): All in deterministic `simulation/tests/`
    - 8 original simulation tests (Byzantine, clock skew, Raft, etc.)
    - 20 moved distributed tests from main suite
- **✅ Comprehensive Unit Tests**: Core functionality coverage
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
# Node management (implemented with Iroh P2P server)
cargo run -- node --id 1 --bind 127.0.0.1:7001 --data-dir ./data

# VM management (fully functional via Iroh P2P and systemd)
cargo run -- vm create --name my-vm
cargo run -- vm start --name my-vm
cargo run -- vm list
cargo run -- vm stop --name my-vm

# Direct systemd service management
systemctl --user {start|stop|status} blixard-vm-{vm-name}

# microvm.nix testing and examples
cargo test -p blixard-vm                    # Run microvm.nix tests
cargo run --example vm_lifecycle            # Run VM lifecycle example
nix build ./generated-flakes/example-vm#nixosConfigurations.example-vm.config.microvm.runner.cloud-hypervisor
```

## VM Networking Setup

### Host Network Configuration

For VMs to use routed networking with tap interfaces, the host system requires proper network setup:

#### 1. User Group Membership
Ensure the user is in the required groups:
```bash
sudo usermod -a -G kvm,blixard $USER
# Log out and back in for group changes to take effect
```

#### 2. Tap Interface Creation
For each VM, create a corresponding tap interface with multi-queue support:
```bash
# Example for vm12 (used by test-vm)
sudo ip link delete vm12 2>/dev/null || true
sudo ip tuntap add dev vm12 mode tap group kvm multi_queue
sudo ip link set dev vm12 up
sudo ip addr add "10.0.0.0/32" dev vm12
sudo ip route add "10.0.0.12/32" dev vm12
```

#### 3. TUN/TAP Device Permissions
Verify TUN device permissions allow group access:
```bash
ls -la /dev/net/tun  # Should show: crw-rw-rw- or crw-rw-r--
```

#### 4. Bulk Interface Setup Script
Use the setup script for multiple interfaces:
```bash
./scripts/setup-tap-networking.sh  # Creates vm1-vm64 interfaces
```

### VM Configuration Requirements

VMs using routed networking need specific configuration in their flake.nix:

```nix
microvm = {
  hypervisor = "qemu";
  vcpu = 2;  # Multi-queue interfaces require matching queue count
  mem = 1024;
  
  interfaces = [ {
    type = "tap";
    id = "vm12";  # Must match host tap interface name
    mac = "02:00:00:00:0c:01";  # Unique MAC per VM
  } ];
  
  # Console socket for debugging
  socket = "/tmp/test-vm-console.sock";
};

# Network configuration inside VM
systemd.network.networks."10-eth" = {
  matchConfig.MACAddress = "02:00:00:00:0c:01";
  address = [ "10.0.0.12/32" ];  # VM's routed IP
  routes = [
    { Destination = "10.0.0.0/32"; GatewayOnLink = true; }
    { Destination = "0.0.0.0/0"; Gateway = "10.0.0.0"; GatewayOnLink = true; }
  ];
};
```

### Common Networking Issues

1. **"could not configure /dev/net/tun (vm12): Invalid argument"**
   - **Cause**: Tap interface lacks multi-queue support or wrong group ownership
   - **Fix**: Recreate interface with `multi_queue` flag and correct group

2. **"Read-only file system" during systemd service creation**
   - **Cause**: Code trying to access system directories for user services
   - **Fix**: Ensure all systemd operations use `--user` and `~/.config/systemd/user/`

3. **Permission denied accessing tap interface**
   - **Cause**: User not in kvm/blixard groups or TUN device permissions
   - **Fix**: Add user to groups and check `/dev/net/tun` permissions

4. **VM boots but no network connectivity**
   - **Cause**: Missing host routes or incorrect VM IP configuration
   - **Fix**: Verify host routes and VM systemd.network configuration match

### Debugging Network Issues

```bash
# Check tap interface status
ip link show vm12
cat /sys/class/net/vm12/tun_flags  # Should show multi-queue flags

# Check VM systemd service status
systemctl --user status blixard-vm-test-vm

# Monitor VM console output
journalctl --user -u blixard-vm-test-vm -f

# Test connectivity
ping 10.0.0.12
ssh root@10.0.0.12
```

## Key Files

### Core Implementation
- `src/main.rs` - CLI entry point and command parsing
- `src/lib.rs` - Library root
- `src/error.rs` - Error type definitions
- `src/types.rs` - Domain types and data structures
- `src/node.rs` - Node lifecycle and coordination
- `src/iroh_server.rs` - Iroh P2P service implementation
- `src/storage.rs` - Raft storage backend
- `src/vm_manager.rs` - VM state management
- `src/iroh_transport.rs` - Iroh P2P transport layer

### microvm.nix Integration (`blixard-vm/`)
- `src/microvm_backend.rs` - Main VM backend implementation using microvm.nix
- `src/nix_generator.rs` - Nix flake generation using Tera templates
- `src/process_manager.rs` - VM process lifecycle management
- `src/types.rs` - Enhanced VM configuration types
- `nix/templates/vm-flake.nix` - Nix flake template for VM generation
- `examples/vm_lifecycle.rs` - Example demonstrating VM operations
- `tests/backend_integration_test.rs` - Integration tests for VM backend

### Configuration
- `Cargo.toml` - Dependencies configured for distributed systems
- `rust-toolchain.toml` - Rust version specification
- `build.rs` - Protocol buffer compilation
- `flake.nix` - Nix development environment

## Development Guidelines

### Core Rules
1. Follow existing error handling patterns in `src/error.rs`
2. Use structured types from `src/types.rs`
3. Implement Iroh P2P services with proper message handling
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
- **Networking**: iroh, iroh-net (QUIC transport), postcard (message serialization)
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

5. **Raft Consensus Bypass Issues** (✅ FULLY FIXED)
   - Fixed: VM Manager was writing directly to database, bypassing Raft consensus
   - Fixed: Worker registration was bypassing Raft for non-bootstrap scenarios
   - Fixed: Non-leader nodes weren't updating configuration after RemoveNode operations
   - Solution: Transformed VM Manager into a stateless executor
   - Solution: Added `register_worker_through_raft()` for proper worker registration
   - Solution: Non-leaders now update conf state directly from log entries
   - Solution: `get_cluster_status` now uses authoritative Raft configuration
   - All distributed state changes now go through Raft consensus
   - See `CONSENSUS_AND_TEST_FIXES_SUMMARY.md` and `plan.md` for detailed implementation notes

6. **Test Suite Stability** (✅ FIXED) 
   - Fixed: PropTest concurrent operations failing due to duplicate peer IDs
   - Fixed: Storage performance benchmark snapshot transfer errors
   - Fixed: Large state transfer test encountering node ID conflicts
   - Solution: Added uniqueness filter to peer_info_strategy in PropTests
   - Solution: Reduced state sizes and added batching for edge case tests
   - Solution: Improved error handling in benchmarks
   - 99.5% test success rate (189/190 tests passing)
   - See `TEST_STATUS_SUMMARY.md` for detailed test status

7. **Hollow Test Elimination** (✅ FIXED)
   - Fixed: `raft_quick_test.rs` only checking completion without verifying Raft behavior
   - Fixed: `storage_edge_case_tests.rs` logging results without assertions
   - Solution: Added proper assertions to verify distributed system correctness
   - Solution: Tests now verify leader election, consensus, and input validation
   - Result: Improved tests caught real bugs in Raft implementation
   - **76 total sleep() calls identified**, 43 fixed (57%), 33 remaining (43%)

## Test Infrastructure Status

See `TEST_RELIABILITY_ISSUES.md` and `TEST_SUITE_SLEEP_ELIMINATION_FINAL.md` for details on the improvements made:

**✅ FIXED Issues:**
1. **76 hardcoded sleep() calls** - 48 replaced with condition-based waiting (63% complete)
2. **All raw `tokio::time::sleep()` calls eliminated** - 0 remaining
3. **Race condition with removed nodes** - Messages from removed nodes no longer crash Raft
4. **Raft manager recovery** - Automatic restart with exponential backoff on failures
5. **Test reliability improved** - Most tests now pass consistently
6. **Hollow tests eliminated** - Tests now verify actual distributed system behavior instead of just completion
7. **Edge case validation** - Tests verify proper input validation and error handling

**Recent Improvements:**
- `raft_quick_test.rs`: Now verifies leader election and Raft consensus behavior
- `storage_edge_case_tests.rs`: Added assertions for memory pressure and invalid input handling
- `three_node_cluster_tests.rs`: Replaced sleep with condition-based waiting for replication
- **5 additional test files fixed**: `cluster_integration_tests.rs`, `three_node_manual_test.rs`, `node_proptest.rs`, `node_tests.rs`, `common/raft_test_utils.rs`
- All remaining sleep calls use environment-aware `timing::robust_sleep()` with 3x CI scaling

**Remaining Work:**
- Add Byzantine failure tests for malicious node behavior
- Add clock skew tests for time synchronization edge cases
- Some node tests marked as `#[ignore]` need updating to use TestCluster for Raft consensus
- Performance benchmarks need adjustment for new architecture

When working on tests:
- Use `test_helpers::TestNode` and `TestCluster` abstractions
- **NEVER use hardcoded sleep()** - Use `timing::wait_for_condition_with_backoff()` instead
- Tests requiring Raft consensus must use TestCluster, not direct Node operations
- **Always include meaningful assertions** - tests should verify actual behavior, not just completion
- Edge case tests must verify proper error handling and input validation
- Performance tests should include assertions about system responsiveness under load

### Test Suite Sleep Elimination Progress ✅
**MAJOR ACHIEVEMENT**: Systematically replaced 48 of 76 hardcoded sleep() calls with robust condition-based waiting:

**✅ COMPLETED FILES (12 critical test files)**:
- `peer_connector_tests.rs` (17 calls) - Connection lifecycle with condition-based waiting
- `test_isolation_verification.rs` (9 calls) - Resource cleanup with environment-aware timing
- `distributed_storage_consistency_tests.rs` (7 calls) - Replication verification
- `three_node_cluster_tests.rs` (9 calls) - Now uses condition-based waiting for cluster state
- `storage_performance_benchmarks.rs` (5 calls) - Environment-aware timing for CI compatibility  
- `node_lifecycle_integration_tests.rs` (4 calls) - Robust lifecycle synchronization
- `cli_cluster_commands_test.rs` (2 calls) - Reliable CLI test timing
- `cluster_integration_tests.rs` (1 call) - Configuration propagation verification
- `three_node_manual_test.rs` (5 calls) - Complete condition-based cluster formation
- `node_proptest.rs` (1 call) - Property test timing improvements
- `node_tests.rs` (2 calls) - Concurrent access and VM lifecycle timing
- `common/raft_test_utils.rs` (1 call) - Leader election waiting

**📊 IMPACT**:
- **100% test success rate** for improved files
- **Zero raw `tokio::time::sleep()` calls remaining**
- **Tests catch real bugs** instead of hoping timeouts are sufficient
- **CI reliability** - Automatic 3x timeout scaling in CI environments

**✅ COMPLETED MILESTONE**: All raw sleep calls eliminated. Remaining 28 calls use `timing::robust_sleep()` for legitimate timing needs.

**Framework established** for all timing operations:
- `timing::wait_for_condition_with_backoff()` - Exponential backoff with environment-aware timeouts
- `timing::robust_sleep()` - Environment-aware sleep for necessary delays
- `timing::scaled_timeout()` - Automatic timeout scaling for CI environments

## Implementation Status Audit (2025-01-25)

### ✅ Completed Features

1. ✅ **Multi-Node Clustering** - Fully working with robust join/leave operations
2. ✅ **VM Orchestration** - MicroVM lifecycle management via microvm.nix (Phase 1 & 2)
3. ✅ **VM Scheduler** - Complete implementation with multiple placement strategies:
   - Resource-aware placement (CPU, memory, disk)
   - Multiple strategies: MostAvailable, LeastAvailable, RoundRobin, Manual
   - Feature requirement matching (gpu, microvm, etc.)
   - Integration with Raft consensus for distributed decisions
   - Full gRPC API: CreateVmWithScheduling, ScheduleVmPlacement
4. ✅ **Log Compaction** - Automatic Raft log compaction:
   - Triggers at 1000 applied entries
   - Creates snapshots and truncates old logs
   - Prevents compaction thrashing with 500-entry hysteresis
   - Full state preservation in snapshots
5. ✅ **Network Partition Handling** - Comprehensive testing via MadSim:
   - True network partition simulation (not just node removal)
   - Split-brain prevention verification
   - Partition healing and reconciliation
   - Message-level network control
6. ✅ **Byzantine Failure Testing** - Extensive malicious behavior tests:
   - Wrong term messages, multiple votes, fake leaders
   - Message replay attacks, identity forgery
   - Clock skew tolerance testing
7. ✅ **Dynamic Membership** - Safe cluster reconfiguration via Raft

### 🔧 Partially Implemented

1. **Metrics & Observability**
   - ✅ OpenTelemetry metrics foundation (metrics_otel_v2.rs)
   - ✅ Comprehensive metric definitions for all components
   - ✅ HTTP /metrics endpoint (metrics_server.rs)
   - ✅ Prometheus metrics exposition format
   - ✅ Distributed tracing with OpenTelemetry spans (tracing_otel.rs)
   - ✅ P2P trace context propagation
   - ✅ Components instrumented: RaftManager, PeerConnector, Storage, Iroh P2P, VM operations
   - ✅ Grafana dashboards with 60+ panels (monitoring/grafana/dashboards/blixard-comprehensive.json)
   - ✅ Production alerting rules for Prometheus (monitoring/prometheus/alerts/blixard-alerts.yml)
   - ✅ OTLP export configuration for cloud vendors (AWS, GCP, Azure, Datadog)
   - ✅ Exemplar support for trace-to-metrics correlation
   - ✅ Operational runbooks for critical alerts (docs/runbooks/)

2. **Security & Authentication**
   - ✅ Built-in encryption via Iroh's QUIC transport (Ed25519 node keys)
   - ✅ Token-based authentication with SHA256 hashing
   - ✅ AWS Cedar Policy Engine for authorization (replaces RBAC)
   - ✅ Certificate-based enrollment for secure node onboarding
   - ✅ Authentication interceptor for automatic request validation
   - ✅ Secure secrets management with AES-256-GCM encryption
   - ✅ Comprehensive security documentation (docs/security/)

### 📋 Future Implementation Areas

1. **Production Hardening**
   - ✅ Configuration management (TOML-based with hot-reload support)
   - ✅ Security: Built-in QUIC encryption, Cedar authorization
   - Resource limits and quotas per tenant
   - Backup and disaster recovery procedures
   - Additional operational runbooks for non-critical alerts

2. **Performance Optimization**
   - Connection pooling for Iroh P2P clients
   - Batch processing for Raft proposals
   - Caching layer for frequently accessed data
   - Profiling and optimization of hot paths

4. **Advanced Scheduling Features**
   - Anti-affinity rules for HA deployments
   - Resource reservations and overcommit policies
   - Priority-based scheduling and preemption
   - Multi-datacenter awareness
   - Cost-aware placement strategies

### microvm.nix Integration Status ✅

**COMPLETED** - Phases 1 & 2 of microvm.nix integration:
- ✅ **Phase 1**: Basic flake-parts module structure with comprehensive VM options
- ✅ **Phase 2**: Process management with lifecycle operations (start/stop/status)
- ✅ **Template System**: Tera-based Nix flake generation that produces working builds
- ✅ **Testing Infrastructure**: Unit tests, integration tests, and example programs
- ✅ **Documentation**: Complete implementation docs and usage examples

**Next Phase** (Phase 3): 
- Integration with Raft consensus for distributed VM management
- VM scheduler for automatic placement decisions based on resource availability
- Health monitoring and auto-recovery for VM processes


## AI Development Guidelines

### What to Build On
- Use existing error types from `src/error.rs`
- Follow CLI patterns from `src/main.rs`
- Implement Iroh P2P services with proper message handling
- Use domain types from `src/types.rs`
- Extend storage functionality in `src/storage.rs`
- Add VM operations to `src/vm_manager.rs`

### What AI Must NEVER Do
- Change the Iroh P2P protocol without approval
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
   - Iroh: https://docs.rs/iroh/
   - Iroh-net: https://docs.rs/iroh-net/
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

## microvm.nix Integration Implementation

### Architecture Overview

The microvm.nix integration provides complete VM lifecycle management through three main components:

1. **NixFlakeGenerator** (`blixard-vm/src/nix_generator.rs`)
   - Generates Nix flakes from VM configurations using Tera templates
   - Converts Blixard VM types to microvm.nix configuration format
   - Handles network interfaces, volumes, and NixOS module configuration

2. **VmProcessManager** (`blixard-vm/src/process_manager.rs`)
   - Manages VM process lifecycle (start/stop/status/kill)
   - Mock-friendly design with CommandExecutor trait for testing
   - Tracks running VMs and provides graceful shutdown with timeouts

3. **MicrovmBackend** (`blixard-vm/src/microvm_backend.rs`)
   - Main implementation of VmBackend trait from blixard-core
   - Orchestrates flake generation and process management
   - Integrates with distributed Raft consensus for VM state

### Configuration System

Enhanced VM configuration types in `blixard-vm/src/types.rs`:

```rust
pub struct VmConfig {
    pub name: String,
    pub hypervisor: Hypervisor,  // CloudHypervisor, Firecracker, Qemu
    pub vcpus: u32,
    pub memory: u32,
    pub networks: Vec<NetworkConfig>,  // Tap, User networking
    pub volumes: Vec<VolumeConfig>,    // RootDisk, DataDisk, VirtioFS
    pub nixos_modules: Vec<NixModule>, // File, FlakePart, Inline
    pub flake_modules: Vec<String>,    // flake-parts module references
    pub kernel: Option<KernelConfig>,
    pub init_command: Option<String>,
}
```

### Template System

The Nix flake template (`blixard-vm/nix/templates/vm-flake.nix`) generates working NixOS configurations:

- **Standard NixOS Structure**: Uses `nixpkgs.lib.nixosSystem` (not flake-parts)
- **microvm.nix Integration**: Includes microvm.nixosModules.microvm
- **Fixed Configuration**: Hardcoded network and volume settings for reliability
- **Basic NixOS Setup**: Auto-login root user, systemd services, state version

### Testing Strategy

Comprehensive test coverage across multiple levels:

1. **Unit Tests**: Individual component testing with mocks
2. **Integration Tests**: End-to-end VM lifecycle testing  
3. **Template Validation**: Generated flakes build successfully with Nix
4. **Property Tests**: VM configuration constraint validation

### Usage Examples

```bash
# Run VM lifecycle demonstration
cargo run --example vm_lifecycle --manifest-path blixard-vm/Cargo.toml

# Test VM backend integration
cargo test -p blixard-vm

# Build generated VM with Nix (demonstrates working integration)
nix build ./generated-flakes/example-vm#nixosConfigurations.example-vm.config.microvm.runner.cloud-hypervisor
```

### Implementation Notes

- **Error Handling**: Uses blixard-core error types throughout
- **Async Design**: All operations are async-compatible with tokio
- **Resource Management**: Proper cleanup of VM processes and flake directories
- **Security**: No hardcoded secrets, VM configurations isolated per VM
- **Performance**: Template caching and efficient process tracking