# Tonic-Iroh-Transport Integration

## Overview

We are switching from our custom Iroh integration to using the published `tonic-iroh-transport` crate (v0.0.3) from hellas-ai. This library provides a complete solution for running tonic gRPC services over Iroh P2P connections.

## Key Benefits of Using tonic-iroh-transport

1. **Proven Implementation**: The library handles all the complexities of bridging HTTP/2 gRPC streams with QUIC streams
2. **Type Safety**: Full integration with tonic's generated clients and servers
3. **Less Custom Code**: We can remove our custom wire protocol implementation
4. **Active Maintenance**: Published crate with recent updates (June 2025)
5. **Performance**: Benchmarks show 40,889 messages/second with 0.02ms average latency

## Migration Strategy

### Phase 1: Add Dependency ✅
- Added `tonic-iroh-transport = "0.0.3"` to Cargo.toml
- Created `tonic_iroh_bridge.rs` using the library's API

### Phase 2: Update Service Runner
- Modified `dual_service_runner.rs` to use `IrohGrpcServer` from tonic-iroh-transport
- Implemented `IrohMigrationFilteredService` for selective service migration

### Phase 3: Client Integration
- Use `IrohClient` from tonic-iroh-transport for P2P connections
- Replace custom `IrohPeerConnector` internals with library client

## API Usage Examples

### Server Setup
```rust
// Create iroh endpoint
let endpoint = Endpoint::builder().bind().await?;

// Set up gRPC service with P2P transport
let (handler, incoming, alpn) = GrpcProtocolHandler::for_service(
    ClusterServiceServer::new(service)
);

// Register with iroh
let router = iroh::protocol::Router::builder(endpoint)
    .accept(alpn, handler)
    .spawn();

// Start gRPC server
Server::builder()
    .add_service(ClusterServiceServer::new(service))
    .serve_with_incoming(incoming)
    .await?;
```

### Client Usage
```rust
// Create client
let iroh_client = IrohClient::new(endpoint);

// Connect to service
let channel = iroh_client
    .connect_to_service::<ClusterServiceServer<()>>(peer_addr)
    .await?;

// Create gRPC client
let mut client = ClusterServiceClient::new(channel);

// Make RPC calls as normal
let response = client.health_check(request).await?;
```

## Implementation Status

### Completed
- ✅ Added tonic-iroh-transport dependency
- ✅ Created `tonic_iroh_bridge.rs` with server and client wrappers
- ✅ Updated `dual_service_runner.rs` to use the library
- ✅ Implemented service filtering for gradual migration

### Next Steps
1. Remove custom wire protocol implementation (`iroh_protocol.rs`)
2. Update `IrohPeerConnector` to use library's `IrohClient`
3. Test dual-transport mode with real services
4. Update client factory to use library's client creation

## Files to Update/Remove

### Can Be Simplified
- `iroh_peer_connector.rs` - Use library's connection management
- `iroh_client.rs` - Use library's client implementations

### Can Be Removed
- `iroh_protocol.rs` - No longer needed with library
- Custom message framing code

### Keep
- Service trait definitions (health, status, vm, etc.)
- Transport configuration
- Migration strategy logic

## Testing Plan

1. **Unit Tests**: Verify server and client creation
2. **Integration Tests**: Test service calls over P2P
3. **Dual Mode Tests**: Verify services work on both transports
4. **Migration Tests**: Test gradual service migration

## Performance Expectations

Based on the library's benchmarks:
- Throughput: ~40,889 messages/second
- Latency: ~0.02ms average
- Connection setup: Higher initial cost but amortized

These numbers are excellent for our use case and should provide better performance than our custom implementation.

## Conclusion

Switching to `tonic-iroh-transport` significantly simplifies our codebase while providing a battle-tested implementation. This allows us to focus on our core business logic rather than maintaining transport layer code.