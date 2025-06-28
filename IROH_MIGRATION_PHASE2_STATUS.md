# Iroh Migration Phase 2 Status

## Overview

Phase 2 of the Iroh migration focuses on migrating non-critical services (health check, status queries, and monitoring endpoints) to validate the dual-transport approach. This phase builds on the foundation established in Phase 1.

## Completed Components

### 1. Transport-Agnostic Service Implementations

Created modular service implementations that can work over both gRPC and Iroh:

#### Health Service (`src/transport/services/health.rs`)
- **HealthService trait**: Transport-agnostic health check interface
- **HealthServiceImpl**: Implementation with node health validation
- **Health checks include**:
  - Raft manager status
  - Storage accessibility
  - Peer connector health
- **gRPC adapter**: Implements ClusterService trait for backward compatibility

#### Status Service (`src/transport/services/status.rs`)
- **StatusService trait**: Interface for cluster and Raft status queries
- **StatusServiceImpl**: Full implementation of status queries
- **Supports**:
  - GetClusterStatus: Returns node list, leader info, and term
  - GetRaftStatus: Returns Raft state information
- **P2P integration**: Includes Iroh node IDs and addresses when available
- **Dual adapters**: Implements both ClusterService and BlixardService traits

#### Monitoring Service (`src/transport/services/monitoring.rs`)
- **MonitoringService trait**: Interface for metrics and monitoring data
- **MonitoringServiceImpl**: Comprehensive monitoring implementation
- **Features**:
  - Prometheus metrics export
  - System information (CPU, memory, disk)
  - Cluster health summary
  - Structured monitoring response format
- **HTTP endpoint support**: Ready for Prometheus scraping

### 2. Dual Service Runner (`src/transport/dual_service_runner.rs`)

Implements the infrastructure to run services over both transports simultaneously:

- **DualServiceRunner**: Main runner supporting three modes:
  - gRPC-only mode
  - Iroh-only mode (placeholder)
  - Dual mode with migration strategy
- **CombinedNonCriticalService**: Aggregates health and status services
- **MigrationFilteredService**: Routes requests based on migration strategy
- **Concurrent transport support**: Can run both gRPC and Iroh servers

### 3. Transport Configuration Integration

Updated core types to support transport configuration:

```rust
pub struct NodeConfig {
    // ... existing fields ...
    pub transport_config: Option<TransportConfig>,
}
```

## Architecture Decisions

### 1. Service Trait Pattern
Each service type has a transport-agnostic trait that defines the core operations. This allows:
- Easy testing with mock implementations
- Clean separation between business logic and transport
- Future flexibility for additional transports

### 2. Adapter Pattern for gRPC
Services implement the existing gRPC service traits to maintain backward compatibility while the migration is in progress.

### 3. Combined Service Approach
For gRPC, we combine multiple logical services into a single implementation to reduce boilerplate while maintaining clean separation in the trait layer.

## Next Steps

### 1. Complete Iroh Protocol Handlers
The Iroh protocol handlers are currently placeholders. Need to implement:
- Request/response serialization over QUIC streams
- Protocol negotiation using ALPN
- Error handling and timeouts

### 2. Integration with Node
- Add transport initialization to Node startup
- Configure dual service runner based on NodeConfig
- Update existing gRPC server to use new service implementations

### 3. Client-Side Implementation
- Create Iroh clients for health, status, and monitoring
- Implement fallback logic when Iroh is unavailable
- Add latency tracking for adaptive transport selection

### 4. Testing Infrastructure
- Unit tests for each service implementation
- Integration tests for dual-transport scenarios
- Performance benchmarks comparing gRPC vs Iroh

## Usage Example

```toml
# Example configuration for Phase 2
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
alpn_protocols = ["blixard/health/1", "blixard/status/1", "blixard/monitoring/1"]

[transport.strategy]
prefer_iroh_for = ["health", "status", "monitoring"]
raft_transport = "always_grpc"
fallback_to_grpc = true
```

## Migration Path

1. **Deploy with dual mode**: Services available on both transports
2. **Monitor metrics**: Track latency, error rates, and usage patterns
3. **Gradually shift traffic**: Update client configurations to prefer Iroh
4. **Validate stability**: Ensure Iroh services are reliable
5. **Deprecate gRPC endpoints**: Once confidence is established

## Technical Challenges

### 1. Protocol Design
Need to define efficient binary protocols for Iroh that maintain feature parity with gRPC while leveraging QUIC benefits.

### 2. Service Discovery
Clients need to discover which transport to use for each service, potentially requiring a service registry or configuration management.

### 3. Monitoring Integration
Ensure monitoring works seamlessly across both transports, with unified metrics and tracing.

## Phase 2 Completion Status

- ✅ Service trait definitions
- ✅ Service implementations (health, status, monitoring)
- ✅ gRPC adapter layer
- ✅ Dual service runner framework
- ✅ Transport configuration in NodeConfig
- ⏳ Iroh protocol handlers (placeholders only)
- ⏳ Integration with existing Node/gRPC server
- ⏳ Client implementations
- ⏳ Testing and benchmarks

## Conclusion

Phase 2 has successfully created the service abstraction layer and dual-transport infrastructure needed to migrate non-critical services. The modular design allows for incremental migration and thorough testing before moving critical services in later phases.