# Blixard Iroh Transport Migration Plan

## Executive Summary

This document outlines a comprehensive plan to migrate Blixard from gRPC to Iroh-based peer-to-peer communication using the `tonic-iroh-transport` library. This migration will provide end-to-end encrypted communication, automatic NAT traversal, and improved network resilience while maintaining compatibility with our existing Tonic/gRPC service definitions.

## Table of Contents

1. [Motivation](#motivation)
2. [Current Architecture](#current-architecture)
3. [Target Architecture](#target-architecture)
4. [Implementation Phases](#implementation-phases)
5. [Technical Implementation](#technical-implementation)
6. [Risk Analysis](#risk-analysis)
7. [Testing Strategy](#testing-strategy)
8. [Performance Considerations](#performance-considerations)
9. [Timeline](#timeline)
10. [Success Criteria](#success-criteria)

## Motivation

### Why Migrate to Iroh?

1. **End-to-End Encryption by Default**
   - Ed25519-based node identities
   - No TLS certificate management
   - Encrypted QUIC connections (TLS 1.3)

2. **Superior Network Connectivity**
   - Automatic NAT traversal with hole punching
   - Relay server fallback for unreachable nodes
   - Connection migration (nodes can change IPs)
   - Works behind restrictive firewalls

3. **Performance Benefits**
   - QUIC's stream multiplexing
   - No head-of-line blocking
   - Better performance on lossy networks
   - Lower connection establishment overhead

4. **Operational Simplicity**
   - Single transport for both RPC and data transfer
   - Self-forming mesh networks
   - Built-in peer discovery

### Why Use tonic-iroh-transport?

- Maintains compatibility with existing gRPC service definitions
- Proven RPC semantics (timeouts, cancellation, streaming)
- Gradual migration path
- Keeps Tonic ecosystem benefits (interceptors, middleware)

## Current Architecture

### Components Using gRPC

1. **ClusterService** (20 RPCs)
   - Node management: `JoinCluster`, `LeaveCluster`, `GetClusterStatus`
   - Raft consensus: `SendRaftMessage` (critical)
   - VM operations: Create, Start, Stop, Delete, List, Status, Migrate
   - Task management: Submit, Status
   - Health monitoring: `HealthCheck`

2. **BlixardService** (2 RPCs)
   - `GetRaftStatus`
   - `ProposeTask`

3. **PeerConnector**
   - Manages gRPC connections to peers
   - Implements connection pooling and circuit breakers
   - Handles Raft message delivery with retry logic

### Current Network Flow

```
Client → gRPC (HTTP/2) → Node → Raft → gRPC → Peers
                            ↓
                         VM Manager
                            ↓
                    P2P (Iroh) for data
```

## Target Architecture

### Unified Iroh Transport

```
Client → Iroh (QUIC) → Node → Raft → Iroh → Peers
                          ↓
                      VM Manager
                          ↓
                   Iroh for everything
```

### Key Components

1. **IrohEndpoint**: Single endpoint for all communication
2. **Service Handlers**: tonic-iroh-transport bridges for each service
3. **IrohPeerConnector**: Replaces gRPC-based PeerConnector
4. **Unified Configuration**: Single transport configuration

## Implementation Phases

### Phase 1: Foundation (Weeks 1-2)

**Goal**: Add tonic-iroh-transport and create dual-transport infrastructure

1. **Add Dependencies**
   ```toml
   [dependencies]
   tonic-iroh-transport = "1.0"
   ```

2. **Create Transport Abstraction**
   ```rust
   pub enum Transport {
       Grpc(GrpcTransport),
       Iroh(IrohTransport),
       Dual(GrpcTransport, IrohTransport),
   }
   ```

3. **Configuration Schema**
   ```toml
   [transport]
   mode = "dual"  # "grpc", "iroh", or "dual"
   
   [transport.grpc]
   bind_address = "0.0.0.0:7001"
   
   [transport.iroh]
   enabled = true
   home_relay = "https://relay.iroh.network"
   discovery_port = 0  # random
   
   [transport.migration]
   prefer_iroh_for = ["health", "status", "monitoring"]
   raft_transport = "grpc"  # Start conservative
   ```

4. **Implement Service Registry**
   - Abstract service registration
   - Support both transports simultaneously
   - Add transport selection logic

### Phase 2: Non-Critical Services (Weeks 3-4)

**Goal**: Migrate low-risk services to validate approach

1. **Health Check Service**
   - Low traffic, simple request/response
   - Good first candidate

2. **Status Queries**
   - GetClusterStatus
   - GetRaftStatus
   - ListVms

3. **Monitoring Endpoints**
   - Metrics collection
   - Resource summaries

**Implementation**:
```rust
// Service wrapper supporting both transports
pub struct DualTransportService<S> {
    service: S,
    grpc_server: Option<Server>,
    iroh_handler: Option<GrpcProtocolHandler>,
}

impl<S> DualTransportService<S> {
    pub async fn serve(&self, config: &TransportConfig) -> Result<()> {
        match config.mode {
            TransportMode::Grpc => self.serve_grpc().await,
            TransportMode::Iroh => self.serve_iroh().await,
            TransportMode::Dual => {
                tokio::select! {
                    res = self.serve_grpc() => res,
                    res = self.serve_iroh() => res,
                }
            }
        }
    }
}
```

### Phase 3: VM Operations (Weeks 5-6)

**Goal**: Migrate VM management operations

1. **VM Lifecycle Operations**
   - CreateVm, StartVm, StopVm, DeleteVm
   - Already integrated with P2P for image transfer
   - Natural fit for unified transport

2. **Scheduling Operations**
   - ScheduleVmPlacement
   - CreateVmWithScheduling

3. **Migration Operations**
   - MigrateVm
   - Benefits from P2P direct connections

**Key Consideration**: These operations are less latency-sensitive than Raft

### Phase 4: Peer Connector Migration (Weeks 7-8)

**Goal**: Create IrohPeerConnector for non-Raft peer communication

1. **Implement IrohPeerConnector**
   ```rust
   pub struct IrohPeerConnector {
       endpoint: Endpoint,
       connections: Arc<DashMap<u64, IrohClient>>,
       peers: Arc<RwLock<HashMap<u64, PeerInfo>>>,
       circuit_breakers: Arc<DashMap<u64, CircuitBreaker>>,
   }
   
   impl IrohPeerConnector {
       pub async fn connect_to_peer(&self, peer_id: u64) -> Result<IrohClient> {
           let peer_info = self.get_peer_info(peer_id).await?;
           let node_addr = self.peer_info_to_node_addr(&peer_info)?;
           
           let client = IrohClient::new(self.endpoint.clone());
           let channel = client.connect_to_service::<ClusterService>(node_addr).await?;
           
           self.connections.insert(peer_id, client);
           Ok(client)
       }
   }
   ```

2. **Maintain Feature Parity**
   - Connection pooling
   - Circuit breakers
   - Message buffering
   - Automatic reconnection

### Phase 5: Raft Consensus Evaluation (Weeks 9-10)

**Goal**: Determine if Raft can run over Iroh transport

1. **Benchmark Setup**
   - Create isolated test cluster
   - Implement both transports for Raft
   - Measure under various conditions

2. **Key Metrics**
   - Message latency (p50, p95, p99)
   - Throughput under load
   - Election timeout stability
   - Recovery time after partition

3. **Test Scenarios**
   - Local network (low latency)
   - WAN simulation (higher latency)
   - Packet loss scenarios
   - Network partitions

4. **Decision Matrix**
   ```
   If p99_latency < 10ms AND election_stability > 99.9%:
       → Proceed with Raft on Iroh
   Else:
       → Keep Raft on gRPC (hybrid mode)
   ```

### Phase 6: Production Rollout (Weeks 11-12)

**Goal**: Gradual production deployment

1. **Canary Deployment**
   - 5% of nodes on Iroh transport
   - Monitor for 1 week
   - Gradual increase: 5% → 25% → 50% → 100%

2. **Rollback Strategy**
   - Configuration flag to revert
   - Dual-transport mode as safety net
   - Monitoring and alerts

3. **Final Migration**
   - Remove gRPC dependencies (if full migration)
   - Update documentation
   - Training and knowledge transfer

## Technical Implementation

### 1. Transport Configuration

```rust
// src/config/transport.rs
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum TransportConfig {
    Grpc(GrpcConfig),
    Iroh(IrohConfig),
    Dual {
        grpc: GrpcConfig,
        iroh: IrohConfig,
        strategy: MigrationStrategy,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct MigrationStrategy {
    /// Services to prefer Iroh for
    pub prefer_iroh: Vec<ServiceType>,
    
    /// Raft-specific transport preference
    pub raft_transport: RaftTransportPreference,
    
    /// Fallback behavior
    pub fallback_to_grpc: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub enum RaftTransportPreference {
    AlwaysGrpc,
    AlwaysIroh,
    Adaptive { latency_threshold_ms: f64 },
}
```

### 2. Service Factory

```rust
// src/transport/service_factory.rs
pub struct ServiceFactory {
    transport_config: TransportConfig,
    endpoint: Option<Endpoint>,
}

impl ServiceFactory {
    pub async fn create_cluster_service(
        &self, 
        implementation: ClusterServiceImpl
    ) -> Result<Box<dyn ServiceRunner>> {
        match &self.transport_config {
            TransportConfig::Grpc(config) => {
                Ok(Box::new(GrpcServiceRunner::new(config, implementation)))
            }
            TransportConfig::Iroh(config) => {
                Ok(Box::new(IrohServiceRunner::new(
                    config, 
                    self.endpoint.clone().unwrap(),
                    implementation
                )))
            }
            TransportConfig::Dual { strategy, .. } => {
                Ok(Box::new(DualServiceRunner::new(
                    strategy.clone(),
                    implementation
                )))
            }
        }
    }
}
```

### 3. Client Factory

```rust
// src/transport/client_factory.rs
pub struct ClientFactory {
    transport_config: TransportConfig,
    endpoint: Option<Endpoint>,
}

impl ClientFactory {
    pub async fn create_cluster_client(
        &self,
        peer_addr: &PeerAddress,
    ) -> Result<ClusterServiceClient<Channel>> {
        match &self.transport_config {
            TransportConfig::Grpc(_) => {
                let channel = Channel::from_shared(format!("http://{}", peer_addr.grpc_addr))?
                    .connect()
                    .await?;
                Ok(ClusterServiceClient::new(channel))
            }
            TransportConfig::Iroh(_) => {
                let client = IrohClient::new(self.endpoint.clone().unwrap());
                let channel = client
                    .connect_to_service::<ClusterService>(peer_addr.node_addr.clone())
                    .await?;
                Ok(ClusterServiceClient::new(channel))
            }
            TransportConfig::Dual { strategy, .. } => {
                // Choose based on strategy
                self.create_with_strategy(peer_addr, strategy).await
            }
        }
    }
}
```

### 4. Metrics and Monitoring

```rust
// src/transport/metrics.rs
pub struct TransportMetrics {
    /// Latency histograms per transport type
    grpc_latency: Histogram,
    iroh_latency: Histogram,
    
    /// Connection metrics
    active_connections: IntGaugeVec,
    connection_errors: IntCounterVec,
    
    /// Raft-specific metrics
    raft_message_latency: HistogramVec,
    election_timeouts: IntCounter,
}

impl TransportMetrics {
    pub fn record_rpc(&self, transport: &str, service: &str, method: &str, duration: Duration) {
        match transport {
            "grpc" => self.grpc_latency.observe(duration.as_secs_f64()),
            "iroh" => self.iroh_latency.observe(duration.as_secs_f64()),
            _ => {}
        }
        
        if service == "raft" {
            self.raft_message_latency
                .with_label_values(&[transport, method])
                .observe(duration.as_secs_f64());
        }
    }
}
```

## Risk Analysis

### High Risks

1. **Raft Consensus Instability**
   - **Risk**: Higher latency causes election timeouts
   - **Mitigation**: Extensive testing, keep gRPC option, adaptive timeout adjustment
   - **Fallback**: Hybrid mode with Raft on gRPC

2. **Performance Regression**
   - **Risk**: Iroh transport slower than gRPC for some workloads
   - **Mitigation**: Comprehensive benchmarking, gradual rollout
   - **Fallback**: Dual-transport mode, service-specific configuration

### Medium Risks

1. **Operational Complexity**
   - **Risk**: Two transports increase complexity
   - **Mitigation**: Good abstractions, comprehensive monitoring
   - **Resolution**: Full migration after stability proven

2. **Debugging Challenges**
   - **Risk**: Loss of gRPC tooling (grpcurl, etc.)
   - **Mitigation**: Build equivalent Iroh tools, extensive logging

### Low Risks

1. **Library Maturity**
   - **Risk**: tonic-iroh-transport is newer than gRPC
   - **Mitigation**: Thorough testing, contribute fixes upstream

## Testing Strategy

### 1. Unit Tests

```rust
#[cfg(test)]
mod transport_tests {
    #[tokio::test]
    async fn test_service_over_iroh() {
        let service = create_test_service();
        let (handler, incoming, alpn) = GrpcProtocolHandler::for_service(
            ClusterServiceServer::new(service)
        );
        
        // Test basic RPC functionality
        // Test streaming
        // Test cancellation
        // Test timeout
    }
    
    #[tokio::test]
    async fn test_transport_selection() {
        let config = TransportConfig::Dual { 
            strategy: MigrationStrategy {
                prefer_iroh: vec![ServiceType::Health],
                ..
            }
        };
        
        // Verify correct transport selected
    }
}
```

### 2. Integration Tests

- Multi-node cluster formation over Iroh
- Raft consensus with both transports
- VM operations end-to-end
- Network partition handling

### 3. Simulation Tests (MadSim)

```rust
#[cfg(madsim)]
mod simulation_tests {
    #[madsim::test]
    async fn test_raft_over_iroh_with_delays() {
        // Simulate various network conditions
        // Test election stability
        // Verify consensus properties
    }
}
```

### 4. Benchmark Suite

```rust
// benches/transport_comparison.rs
fn bench_rpc_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("transport_latency");
    
    group.bench_function("grpc_unary", |b| {
        b.iter(|| /* gRPC unary call */)
    });
    
    group.bench_function("iroh_unary", |b| {
        b.iter(|| /* Iroh unary call */)
    });
    
    group.bench_function("grpc_raft_message", |b| {
        b.iter(|| /* Raft message over gRPC */)
    });
    
    group.bench_function("iroh_raft_message", |b| {
        b.iter(|| /* Raft message over Iroh */)
    });
}
```

## Performance Considerations

### Expected Performance

Based on tonic-iroh-transport benchmarks:
- **Throughput**: ~40,889 messages/second
- **Latency**: ~0.02ms average
- **Connection setup**: Higher initial cost, but amortized

### Optimization Strategies

1. **Connection Pooling**
   - Reuse Iroh connections
   - Pre-establish connections to known peers

2. **Stream Management**
   - Use stream priorities for Raft messages
   - Separate streams for different traffic types

3. **Relay Configuration**
   - Deploy custom relay servers in each region
   - Optimize relay selection algorithm

### Performance Monitoring

```yaml
# Grafana dashboards to create
dashboards:
  - transport_comparison:
      - latency_comparison_histogram
      - throughput_by_transport
      - connection_establishment_time
      - relay_vs_direct_connections
  
  - raft_performance:
      - election_success_rate
      - consensus_latency
      - leader_stability
      - message_retry_rate
```

## Timeline

### 12-Week Implementation Schedule

| Week | Phase | Deliverables |
|------|-------|-------------|
| 1-2 | Foundation | Transport abstraction, dual-mode config |
| 3-4 | Non-Critical Services | Health, status, monitoring on Iroh |
| 5-6 | VM Operations | VM lifecycle on Iroh |
| 7-8 | Peer Connector | IrohPeerConnector implementation |
| 9-10 | Raft Evaluation | Benchmarks, stability tests |
| 11-12 | Production Rollout | Canary → Full deployment |

### Milestones

1. **M1 (Week 2)**: Dual-transport infrastructure complete
2. **M2 (Week 4)**: First services running on Iroh in staging
3. **M3 (Week 6)**: All non-Raft services portable
4. **M4 (Week 8)**: Feature parity with gRPC
5. **M5 (Week 10)**: Raft decision made
6. **M6 (Week 12)**: Production deployment complete

## Success Criteria

### Technical Success

- [ ] All services accessible via Iroh transport
- [ ] No increase in p99 latency > 10%
- [ ] Raft consensus stability maintained
- [ ] Zero unplanned downtime during migration

### Operational Success

- [ ] Simplified network configuration (no TLS certs)
- [ ] Improved connectivity behind NATs
- [ ] Reduced operational complexity
- [ ] Clear rollback procedure

### Business Success

- [ ] Improved security posture (E2E encryption)
- [ ] Better support for edge deployments
- [ ] Foundation for future P2P features
- [ ] No negative impact on existing users

## Conclusion

Migrating to Iroh transport using tonic-iroh-transport provides significant benefits while maintaining a low-risk migration path. The phased approach allows for validation at each step, with clear rollback options. The hybrid mode provides a safe long-term option if Raft performs better on gRPC.

The key to success is careful measurement, gradual rollout, and maintaining optionality throughout the process. With proper execution, Blixard will gain a more secure, resilient, and flexible networking layer while maintaining the stability users expect.

## Appendix: Code Examples

### A. Complete Service Migration Example

```rust
// Before: gRPC only
pub async fn start_grpc_server(
    shared: Arc<SharedNodeState>,
    addr: SocketAddr,
) -> Result<()> {
    let cluster_service = ClusterServiceImpl::new(shared.clone());
    
    Server::builder()
        .add_service(ClusterServiceServer::new(cluster_service))
        .serve(addr)
        .await?;
    Ok(())
}

// After: Transport-agnostic
pub async fn start_server(
    shared: Arc<SharedNodeState>,
    config: TransportConfig,
) -> Result<()> {
    let cluster_service = ClusterServiceImpl::new(shared.clone());
    
    match config {
        TransportConfig::Grpc(grpc_config) => {
            Server::builder()
                .add_service(ClusterServiceServer::new(cluster_service))
                .serve(grpc_config.bind_addr)
                .await?;
        }
        TransportConfig::Iroh(iroh_config) => {
            let endpoint = Endpoint::builder()
                .bind()
                .await?;
                
            let (handler, incoming, alpn) = GrpcProtocolHandler::for_service(
                ClusterServiceServer::new(cluster_service)
            );
            
            endpoint.set_alpn(vec![alpn]);
            endpoint.accept_with_handler(handler, incoming).await;
        }
        TransportConfig::Dual { .. } => {
            // Run both in parallel
            tokio::select! {
                _ = start_grpc_server(shared.clone(), grpc_config.bind_addr) => {},
                _ = start_iroh_server(shared.clone(), iroh_config) => {},
            }
        }
    }
    Ok(())
}
```

### B. Client Connection Example

```rust
// Before: gRPC only
let mut client = ClusterServiceClient::connect(format!("http://{}", addr))
    .await?;

// After: Transport-aware
let mut client = match transport_config.client_transport_for(&peer_id) {
    Transport::Grpc => {
        ClusterServiceClient::connect(format!("http://{}", peer.grpc_addr))
            .await?
    }
    Transport::Iroh => {
        let iroh_client = IrohClient::new(endpoint);
        let channel = iroh_client
            .connect_to_service::<ClusterService>(peer.node_addr)
            .await?;
        ClusterServiceClient::new(channel)
    }
};

// Usage remains the same!
let response = client.health_check(Request::new(HealthCheckRequest {})).await?;
```

### C. Gradual Migration Configuration

```toml
# Start conservative
[transport]
mode = "dual"

[transport.migration]
prefer_iroh = []
raft_transport = "grpc"

# After validation
[transport.migration]
prefer_iroh = ["health", "status", "monitoring"]
raft_transport = "grpc"

# VM operations
[transport.migration]
prefer_iroh = ["health", "status", "monitoring", "vm_ops"]
raft_transport = "grpc"

# Full migration (if Raft performs well)
[transport]
mode = "iroh"

[transport.iroh]
home_relay = "https://relay.iroh.network"
```

## Implementation Progress

### ✅ Completed (2025-01-28)

1. **Custom Iroh RPC Protocol** (`iroh_protocol.rs`)
   - Binary protocol with framing and request IDs
   - Support for request/response and streaming
   - Efficient serialization with bincode
   - Message types: Request, Response, Error, StreamData, etc.

2. **Service Framework** (`iroh_service.rs`)
   - Trait-based service abstraction (`IrohService`)
   - Service registry for multiple services
   - RPC server (`IrohRpcServer`) that accepts connections
   - RPC client (`IrohRpcClient`) with connection pooling

3. **Health Service** (`iroh_health_service.rs`)
   - First service implemented over Iroh
   - Methods: `check` (health status) and `ping`
   - Client wrapper for easy usage

4. **Status Service** (`iroh_status_service.rs`)
   - Cluster and Raft status queries over Iroh
   - Methods: `get_cluster_status`, `get_raft_status`
   - Reuses existing `StatusServiceImpl` logic

5. **Dual Service Runner Integration** (`dual_service_runner.rs`)
   - ✅ Iroh-only mode: Serves all services via Iroh RPC
   - ✅ Dual mode: Can serve different services on different transports
   - ✅ Migration strategy support: Services can be gradually moved to Iroh

6. **Tests and Examples**
   - Unit tests for protocol, services
   - Integration test for dual transport mode
   - Demo example showing both transports working simultaneously

### 🔧 In Progress

1. **VM Service over Iroh**
   - Need to implement `IrohVmService` for VM operations
   - Will reuse existing VM manager logic

2. **Raft Message Transport**
   - Critical path - needs careful implementation
   - Should maintain ordering and reliability guarantees

### 📋 TODO

1. **Client Factory Updates**
   - Update `client_factory.rs` to create Iroh clients
   - Implement transport selection logic

2. **Peer Connector Integration**
   - Update `iroh_peer_connector.rs` to use our RPC protocol
   - Handle connection lifecycle and retries

3. **Monitoring and Metrics**
   - Add Iroh-specific metrics
   - Connection stats, RPC latencies, etc.

4. **Production Hardening**
   - Connection health checks
   - Graceful degradation
   - Better error handling and retries

### Key Decision: Custom RPC vs tonic-iroh-transport

After investigation, we chose to implement a custom RPC protocol because:
- `tonic-iroh-transport` had compatibility issues with our setup
- Custom protocol gives us more control and flexibility
- Simpler implementation that meets our specific needs
- Can optimize for our use cases (e.g., Raft messages)

The custom protocol is working well and provides:
- Simple, efficient binary framing
- Request/response correlation
- Service multiplexing
- Future support for streaming if needed