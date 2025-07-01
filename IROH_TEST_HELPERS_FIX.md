# Test Helpers Iroh Integration Fix

## Current Issue

The test_helpers.rs file is starting Iroh services but still has references to gRPC client types:
- `ClusterServiceClient<Channel>` - gRPC client type
- `Channel` - tonic/gRPC type

But it's actually using:
- `start_iroh_services()` - Iroh transport
- `IrohClusterServiceClient` - Iroh client type

## Solution Options

### Option 1: Update to use IrohClusterServiceClient (Recommended)

Replace all gRPC client types with Iroh equivalents:

```rust
// Replace:
pub async fn client(&self) -> BlixardResult<ClusterServiceClient<Channel>>

// With:
pub async fn client(&self) -> BlixardResult<IrohClusterServiceClient>
```

### Option 2: Add Transport Abstraction

Create a trait that both gRPC and Iroh clients implement:

```rust
#[async_trait]
pub trait ClusterClient {
    async fn join_cluster(&self, node_id: u64, bind_address: String) -> BlixardResult<JoinClusterResponse>;
    // ... other methods
}
```

### Option 3: Use Feature Flags

```rust
#[cfg(feature = "grpc")]
pub async fn client(&self) -> BlixardResult<ClusterServiceClient<Channel>>

#[cfg(feature = "iroh")]
pub async fn client(&self) -> BlixardResult<IrohClusterServiceClient>
```

## Implementation Plan

Going with Option 1 (simplest for now):

1. Update client return types
2. Update RetryClient to work with Iroh
3. Update TestCluster client cache
4. Fix any method call differences between gRPC and Iroh clients