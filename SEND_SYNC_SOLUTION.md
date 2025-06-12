# Solution: Making Node Send + Sync for Arc<Node>

## Problem Analysis

The `Node` struct was not `Send + Sync` due to several fields:

1. **`oneshot::Sender<()>`** - Send but not Sync
2. **`JoinHandle<BlixardResult<()>>`** - Send but not Sync  
3. **`mpsc::UnboundedSender<...>`** - Send but not Sync
4. **`VmManager`** - Contains `mpsc::UnboundedSender` which is not Sync

These types are designed for single-threaded ownership and cannot be safely shared across threads without synchronization.

## Solution: Shared State Pattern

Instead of trying to wrap all fields in `Mutex/RwLock`, we implemented a cleaner architectural solution:

### 1. Created `SharedNodeState` struct
- Contains all the data that needs to be shared across threads
- Wraps non-Sync types in `Mutex` for thread-safe access
- All fields are now `Send + Sync`

### 2. Refactored `Node` struct
- Now only contains:
  - `shared: Arc<SharedNodeState>` - The shared state
  - `handle: Option<JoinHandle<...>>` - Runtime handles that stay with the owner
  - `raft_handle: Option<JoinHandle<...>>` - Raft task handle

### 3. Updated gRPC server
- Changed from `Arc<Node>` to `Arc<SharedNodeState>`
- All operations now go through the shared state

## Key Changes

### File: `src/node_shared.rs` (new)
```rust
pub struct SharedNodeState {
    pub config: NodeConfig,
    pub database: Option<Arc<Database>>,
    
    // Wrapped in Mutex for thread safety
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
    raft_proposal_tx: Mutex<Option<mpsc::UnboundedSender<RaftProposal>>>,
    raft_message_tx: Mutex<Option<mpsc::UnboundedSender<(u64, Message)>>>,
    
    // VM manager wrapped for thread safety
    vm_manager: RwLock<Option<Arc<VmManagerShared>>>,
    
    // Track running state
    is_running: RwLock<bool>,
}

// Compile-time verification
static_assertions::assert_impl_all!(SharedNodeState: Send, Sync);
```

### File: `src/node.rs` (updated)
```rust
pub struct Node {
    /// Shared state that is Send + Sync
    shared: Arc<SharedNodeState>,
    /// Non-sync runtime handles
    handle: Option<JoinHandle<BlixardResult<()>>>,
    raft_handle: Option<JoinHandle<BlixardResult<()>>>,
}

impl Node {
    /// Get a shared reference to the node state
    /// This is what should be passed to the gRPC server
    pub fn shared(&self) -> Arc<SharedNodeState> {
        Arc::clone(&self.shared)
    }
}
```

### File: `src/grpc_server.rs` (updated)
```rust
pub struct BlixardGrpcService {
    node: Arc<SharedNodeState>,  // Changed from Arc<Node>
}
```

## Benefits

1. **Clean separation of concerns**: Shared state vs runtime handles
2. **Type safety**: Compile-time verification of Send + Sync
3. **Minimal locking**: Only non-Sync types are wrapped in Mutex
4. **Easy to use**: Call `node.shared()` to get the shareable state
5. **Backward compatible**: Public API of Node mostly unchanged

## Usage

```rust
// Create node
let mut node = Node::new(config);
node.initialize().await?;

// Get shared state for gRPC server
let shared_state = node.shared();

// Start gRPC server with shared state
let server_handle = tokio::spawn(async move {
    start_grpc_server(shared_state, bind_address).await
});
```

This solution provides a clean, type-safe way to share node state across threads while maintaining proper ownership of runtime handles.