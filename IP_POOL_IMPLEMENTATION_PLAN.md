# IP Pool Management Implementation Plan

## Overview

This document provides a step-by-step implementation plan for adding distributed IP pool management to Blixard, building on the existing Raft consensus and VM management infrastructure.

## Implementation Phases

### Phase 1: Core Data Structures and Storage (2-3 days)

#### 1.1 Add IP Pool Types
- [ ] Add `IpPool`, `IpAllocation`, and `IpPoolCommand` types to `blixard-core/src/types.rs`
- [ ] Add serialization/deserialization tests for new types
- [ ] Add validation methods for IP pool configuration

#### 1.2 Extend Storage Layer
- [ ] Add IP pool tables to `blixard-core/src/storage.rs`:
  - `IP_POOL_TABLE` - Store IP pool configurations
  - `IP_ALLOCATION_TABLE` - Store IP allocations by VM name
  - `IP_ALLOCATION_BY_IP_TABLE` - Index for fast IP lookup
- [ ] Add snapshot support for IP pool data in `SnapshotData`
- [ ] Write storage tests for IP pool operations

#### 1.3 Extend Raft Proposal Types
- [ ] Add `IpPoolOperation` variant to `ProposalData` enum
- [ ] Update serialization/deserialization for new proposal type
- [ ] Add tests for Raft proposal encoding/decoding

### Phase 2: IP Pool Manager Component (3-4 days)

#### 2.1 Create IP Pool Manager
- [ ] Create `blixard-core/src/ip_pool_manager.rs`
- [ ] Implement core IP pool manager structure with:
  - Database reference
  - Local cache for performance
  - Initialization from database
- [ ] Add IP pool validation logic:
  - Subnet parsing and validation
  - IP range validation
  - Overlap detection between pools

#### 2.2 Implement Pool Operations
- [ ] `create_pool` - Add new IP pool with validation
- [ ] `delete_pool` - Remove pool (only if no allocations)
- [ ] `list_pools` - Get all configured pools
- [ ] `get_pool_by_id` - Retrieve specific pool details

#### 2.3 Implement IP Allocation Logic
- [ ] `suggest_ip_allocation` - Find available IP without Raft
- [ ] `allocate_ip` - Reserve IP through Raft consensus
- [ ] `release_ip` - Free IP when VM is deleted
- [ ] `get_allocations_by_pool` - List all allocations in a pool
- [ ] `find_ip_pool` - Determine which pool an IP belongs to

#### 2.4 Cache Management
- [ ] Implement cache rebuild from database
- [ ] Add cache invalidation on Raft updates
- [ ] Add metrics for cache hit/miss rates
- [ ] Implement periodic cache validation

### Phase 3: Raft Integration (2-3 days)

#### 3.1 Extend Raft State Machine
- [ ] Add `apply_ip_pool_command` method to `RaftManager`
- [ ] Implement command handlers:
  - `apply_create_pool`
  - `apply_delete_pool`
  - `apply_allocate_ip`
  - `apply_release_ip`
- [ ] Add transaction rollback for failed operations

#### 3.2 Integrate with VM Creation
- [ ] Modify `apply_vm_command` to validate IP allocation
- [ ] Add automatic IP allocation for VMs without specified IP
- [ ] Implement batch proposals for atomic VM + IP creation
- [ ] Add IP release on VM deletion

#### 3.3 Add Validation
- [ ] Prevent duplicate IP allocations
- [ ] Validate IP belongs to specified pool
- [ ] Check pool capacity before allocation
- [ ] Add topology-aware pool selection

### Phase 4: Backend Integration (2-3 days)

#### 4.1 Update VM Backend Interface
- [ ] Add IP pool configuration to `VmBackend` trait
- [ ] Update `create_vm` to accept allocated IP
- [ ] Add method to query pool information

#### 4.2 Update MicrovmBackend
- [ ] Remove local `IpAddressPool` struct
- [ ] Update `convert_config` to use allocated IP from core config
- [ ] Add pool information lookup for gateway/subnet
- [ ] Update network configuration generation
- [ ] Remove `allocate_vm_ip` and `release_vm_ip` methods

#### 4.3 Mock Backend Updates
- [ ] Update mock backend to simulate IP allocation
- [ ] Add tests for IP pool integration

### Phase 5: CLI Commands (2 days)

