# P2P Bootstrap Implementation

## Overview

This document describes the implementation of the P2P bootstrap mechanism for Blixard, which enables nodes to join a cluster using Iroh P2P transport without prior knowledge of the leader's Iroh NodeId.

## Problem Statement

When using Iroh P2P transport, nodes need to know each other's Iroh NodeIds (cryptographic identities) to establish connections. However, when a new node wants to join a cluster, it only knows the traditional network address (e.g., `127.0.0.1:7001`) of the leader node, not its Iroh NodeId. This created a bootstrap problem that prevented multi-node cluster formation.

## Solution

We implemented an HTTP bootstrap mechanism that allows nodes to exchange P2P information before establishing the Iroh P2P connection.

### Architecture

1. **Bootstrap Endpoint**: Each node exposes an HTTP endpoint at `/bootstrap` on the metrics server (port offset +1000 from the main port).

2. **Bootstrap Flow**:
   - Joining node queries the leader's bootstrap endpoint
   - Leader returns its P2P information (NodeId, addresses, relay URL)
   - Joining node uses this info to establish P2P connection
   - Normal cluster join proceeds via P2P

### Implementation Details

#### 1. Bootstrap Info Structure (`iroh_types.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapInfo {
    pub node_id: u64,                    // Cluster node ID
    pub p2p_node_id: String,            // Iroh P2P node ID (base64)
    pub p2p_addresses: Vec<String>,     // Direct P2P addresses
    pub p2p_relay_url: Option<String>,  // Optional relay URL
}
```

#### 2. Bootstrap Endpoint (`metrics_server.rs`)

- Extended the existing metrics server to handle `/bootstrap` requests
- Server accepts `SharedNodeState` to access P2P information
- Returns JSON-encoded `BootstrapInfo`

#### 3. Bootstrap Client (`node.rs`)

- When discovery fails, falls back to HTTP bootstrap
- Fetches bootstrap info from `http://<join_addr>:<port+1000>/bootstrap`
- Creates `NodeAddr` from bootstrap info
- Establishes P2P connection before sending join request

### Key Files Modified

1. **`blixard-core/src/iroh_types.rs`**
   - Added `BootstrapInfo` struct

2. **`blixard-core/src/metrics_server.rs`**
   - Added `/bootstrap` endpoint
   - Modified to accept `SharedNodeState`
   - Implemented `get_bootstrap_info()` function

3. **`blixard-core/src/node.rs`**
   - Replaced TODO with HTTP bootstrap implementation
   - Added reqwest HTTP client for bootstrap requests
   - Integrated bootstrap flow into join_cluster

4. **`blixard/src/orchestrator.rs`**
   - Updated metrics server initialization to pass `SharedNodeState`

5. **`blixard-core/Cargo.toml`**
   - Added `reqwest` dependency for HTTP client

### Usage

1. **Start bootstrap node**:
   ```bash
   export BLIXARD_P2P_ENABLED=true
   export BLIXARD_ENABLE_MDNS_DISCOVERY=false
   cargo run -- node --id 1 --bind 127.0.0.1:7001 --data-dir ./data1
   ```

2. **Verify bootstrap endpoint**:
   ```bash
   curl http://127.0.0.1:8001/bootstrap
   ```

3. **Join additional nodes**:
   ```bash
   export BLIXARD_P2P_ENABLED=true
   export BLIXARD_ENABLE_MDNS_DISCOVERY=false
   cargo run -- node --id 2 --bind 127.0.0.1:7002 --data-dir ./data2 --peers 127.0.0.1:7001
   ```

### Testing

A test script `test_p2p_bootstrap.sh` has been created to demonstrate multi-node cluster formation using the P2P bootstrap mechanism.

### Future Improvements

1. **Security**: Add authentication to bootstrap endpoint
2. **Caching**: Cache bootstrap information to reduce HTTP requests
3. **Multiple Bootstrap Nodes**: Support querying multiple nodes for bootstrap info
4. **DNS Integration**: Store P2P info in DNS TXT records as an alternative
5. **Relay Configuration**: Properly extract relay URL from Iroh endpoint

## Conclusion

The P2P bootstrap mechanism successfully solves the chicken-and-egg problem of establishing Iroh P2P connections without prior knowledge of node identities. This enables seamless multi-node cluster formation while maintaining the security and performance benefits of Iroh's P2P transport.