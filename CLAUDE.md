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
- **Node Structure** - Node management skeleton in `src/node.rs`

### Implementation Status
Recent progress:
- ✅ Node CLI command with gRPC server startup
- ✅ Basic gRPC service implementation
- ✅ MadSim integration for deterministic testing
- ✅ Tonic 0.12 upgrade for compatibility
- 🔧 VM lifecycle management (stubs only)
- 🔧 Raft consensus implementation (storage prepared)
- 🔧 Distributed storage (redb integrated)

## Development Commands

### Build and Test
```bash
cargo build                # Build the project
cargo test                 # Run all tests
cargo test --features simulation    # Run with madsim deterministic simulation
cargo test --features failpoints    # Run with failpoint injection
cargo test --features all-tests     # Run all testing frameworks
cargo run -- --help       # Show CLI help
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

## Future Implementation Areas

Based on the current foundation, the following areas need implementation:
1. **Node Management** - Cluster membership and lifecycle
2. **Raft Consensus** - Distributed consensus using raft crate
3. **VM Orchestration** - MicroVM lifecycle management via microvm.nix
4. **Storage Layer** - Persistent state with redb
5. **Testing Framework** - Deterministic testing setup
6. **gRPC Services** - Server and client implementations

## AI Development Guidelines

### What to Build On
- Use existing error types from `src/error.rs`
- Follow CLI patterns from `src/main.rs`
- Implement gRPC services per `proto/blixard.proto`
- Use domain types from `src/types.rs`

### What AI Must NEVER Do
- Change the gRPC protocol without approval
- Remove existing error handling patterns
- Alter the CLI structure without discussion
- Commit secrets or configuration files with sensitive data

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