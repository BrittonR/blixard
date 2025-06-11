# TODO List

## In Progress

## Pending

### High Priority - Core Infrastructure
- [ ] Create gRPC server implementation for BlixardService in `proto/blixard.proto`
- [ ] Implement Raft consensus integration using raft crate with redb persistence
- [ ] Add comprehensive tests for Node struct (initialization, VM commands, database)

### Medium Priority - VM Management
- [ ] Implement VM lifecycle operations (create, start, stop, delete) via microvm.nix
- [ ] Add VM status tracking and health monitoring
- [ ] Create VM configuration validation and serialization
- [ ] Implement resource allocation and scheduling logic

### Medium Priority - Testing & Reliability
- [ ] Fix failing madsim network tests (3 tests failing)
- [ ] Expand MadSim deterministic tests for network partitions
- [ ] Add failpoint injection tests for error handling
- [ ] Create integration tests for full node cluster scenarios
- [ ] Add property-based tests for Raft consensus edge cases

### Low Priority - Enhancement
- [ ] Add metrics and observability with tracing
- [ ] Implement CLI auto-completion
- [ ] Add configuration file support (TOML/YAML)
- [ ] Create basic web dashboard for cluster monitoring

## Completed

- [x] **Implement basic Node struct and lifecycle management in `src/node.rs`**
  - ✅ Node struct with Raft integration, VM state management, and database storage
  - ✅ RedbRaftStorage - Custom Raft storage implementation using redb
  - ✅ VM command processing - Async channel-based VM lifecycle management  
  - ✅ Database integration - redb for persistent VM state storage
  - ✅ Architecture - Clean separation of concerns with async lifecycle management
  - ✅ Updated dependencies to use `raft` crate and `microvm.nix`
  - ✅ Enhanced error handling with new variants (Storage, Raft, Serialization, Internal, NotImplemented)

## Notes

- **Technology Stack Updated**: Now uses `raft` crate (not tikv-raft-rs) and `microvm.nix` for VM management
- **Node Implementation**: Complete foundation with Raft integration, VM state management, database storage
- **Testing Status**: All main project tests passing (10/10), MadSim cluster tests working (5/5)
- **Next Priority**: gRPC server implementation and Raft persistence layer
- **Architecture**: Clean async patterns with channel-based VM command processing