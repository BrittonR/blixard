# Iroh Custom Implementation Plan

## Background

After attempting to integrate `tonic-iroh-transport` v0.0.3, we discovered several issues:
1. Version incompatibility (requires tonic 0.13)
2. API doesn't match documentation
3. Library appears unstable (v0.0.3)

We're reverting to a custom implementation that gives us full control.

## Architecture Overview

### Layer 1: Raw Iroh Transport
- Use iroh's QUIC streams directly
- Handle connection establishment and management
- Implement message framing

### Layer 2: Protocol Layer
- Define our own RPC protocol over QUIC streams
- Support request/response pattern
- Handle streaming (for Raft messages)
- Implement proper error handling

### Layer 3: Service Abstraction
- Create trait-based service definitions
- Generate client/server stubs
- Maintain compatibility with existing gRPC service definitions

## Implementation Steps

### Step 1: Basic Message Protocol
```rust
// Message framing
struct MessageFrame {
    version: u8,
    message_type: MessageType,
    request_id: [u8; 16],
    payload_len: u32,
    payload: Vec<u8>,
}

enum MessageType {
    Request,
    Response,
    StreamData,
    Error,
    KeepAlive,
}
```

### Step 2: Service Handler Framework
```rust
#[async_trait]
trait IrohService: Send + Sync {
    type Request: Serialize + DeserializeOwned;
    type Response: Serialize + DeserializeOwned;
    
    async fn handle(&self, request: Self::Request) -> Result<Self::Response>;
}

struct IrohServiceHandler<S: IrohService> {
    service: Arc<S>,
    endpoint: Endpoint,
}
```

### Step 3: Client Implementation
```rust
struct IrohServiceClient {
    endpoint: Endpoint,
    connections: DashMap<NodeId, Connection>,
}

impl IrohServiceClient {
    async fn call<Req, Res>(&self, target: NodeAddr, request: Req) -> Result<Res>
    where
        Req: Serialize,
        Res: DeserializeOwned,
    {
        // 1. Get or establish connection
        // 2. Open bi-directional stream
        // 3. Send request
        // 4. Wait for response
        // 5. Handle errors
    }
}
```

### Step 4: Integration Points

#### Health Service Example
```rust
struct HealthServiceImpl {
    node: Arc<SharedNodeState>,
}

#[async_trait]
impl IrohService for HealthServiceImpl {
    type Request = HealthCheckRequest;
    type Response = HealthCheckResponse;
    
    async fn handle(&self, _req: Self::Request) -> Result<Self::Response> {
        Ok(HealthCheckResponse {
            status: "healthy".to_string(),
            node_id: self.node.node_id(),
            // ...
        })
    }
}
```

### Step 5: Dual Transport Support
- Keep existing gRPC services
- Add Iroh handlers for the same services
- Use configuration to route traffic

## Advantages of Custom Implementation

1. **Full Control**: We control the protocol and can optimize for our use case
2. **No Version Conflicts**: No dependency on unstable external libraries
3. **Better Integration**: Can integrate tightly with our existing code
4. **Learning Opportunity**: Better understanding of P2P networking
5. **Future Flexibility**: Can add features like compression, encryption layers

## Timeline

- **Week 1**: Basic protocol and message framing
- **Week 2**: Service handler framework
- **Week 3**: Client implementation and connection management
- **Week 4**: Integration with existing services
- **Week 5**: Testing and optimization
- **Week 6**: Documentation and benchmarking

## Success Criteria

1. Health check service works over Iroh
2. Can establish and maintain P2P connections
3. Performance comparable to gRPC for local connections
4. Proper error handling and recovery
5. Clean API for service developers

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Protocol bugs | Extensive testing, simple initial design |
| Performance issues | Benchmark early, optimize later |
| Compatibility | Keep gRPC as fallback |
| Complexity | Start simple, add features incrementally |

## Conclusion

While using tonic-iroh-transport seemed attractive, building our own implementation gives us more control and stability. We can start simple and add features as needed, while maintaining full compatibility with our existing architecture.