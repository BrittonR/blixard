# Iroh Migration Phase 1 Status

## Overview

Phase 1 of the Iroh migration plan has been successfully implemented, establishing the foundation for dual-transport infrastructure that will enable gradual migration from gRPC to Iroh-based peer-to-peer communication.

## Completed Components

### 1. Transport Abstraction Layer (`src/transport/`)

Created a comprehensive transport abstraction that supports:
- **Transport modes**: gRPC-only, Iroh-only, and Dual mode
- **Configuration system**: TOML-based configuration with migration strategies
- **Service factory**: Transport-agnostic service creation
- **Client factory**: Transport-aware client creation with adaptive selection

### 2. Configuration Schema (`src/transport/config.rs`)

Implemented the configuration structure outlined in the migration plan:
```rust
pub enum TransportConfig {
    Grpc(GrpcConfig),
    Iroh(IrohConfig),
    Dual {
        grpc: GrpcConfig,
        iroh: IrohConfig,
        strategy: MigrationStrategy,
    },
}
```

Features:
- Service-specific transport preferences
- Raft transport preference with adaptive mode
- Fallback strategies for resilience

### 3. Iroh-gRPC Bridge (`src/transport/iroh_grpc_bridge.rs`)

Created the foundation for running gRPC services over Iroh:
- `IrohHttpAdapter`: HTTP/2 over QUIC adapter
- `GrpcProtocolHandler`: Protocol handler for gRPC services
- `IrohChannel`: Client channel implementation
- `IrohServiceRunner`: Service runner for Iroh transport

### 4. Metrics Infrastructure (`src/transport/metrics.rs`)

Implemented comprehensive metrics for monitoring the migration:
- RPC latency by transport type
- Connection metrics (active, errors, attempts)
- Raft-specific metrics for consensus monitoring
- Transport selection and fallback tracking

### 5. Service and Client Factories

- `ServiceFactory`: Creates services with appropriate transport
- `ClientFactory`: Creates clients with transport selection logic
- Support for adaptive transport selection based on latency metrics

## Integration with Existing Code

### Fixed Issues
1. **Compilation errors**: Fixed VmConfig missing fields in tests
2. **ClusterStateManager**: Re-enabled P2P state sharing methods
3. **Error types**: Added GrpcError and P2PError variants
4. **Module exports**: Added tui module to lib.rs

### Remaining Work

1. **HTTP/2 over QUIC**: The iroh_grpc_bridge needs completion of HTTP/2 framing
2. **Integration**: Connect transport layer with Node and grpc_server
3. **Configuration**: Add transport config to NodeConfig
4. **Testing**: Add tests for dual transport scenarios

## Next Phases

### Phase 2: Non-Critical Services (Weeks 3-4)
- Migrate health check service
- Migrate status queries (GetClusterStatus, GetRaftStatus)
- Migrate monitoring endpoints

### Phase 3: VM Operations (Weeks 5-6)
- Migrate VM lifecycle operations
- Integrate with existing P2P image transfer

### Phase 4: Peer Connector Migration (Weeks 7-8)
- Create IrohPeerConnector
- Maintain feature parity with gRPC

### Phase 5: Raft Evaluation (Weeks 9-10)
- Benchmark Raft over Iroh
- Decision on Raft transport

## Usage Example

```toml
# Dual transport configuration
[transport]
mode = "dual"

[transport.grpc]
bind_address = "0.0.0.0:7001"
max_message_size = 4194304
tls_enabled = false

[transport.iroh]
enabled = true
home_relay = "https://relay.iroh.network"
discovery_port = 0

[transport.migration]
prefer_iroh_for = ["health", "status", "monitoring"]
raft_transport = "always_grpc"
fallback_to_grpc = true
```

## Technical Debt

1. **tonic-iroh-transport**: The plan referenced this library, but it doesn't exist as a published crate. We've implemented our own bridge.
2. **HTTP/2 framing**: Needs proper implementation for full functionality
3. **Body type compatibility**: Some complexity around hyper/tonic body types

## Conclusion

Phase 1 has successfully established the foundation for the Iroh migration. The transport abstraction layer is in place, configuration is ready, and the basic infrastructure for running gRPC services over Iroh has been created. The next phases can now proceed with migrating specific services.