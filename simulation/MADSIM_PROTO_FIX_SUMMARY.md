# MadSim Proto/Type Mismatch Fixes

## Summary

Fixed compilation errors in MadSim simulation tests by addressing proto generation and import issues.

## Issues Found and Fixed

### 1. **raft_safety_tests.rs** - EnvFilter compilation error
- **Problem**: Used `tracing_subscriber::EnvFilter` without the "env-filter" feature
- **Fix**: Removed env filter usage, simplified to basic `tracing_subscriber::fmt().try_init()`
- **Also fixed**: Removed unused imports (`warn` from tracing, `blixard_simulation::*`)

### 2. **build.rs** - Proto compilation configuration  
- **Problem**: Used complex output directory structure (`sim/` subdirectory) incompatible with madsim-tonic-build
- **Fix**: Simplified to use `tonic_build::compile_protos("proto/simulation.proto")` directly
- **Also fixed**: Changed from array parameter to single path string

### 3. **src/lib.rs** - Proto module inclusion
- **Problem**: Attempted to include proto from custom subdirectory path
- **Fix**: Used standard `tonic::include_proto!("blixard")` after simplifying build.rs

## Verification

All MadSim tests now compile successfully:
- 28 test files compile without errors
- Proto types are correctly generated and accessible
- Tests can be run with: `RUSTFLAGS="--cfg madsim" cargo test`

## Proto Structure

The simulation tests use their own `proto/simulation.proto` file that includes:
- All necessary message types for testing (JoinRequest, CreateVmRequest, etc.)
- Additional test-specific messages (RaftMessageRequest, TaskRequest, etc.)
- Service definitions for ClusterService and BlixardService

This allows the simulation tests to be independent of the main codebase's proto definitions while still testing the same protocol semantics.