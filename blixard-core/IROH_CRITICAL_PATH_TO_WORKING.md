# Critical Path to Working Blixard with Iroh Transport

## Current State Analysis

The Iroh integration is mostly complete but the system cannot start due to several blocking issues:

1. **Transport Config Mismatch**: The code expects `TransportConfig` to be an enum with `Iroh` and `Dual` variants, but it's now a simple struct
2. **Service Runner Issues**: The `start_iroh_services` function is called but the service infrastructure may not be fully connected
3. **P2P Manager Integration**: The P2P manager needs to be properly initialized for Iroh transport
4. **Missing Service Implementations**: Some services may not be properly wired to work over Iroh

## Priority 1: Fix Compilation Errors (Required for Single Node)

### 1.1 Fix TransportConfig Usage in node.rs
**File**: `src/node.rs` (lines 128-132)
**Issue**: Code expects enum-style TransportConfig but it's now a struct
**Fix**: Change the transport detection logic to check for presence of transport_config instead of matching on enum variants

```rust
// Replace the match statement with:
let using_iroh_transport = self.shared.config.transport_config.is_some();
```

### 1.2 Fix TransportConfig Usage Throughout
**Files to check**: 
- Any file that matches on TransportConfig enum variants
- Update to use the new struct-based approach

## Priority 2: Single Node Startup (Minimum Viable)

### 2.1 Ensure Iroh Service Runner Works
**File**: `src/transport/iroh_service_runner.rs`
**Verify**:
- Services are properly registered (Health, Status, VM, Cluster)
- Endpoint is correctly created and listening
- RPC protocol handler works

### 2.2 P2P Manager Initialization
**File**: `src/p2p_manager.rs`
**Verify**:
- P2P manager creates Iroh endpoint correctly
- Endpoint is accessible for service runner
- Node address is properly stored

### 2.3 Basic Health Check
**Test**: A single node should be able to:
- Start successfully
- Respond to health checks via Iroh RPC
- Show proper status

## Priority 3: Cluster Formation (Multi-Node)

### 3.1 Cluster Service over Iroh
**Files**: 
- `src/transport/iroh_cluster_service.rs`
- `src/transport/cluster_operations_adapter.rs`
**Verify**:
- JoinCluster RPC works over Iroh
- Nodes can discover each other via Iroh node addresses
- Cluster status queries work

### 3.2 Raft Transport over Iroh
**File**: `src/transport/iroh_raft_transport.rs`
**Critical**:
- Raft messages (heartbeats, votes, log entries) flow correctly
- Message prioritization works
- Reliable delivery for consensus

### 3.3 Peer Connection Management
**File**: `src/transport/iroh_peer_connector.rs`
**Ensure**:
- Peers can establish Iroh connections
- Connection health monitoring works
- Reconnection logic functions

## Priority 4: VM Management

### 4.1 VM Service Implementation
**File**: `src/transport/iroh_vm_service.rs`
**Operations**:
- CreateVm
- StartVm
- StopVm
- ListVms
- GetVmStatus

### 4.2 VM Backend Integration
**Verify**:
- VM operations go through Raft consensus
- State is properly synchronized across nodes
- microvm.nix backend works with Iroh transport

## Priority 5: Full System Validation

### 5.1 Integration Tests
- Single node with VM operations
- Three-node cluster formation
- VM scheduling across nodes
- Network partition handling

### 5.2 Performance Validation
- Raft consensus latency
- VM operation throughput
- P2P connection overhead

## Immediate Action Items

1. **Fix Compilation** (30 minutes)
   - Update all TransportConfig usage to work with struct
   - Remove enum matching, use presence checks

2. **Verify Single Node** (1 hour)
   - Start a node with Iroh transport
   - Check logs for service initialization
   - Test health endpoint

3. **Test Cluster Formation** (2 hours)
   - Start 3 nodes with Iroh
   - Verify they can join cluster
   - Check Raft leader election

4. **VM Operations** (2 hours)
   - Create and start a VM
   - Verify it works through Iroh RPC
   - Check state synchronization

## Configuration Example

For testing, use this configuration:

```toml
[transport]
home_relay = "https://relay.iroh.network"
discovery_port = 0
alpn_protocols = []

[p2p]
enabled = true

[node]
id = 1
bind_address = "127.0.0.1:7001"
data_dir = "./data"
vm_backend = "microvm"
```

## Success Criteria

The system is considered working when:
1. ✅ Single node starts without errors
2. ✅ Health checks work via Iroh RPC
3. ✅ Three nodes can form a cluster
4. ✅ Raft consensus works (leader election, log replication)
5. ✅ VMs can be created/started/stopped
6. ✅ State is synchronized across all nodes

## Debugging Commands

```bash
# Start single node with debug logging
RUST_LOG=blixard=debug,blixard_core=debug cargo run -p blixard -- node --id 1 --bind 127.0.0.1:7001

# Check node status
cargo run -p blixard -- cluster status --addr 127.0.0.1:7001

# Test VM creation
cargo run -p blixard -- vm create --name test-vm

# Monitor logs for Iroh activity
RUST_LOG=iroh=debug,blixard_core::transport=trace cargo run -p blixard -- node --id 1 --bind 127.0.0.1:7001
```