# Proto to Rust Type Mapping Plan for Iroh Migration

This document outlines the mapping plan to replace Protocol Buffer types with native Rust types for Iroh serialization.

## Analysis Summary

Based on codebase analysis, the most commonly used proto types are:
1. Request/Response types for RPC methods (ClusterStatusRequest, TaskRequest, etc.)
2. Data types (NodeInfo, VmInfo, VmState, etc.)
3. Service definitions (ClusterService, BlixardService)

## Core Data Types Mapping

### 1. Node Management Types

#### proto::NodeInfo → iroh_types::NodeInfo
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: u64,
    pub address: String,
    pub state: NodeState,
    pub p2p_node_id: String,  // Iroh node ID (base64 encoded)
    pub p2p_addresses: Vec<String>,  // Direct P2P addresses
    pub p2p_relay_url: String,  // Relay server URL if available
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum NodeState {
    Unknown,
    Follower,
    Candidate,
    Leader,
}
```

### 2. VM Management Types

#### proto::VmInfo → Already exists as VmInfoData in transport/services/vm.rs
```rust
// Already defined in src/transport/services/vm.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmInfoData {
    pub name: String,
    pub state: String,  // Will be converted to VmState enum
    pub vcpus: u32,
    pub memory_mb: u32,
    pub node_id: u64,
    pub ip_address: String,
}
```

#### proto::VmState → Use existing types::VmStatus
- Already exists in src/types.rs
- Just need to ensure serialization compatibility

### 3. Request/Response Types

#### Cluster Management
```rust
// Join Cluster
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinRequest {
    pub node_id: u64,
    pub bind_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinResponse {
    pub success: bool,
    pub message: String,
    pub peers: Vec<NodeInfo>,
    pub voters: Vec<u64>,  // Current voting members
}

// Leave Cluster
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaveRequest {
    pub node_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaveResponse {
    pub success: bool,
    pub message: String,
}

// Cluster Status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStatusRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStatusResponse {
    pub leader_id: u64,
    pub nodes: Vec<NodeInfo>,
    pub term: u64,
}
```

#### VM Operations (partially exists in vm.rs)
```rust
// Extend existing VmOperationRequest/Response enums
// Most VM request/response types are already handled by the enums in vm.rs
```

#### Task Management
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequest {
    pub task_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub required_features: Vec<String>,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResponse {
    pub accepted: bool,
    pub message: String,
    pub assigned_node: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusRequest {
    pub task_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusResponse {
    pub found: bool,
    pub status: TaskStatus,
    pub output: String,
    pub error: String,
    pub execution_time_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TaskStatus {
    Unknown,
    Pending,
    Running,
    Completed,
    Failed,
}
```

### 4. Health Check Types
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResponse {
    pub healthy: bool,
    pub message: String,
}
```

### 5. Raft Communication
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftMessageRequest {
    pub raft_data: Vec<u8>,  // Serialized raft::prelude::Message
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftMessageResponse {
    pub success: bool,
    pub error: String,
}
```

### 6. Scheduling Types
```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PlacementStrategy {
    MostAvailable,
    LeastAvailable,
    RoundRobin,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterResourceSummary {
    pub total_nodes: u32,
    pub total_vcpus: u32,
    pub used_vcpus: u32,
    pub total_memory_mb: u64,
    pub used_memory_mb: u64,
    pub total_disk_gb: u64,
    pub used_disk_gb: u64,
    pub total_running_vms: u32,
    pub nodes: Vec<NodeResourceUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResourceUsage {
    pub node_id: u64,
    pub capabilities: WorkerCapabilities,
    pub used_vcpus: u32,
    pub used_memory_mb: u64,
    pub used_disk_gb: u64,
    pub running_vms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerCapabilities {
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub features: Vec<String>,
}
```

### 7. P2P Types
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2pPeerInfo {
    pub node_id: String,
    pub address: String,
    pub last_seen: String,
    pub is_connected: bool,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2pStats {
    pub total_bytes_sent: u64,
    pub total_bytes_received: u64,
    pub active_transfers: u32,
    pub cached_images: u32,
    pub cache_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2pImageInfo {
    pub name: String,
    pub version: String,
    pub hash: String,
    pub size_bytes: u64,
    pub created_at: String,
    pub metadata: std::collections::HashMap<String, String>,
    pub is_cached: bool,
}
```

## Migration Strategy

### Phase 1: Create Rust Types Module
1. Create `src/iroh_types.rs` with all the type definitions
2. Implement conversion traits where needed for compatibility
3. Add serialization tests to ensure compatibility

### Phase 2: Update Service Implementations
1. Update transport services to use new types
2. Replace proto imports with iroh_types imports
3. Update dual_service_runner to use new types

### Phase 3: Remove Proto Dependencies
1. Remove proto file and build.rs proto compilation
2. Remove tonic-build dependency
3. Update all imports and usages

### Implementation Priority
1. **High Priority** (most used):
   - ClusterStatusRequest/Response
   - TaskRequest/Response
   - HealthCheckRequest/Response
   - NodeInfo
   - VmInfo/VmState

2. **Medium Priority**:
   - JoinRequest/Response
   - LeaveRequest/Response
   - RaftMessageRequest/Response
   - VM operation types (partially done)

3. **Lower Priority**:
   - P2P types
   - Test service types
   - Streaming service types

## Benefits of Migration
1. Single serialization format (serde) instead of protobuf + serde
2. Simpler build process (no proto compilation)
3. Better integration with Iroh's native types
4. More idiomatic Rust code
5. Easier to extend and modify types