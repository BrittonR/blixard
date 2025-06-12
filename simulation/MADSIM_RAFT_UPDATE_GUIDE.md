# MadSim Raft Tests Update Guide

## Overview
This guide shows how to properly implement Raft consensus tests using MadSim based on the patterns in our codebase.

## Important: Separation of Concerns

**Key Insight**: We do NOT need to make the blixard crate madsim-compatible. The simulation tests are intentionally separate:

1. **Simulation crate has its own proto definitions** - Built with `madsim-tonic-build`
2. **Tests use simulation-specific implementations** - Not the actual blixard code
3. **Focus on protocol and distributed behavior** - Not implementation details

This separation allows us to:
- Test distributed scenarios without modifying production code
- Use MadSim-specific features freely in tests
- Keep the production codebase clean of test dependencies

## Key Differences: Mock vs Proper MadSim Implementation

### 1. **Node Creation**

**Mock Implementation:**
```rust
struct Node {
    config: NodeConfig,
    is_running: bool,
}

let mut node = Node::new(config);
node.start().await.unwrap();
```

**Proper MadSim:**
```rust
let handle = Handle::current();
let node = handle
    .create_node()
    .name("node-1")
    .ip("10.0.0.1".parse().unwrap())
    .build();

// Run service on the node
node.spawn(async move {
    // gRPC server code
});
```

### 2. **Network Communication**

**Mock Implementation:**
```rust
// Direct method calls
node.join_cluster(Some(peer_addr)).await.unwrap();
let (leader_id, nodes, term) = node.get_cluster_status().await.unwrap();
```

**Proper MadSim:**
```rust
// gRPC client/server communication
let mut client = ClusterServiceClient::connect("http://10.0.0.1:7001").await?;

let response = client
    .join_cluster(Request::new(JoinClusterRequest {
        peer_addr: "10.0.0.1:7001".to_string(),
    }))
    .await?;
```

### 3. **Network Simulation**

**Mock Implementation:**
```rust
// Manual state tracking
let mut state = CLUSTER_STATE.lock().unwrap();
state.is_partitioned = true;
state.minority_nodes = vec![1, 2];
```

**Proper MadSim:**
```rust
let net = NetSim::current();

// Create network partition
net.clog_link(node1.id(), node2.id());  // Block communication
net.clog_node(node1.id());              // Isolate entire node

// Add packet loss (at runtime creation)
let mut config = madsim::Config::default();
config.net.packet_loss_rate = 0.1;  // 10% loss
let runtime = Runtime::with_config(config);
```

### 4. **Service Implementation**

**Mock Implementation:**
```rust
impl Node {
    async fn submit_task(&self, task_id: &str, task: TaskSpec) -> Result<u64, Box<dyn Error>> {
        // Mock logic
        Ok(self.config.id)
    }
}
```

**Proper MadSim:**
```rust
#[tonic::async_trait]
impl ClusterService for RaftNodeService {
    async fn submit_task(
        &self,
        request: Request<TaskRequest>,
    ) -> Result<Response<TaskResponse>, Status> {
        // Real Raft consensus logic would go here
        // - Check if we're the leader
        // - Propose to Raft
        // - Wait for commit
        // - Execute task
        Ok(Response::new(TaskResponse { /* ... */ }))
    }
}
```

### 5. **Time and Synchronization**

**Mock Implementation:**
```rust
sleep(Duration::from_secs(2)).await;  // Just waits
```

**Proper MadSim:**
```rust
// Time advances instantly in simulation
let start = Instant::now();
sleep(Duration::from_secs(3600)).await;
assert_eq!(start.elapsed(), Duration::from_secs(3600));

// Use timeouts for network operations
match timeout(Duration::from_millis(100), client.health_check(req)).await {
    Ok(Ok(response)) => { /* success */ }
    _ => { /* timeout or error */ }
}
```

## Implementation Steps

### 1. **Use Simulation Proto Definitions**
The simulation crate already has proto definitions in `proto/blixard.proto`. Use these via:
```rust
use blixard_simulation::{
    cluster_service_client::ClusterServiceClient,
    cluster_service_server::{ClusterService, ClusterServiceServer},
    // ... other types
};
```

### 2. **Create Test Service Implementation**
```rust
// Test implementation - NOT using real blixard types
struct TestRaftNode {
    node_id: u64,
    state: Arc<Mutex<RaftState>>,  // Simulated state
    // Simple in-memory collections for testing
}

#[tonic::async_trait]
impl ClusterService for TestRaftNode {
    // Implement the trait methods with test logic
}
```

### 3. **Test Pattern**
```rust
#[madsim::test]
async fn test_scenario() {
    // 1. Setup runtime (optional custom config)
    let handle = Handle::current();
    
    // 2. Create nodes with specific IPs
    let nodes = create_cluster(3).await;
    
    // 3. Start services
    for (node, addr) in &nodes {
        start_raft_service(node, addr).await;
    }
    
    // 4. Form cluster
    form_cluster(&nodes).await;
    
    // 5. Run test scenario
    let client = create_client(&nodes[0].1).await;
    // ... test operations ...
    
    // 6. Inject faults (optional)
    inject_partition(&nodes[0..2], &nodes[2..]).await;
    
    // 7. Verify behavior
    verify_consensus(&nodes).await;
}
```

## Key Patterns to Follow

### 1. **Node Addressing**
- Use IP addresses like `10.0.0.X` for nodes
- Bind services to specific ports
- Use `SocketAddr` for addresses

### 2. **Async Patterns**
- Always await node spawns when you need results
- Use `join_all` for concurrent operations
- Add timeouts to prevent hanging tests

### 3. **Error Handling**
- Use `Result<Response<T>, Status>` for gRPC
- Convert errors appropriately
- Test both success and failure cases

### 4. **Determinism**
- Use fixed IPs and ports
- Control time advancement explicitly
- Use deterministic randomness when needed

### 5. **Resource Cleanup**
- Nodes are automatically cleaned up
- But ensure spawned tasks complete or are cancelled

## Benefits of Proper MadSim Usage

1. **Realistic Testing**: Actual network communication, not mocked
2. **Fault Injection**: Real network partitions, delays, and packet loss
3. **Deterministic**: Same seed produces same execution
4. **Fast**: Time advances instantly, no real waiting
5. **Scalable**: Can test hundreds of nodes efficiently

## Next Steps

1. **Review Proto Definitions**: Ensure `proto/blixard.proto` has all needed messages
2. **Create Test Implementations**: Build test services that simulate Raft behavior
3. **Write Comprehensive Tests**: Cover all distributed failure scenarios
4. **No Integration Needed**: Tests remain separate from production code
5. **Performance Testing**: Use MadSim to test protocol behavior at scale

## Example: Converting a Test

### Before (Mock):
```rust
let mut node = Node::new(config);
node.start().await.unwrap();
assert!(node.is_running().await);
```

### After (MadSim):
```rust
let (node, addr) = create_raft_node(&handle, 1, "10.0.0.1", 7001).await;
sleep(Duration::from_millis(100)).await;

let mut client = create_client(&addr.to_string()).await;
let response = client.health_check(Request::new(HealthCheckRequest {})).await?;
assert_eq!(response.into_inner().status, "healthy");
```

This approach provides much more realistic testing of distributed behavior!