#### 5.1 IP Pool Management Commands
- [ ] Add `ip-pool` subcommand to CLI
- [ ] Implement commands:
  - `create` - Create new IP pool
  - `delete` - Remove IP pool
  - `list` - Show all pools
  - `show` - Display pool details and usage
  - `allocations` - List all IP allocations

#### 5.2 VM Command Updates
- [ ] Add `--ip-pool` flag to `vm create` command
- [ ] Add `--ip` flag for manual IP specification
- [ ] Update `vm list` to show allocated IPs
- [ ] Add IP information to `vm show` output

### Phase 6: Testing (3-4 days)

#### 6.1 Unit Tests
- [ ] IP pool CRUD operations
- [ ] IP allocation/release logic
- [ ] Subnet validation and overlap detection
- [ ] Cache consistency tests

#### 6.2 Integration Tests
- [ ] Multi-node IP allocation consistency
- [ ] Concurrent allocation stress tests
- [ ] Network partition handling
- [ ] Pool exhaustion scenarios
- [ ] VM creation with IP allocation

#### 6.3 Property-Based Tests
- [ ] IP allocation never duplicates
- [ ] All allocations within pool ranges
- [ ] Cache consistency with database
- [ ] Topology affinity correctness

### Phase 7: Migration and Deployment (2 days)

#### 7.1 Migration Tools
- [ ] Script to import existing VM IPs into pools
- [ ] Validation tool to check IP consistency
- [ ] Rollback procedure documentation

#### 7.2 Documentation
- [ ] Update architecture documentation
- [ ] Add IP pool management guide
- [ ] Update API documentation
- [ ] Add troubleshooting guide

#### 7.3 Observability
- [ ] Add Prometheus metrics:
  - `ip_pool_capacity_total`
  - `ip_pool_allocated_count`
  - `ip_pool_allocation_duration`
  - `ip_pool_allocation_errors`
- [ ] Add OpenTelemetry spans for allocation flow
- [ ] Create Grafana dashboard for IP pool monitoring

## Testing Strategy

### Test Scenarios

1. **Happy Path**:
   - Create pool → Allocate IPs → Create VMs → Delete VMs → IPs released

2. **Edge Cases**:
   - Pool exhaustion
   - Overlapping pool creation attempts
   - Invalid subnet configurations
   - Allocation during network partition

3. **Failure Scenarios**:
   - Node crash during allocation
   - Raft leader change during allocation
   - Database corruption recovery
   - Concurrent allocations for same IP

4. **Performance Tests**:
   - Allocate 1000 IPs across 10 pools
   - Concurrent allocations from multiple nodes
   - Cache performance under load

## Rollout Strategy

1. **Alpha Testing** (Week 1):
   - Deploy to test cluster
   - Run integration tests
   - Performance benchmarking

2. **Beta Testing** (Week 2):
   - Enable for non-production workloads
   - Monitor for allocation conflicts
   - Gather feedback on CLI UX

3. **Production Rollout** (Week 3):
   - Gradual rollout by datacenter
   - Monitor metrics closely
   - Have rollback plan ready

## Success Criteria

- Zero duplicate IP allocations in production
- IP allocation latency < 100ms (p99)
- 100% cache hit rate for read operations
- Successful handling of 1000+ concurrent VMs
- Clean migration of existing VMs to IP pools

## Risk Mitigation

1. **Risk**: Performance regression from Raft consensus
   - **Mitigation**: Batch allocations, optimize cache usage

2. **Risk**: IP allocation conflicts during migration
   - **Mitigation**: Lock pools during migration, validate thoroughly

3. **Risk**: Pool exhaustion blocking VM creation
   - **Mitigation**: Monitoring alerts, automatic pool expansion (future)

4. **Risk**: Cache inconsistency
   - **Mitigation**: Periodic validation, rebuild on discrepancy

## Dependencies

- Existing Raft consensus mechanism
- VM state management through Raft
- Database storage layer (redb)
- Network configuration in microvm.nix

## Timeline

Total estimated time: 14-19 days

- Phase 1: 2-3 days
- Phase 2: 3-4 days  
- Phase 3: 2-3 days
- Phase 4: 2-3 days
- Phase 5: 2 days
- Phase 6: 3-4 days
- Phase 7: 2 days

With parallel work on some phases, the implementation could be completed in approximately 3 weeks.