# P2P Connectivity Status Report

## Current State

### ✅ What's Working

1. **Iroh Transport Initialization**
   - Iroh endpoints are created successfully for each node
   - Each node gets a unique Iroh NodeId (e.g., `0f552b4a4e...`)
   - Local addresses are detected (IPv4 and IPv6)
   - STUN and relay discovery is active

2. **Protocol Implementation**
   - Basic Iroh protocol message serialization/deserialization works
   - Test binaries confirm the protocol layer is functional
   - Message types (Request, Response, Error, etc.) are properly defined

3. **Transport Layer Structure**
   - `IrohTransport` struct is implemented
   - Basic endpoint creation and management
   - File sharing stubs are in place

### ❌ What's Not Working

1. **P2P Manager Initialization Failure**
   - `P2pImageStore::new()` calls `create_or_join_doc()` which is not implemented
   - This causes node startup to fail with "NotImplemented" error
   - The document operations are stubbed but return errors

2. **Missing P2P Info Exchange**
   - No P2P address information in the gRPC Join/Leave messages
   - Nodes don't exchange their Iroh NodeAddr during cluster formation
   - No mechanism to discover other nodes' P2P endpoints

3. **No Connection Establishment**
   - `IrohPeerConnector` exists but isn't wired up
   - No code to actually connect Iroh endpoints between nodes
   - Message routing between P2P and Raft layers not implemented

## What Needs to Be Fixed

### 1. Fix P2P Manager Initialization
**File**: `blixard-core/src/p2p_image_store.rs`
**Issue**: Line 60 calls `create_or_join_doc()` which fails
**Fix**: Either:
- Implement the document operations properly, OR
- Make P2pImageStore initialization optional/lazy

### 2. Add P2P Info to Cluster Messages
**Files**: 
- `proto/blixard.proto` - Add p2p_node_addr field to JoinRequest/Response
- `blixard-core/src/grpc_server.rs` - Include P2P info in join/leave
- `blixard-core/src/node_shared.rs` - Store P2P addresses with peer info

### 3. Implement P2P Connection Logic
**Files**:
- `blixard-core/src/transport/iroh_peer_connector.rs` - Wire up connection establishment
- `blixard-core/src/node.rs` - Initialize peer connector after Iroh transport
- Add connection triggers when new peers join the cluster

### 4. Create P2P Service Registration
**Files**:
- `blixard-core/src/transport/iroh_service_runner.rs` - Register P2P services
- Implement handlers for different message types (Raft, VM operations, etc.)

## Recommended Next Steps

1. **Quick Fix for Testing**: 
   - Comment out the `create_or_join_doc()` call in P2pImageStore
   - This will allow nodes to start and we can test basic connectivity

2. **Implement P2P Address Exchange**:
   - Add Iroh NodeAddr to peer information
   - Include it in cluster join/leave messages
   - Store it in SharedNodeState

3. **Wire Up Peer Connections**:
   - When a new peer joins, establish Iroh connection
   - Use IrohPeerConnector to manage connections
   - Test with simple ping/pong messages

4. **Integrate with Existing Services**:
   - Route Raft messages through P2P when available
   - Fall back to gRPC if P2P fails
   - Add metrics for P2P vs gRPC usage

## Test Plan

1. Start two nodes with P2P disabled (current state)
2. Fix initialization issue
3. Add P2P info exchange
4. Verify nodes can discover each other's P2P addresses
5. Establish P2P connections
6. Send test messages over P2P
7. Integrate with Raft transport