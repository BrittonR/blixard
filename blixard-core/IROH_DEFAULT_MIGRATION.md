# Iroh Default Transport Migration

## Overview

This document tracks the migration from gRPC to Iroh as the default transport in Blixard.

## Completed Steps ✅

1. **Updated Default Transport Configuration**
   - Changed `TransportConfig::default()` to return `TransportConfig::Iroh(IrohConfig::default())` 
   - Changed `RaftTransportPreference::default()` to return `RaftTransportPreference::AlwaysIroh`
   - Files modified:
     - `src/transport/config.rs`

2. **Added Transport to Config System**
   - Added `transport: Option<TransportConfig>` to `Config` struct
   - Updated `Config::default()` to include Iroh transport by default
   - Files modified:
     - `src/config_v2.rs`

3. **Updated Node Creation**
   - Modified `main.rs` to pass transport config from Config to NodeConfig
   - Files modified:
     - `blixard/src/main.rs`

4. **P2P Manager Auto-Initialization**
   - Modified node initialization to create P2P manager when using Iroh transport
   - This ensures Iroh endpoint is available for transport even if P2P is disabled
   - Files modified:
     - `src/node.rs`

## Remaining Work 🔧

### 1. Replace PeerConnector with RaftTransportAdapter

Currently, the node still uses `PeerConnector` directly for Raft messages. We need to:

- Replace `PeerConnector` creation with `RaftTransportAdapter` creation
- Update the outgoing message handler to use the transport adapter
- Ensure proper transport selection based on config

**Current code in `node.rs`:**
```rust
// Create peer connector
let peer_connector = Arc::new(PeerConnector::new(self.shared.clone()));

// Start task to handle outgoing Raft messages
let peer_connector_clone = peer_connector.clone();
let _outgoing_handle = tokio::spawn(async move {
    let mut outgoing_rx = outgoing_rx;
    while let Some((to, msg)) = outgoing_rx.recv().await {
        if let Err(e) = peer_connector_clone.send_raft_message(to, msg).await {
            tracing::warn!("Failed to send Raft message to {}: {}", to, e);
        }
    }
    Ok::<(), BlixardError>(())
});
```

**Should be replaced with:**
```rust
// Get transport config
let transport_config = self.shared.config.transport_config
    .as_ref()
    .unwrap_or(&TransportConfig::default());

// Create Raft transport adapter
let (raft_msg_tx, raft_msg_rx) = mpsc::unbounded_channel();
let transport = RaftTransport::new(
    self.shared.clone(),
    raft_msg_tx,
    transport_config
).await?;

// Store transport in shared state (may need to add field)
self.shared.set_raft_transport(transport.clone()).await;

// Start transport maintenance
transport.start_maintenance().await;

// Start task to handle outgoing Raft messages
let transport_clone = transport.clone();
let _outgoing_handle = tokio::spawn(async move {
    let mut outgoing_rx = outgoing_rx;
    while let Some((to, msg)) = outgoing_rx.recv().await {
        if let Err(e) = transport_clone.send_message(to, msg).await {
            tracing::warn!("Failed to send Raft message to {}: {}", to, e);
        }
    }
    Ok::<(), BlixardError>(())
});
```

### 2. Update Service Creation

The gRPC server and other services need to be aware of the transport config to properly route requests. This may involve:

- Passing transport config to service factories
- Creating dual-mode services when needed
- Ensuring proper client creation for inter-node communication

### 3. Update Tests

All tests need to be updated to work with Iroh as the default transport:

- Integration tests may need to set up proper Iroh endpoints
- Mock transports may need updates
- Performance benchmarks should compare Iroh vs gRPC

### 4. Configuration Documentation

Update configuration examples and documentation:

- Default config files should show Iroh transport
- Migration guide for users switching from gRPC
- Performance tuning recommendations

## Testing Plan

1. **Unit Tests**
   - Verify transport config defaults
   - Test transport adapter creation
   - Test message routing

2. **Integration Tests**
   - Multi-node cluster with Iroh transport
   - Mixed transport clusters (some Iroh, some gRPC)
   - Transport switching during runtime

3. **Performance Tests**
   - Benchmark Iroh vs gRPC for Raft messages
   - Test NAT traversal scenarios
   - Measure latency and throughput

## Risks and Mitigations

1. **Breaking Change for Existing Deployments**
   - Risk: Existing clusters using gRPC won't be able to communicate with new Iroh nodes
   - Mitigation: Provide clear migration path and support dual-mode operation

2. **NAT Traversal Complexity**
   - Risk: Iroh's NAT traversal may not work in all network environments
   - Mitigation: Document network requirements and provide fallback options

3. **Performance Regression**
   - Risk: Iroh may have different performance characteristics than gRPC
   - Mitigation: Comprehensive benchmarking and tuning guides

## Next Steps

1. Implement RaftTransportAdapter integration in node.rs
2. Add transport field to SharedNodeState if needed
3. Update service creation to use transport config
4. Create integration tests for Iroh transport
5. Update documentation and examples