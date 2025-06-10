# Blixard Technical Architecture

## Integration with microvm.nix

Blixard operates as an orchestration layer on top of microvm.nix:

### 1. microvm.nix handles:
- VM creation and lifecycle (start, stop, destroy)
- Hypervisor abstraction (qemu, firecracker, cloud-hypervisor, crosvm, etc.)
- Resource allocation (CPU, memory, disk)
- Network interface creation
- Filesystem sharing and virtio devices

### 2. Blixard adds:
- Distributed consensus for VM state across nodes
- Intelligent placement and scheduling decisions
- Serverless function runtime and warm pools
- Any-node execution with data locality awareness
- Cluster-wide VM management and monitoring
- Integration with Ceph for distributed storage
- Live migration orchestration between nodes

### 3. Workflow:
```
User Request → Blixard CLI → Raft Consensus → Placement Decision
     ↓
Selected Node → microvm.nix API → Hypervisor → Running VM
     ↓
Blixard monitors and manages the VM lifecycle across the cluster
```

## Core Components

### 1. Consensus Layer (`src/consensus/`)

The consensus layer ensures all cluster nodes agree on VM state and operations:

- **Raft Implementation**: Based on tikv/raft-rs for battle-tested consensus
- **VM State Replication**: All VM lifecycle operations replicated across cluster
- **Storage Coordination**: Coordinate with Ceph for consistent VM placement
- **Leader Election**: Automatic leader selection with failure detection
- **Snapshot Management**: Efficient VM state and cluster metadata transfer

```rust
// Example: VM operation through consensus
let operation = VMOperation::Create {
    vm_name: "webapp-1".to_string(),
    config: VMConfig {
        memory_mb: 512,
        vcpus: 2,
        nix_config: "./webapp.nix".to_string(),
        storage: StorageSpec {
            pool: "ssd-pool".to_string(),
            size_gb: 20,
            replicas: 3,
        },
    },
    target_node: None, // Auto-placement
};

// This goes through Raft consensus before execution
cluster.propose(operation).await?;
```

### 2. Storage Layer (`src/storage/`)

Comprehensive storage management with Ceph integration:

- **Ceph Integration**: RADOS, RBD, and CephFS management
- **VM State Store**: VM definitions, current state, and placement decisions
- **Storage Orchestration**: Pool creation, PG management, rebalancing
- **Configuration Storage**: microvm.nix configs and cluster settings
- **Audit Log**: Complete history of VM and storage operations
- **Backup/Restore**: VM snapshots and cross-region replication

```rust
// VM state is strongly consistent across cluster
pub struct VMState {
    pub name: String,
    pub status: VMStatus,  // Creating, Running, Migrating, Stopped, Failed
    pub node_id: Option<String>,
    pub hypervisor: HypervisorType,  // Firecracker, CloudHypervisor, QEMU
    pub config: VMConfig,
    pub storage: StorageAllocation,
    pub network: NetworkConfig,
    pub health: HealthStatus,
    pub metrics: VMMetrics,
    pub last_updated: SystemTime,
}

pub struct StorageAllocation {
    pub rbd_volumes: Vec<RBDVolume>,
    pub cephfs_mounts: Vec<CephFSMount>,
    pub object_buckets: Vec<ObjectBucket>,
}
```

### 3. MicroVM Management (`src/microvm/`)

Advanced VM lifecycle management with dual operational modes:

#### microvm.nix Integration:
- Blixard calls microvm.nix to create/manage VMs on each node
- Translates high-level orchestration decisions to microvm.nix operations
- Monitors VM state through microvm.nix interfaces

#### Dual VM Types:
- **Long-lived VMs**: Traditional lifecycle via microvm.nix
- **Serverless VMs**: Rapid lifecycle with pre-configured microvm.nix templates

#### Key Features:
- **Hypervisor Selection**: Uses microvm.nix's hypervisor abstraction
- **Serverless Runtime**: Sub-100ms cold starts with Firecracker
- **Smart Placement Engine**: Different strategies for long-lived vs serverless
- **Resource Management**: Fixed allocation vs overcommit
- **Live Migration**: Zero-downtime movement (long-lived VMs only)
- **Health Monitoring**: VM health checks, function error rates, cold start metrics

```rust
// Example: Dual VM configurations
pub enum VMConfig {
    LongLived(LongLivedVMConfig),
    Serverless(ServerlessVMConfig),
}

pub struct LongLivedVMConfig {
    pub name: String,
    pub microvm_nix_flake: String,      // microvm.nix flake reference
    pub microvm_nix_config: PathBuf,    // Path to microvm.nix configuration
    pub hypervisor: HypervisorType,     // Which microvm.nix hypervisor to use
    pub resources: FixedVMResources,
    pub storage: StorageRequirements,
    pub network: NetworkRequirements,
    pub dependencies: Vec<String>,
    pub health_check: HealthCheckConfig,
    pub placement: PlacementPolicy,
}

pub struct ServerlessVMConfig {
    pub name: String,
    pub microvm_nix_template: String,  // Pre-built microvm.nix template for fast boot
    pub handler_path: PathBuf,         // Nix function definition
    pub memory_mb: u32,                // Max memory
    pub timeout_ms: u32,               // Max execution time
    pub triggers: Vec<TriggerConfig>,  // HTTP, queue, event, cron
    pub scaling: ScalingPolicy,
    pub placement: DynamicPlacement,   // Flexible placement
}

pub struct DynamicPlacement {
    pub strategy: PlacementStrategy,
    pub constraints: Vec<Constraint>,
    pub preferences: Vec<Preference>,
}

pub enum PlacementStrategy {
    // Run on ANY available node
    BestFit,           // Optimal resource utilization
    DataLocality,      // Near the data being processed  
    NetworkProximity,  // Minimize latency to caller
    LoadBalanced,      // Spread across cluster
    // Can combine strategies
    Combined(Vec<PlacementStrategy>),
}

pub struct ScalingPolicy {
    pub min_instances: u32,     // Can be 0 for scale-to-zero
    pub max_instances: u32,     // Max concurrent executions
    pub target_concurrency: u32, // Invocations per instance
    pub scale_down_delay_ms: u32, // Keep warm duration
}
```

### 4. Networking Layer (`src/network/`)

Comprehensive network management for VM isolation and communication:

- **Virtual Networking**: VLAN, VXLAN, and bridge management for VM isolation
- **gRPC Communication**: Efficient inter-node cluster communication
- **Service Discovery**: Automatic node discovery via Tailscale mesh networking
- **Load Balancing**: L4/L7 load balancing for VM services
- **Network Partition Handling**: Graceful degradation with storage quorum
- **Ingress/Egress**: Traffic routing and firewall management
- **Multi-tenancy**: Network isolation between different VM workloads

### 5. CLI Interface (`src/cli/`)

Rich command-line interface for all operations:

- **VM Management**: kubectl-like commands for VM lifecycle operations
- **Storage Operations**: Ceph pool, volume, and filesystem management
- **Configuration Management**: Validate and distribute microvm.nix configurations
- **Interactive Mode**: REPL-style interface for exploration and debugging
- **GitOps Integration**: Apply configurations from Git repositories
- **Output Formatting**: JSON, YAML, table formats with filtering
- **Autocomplete**: Shell completion for all commands and resources

```bash
# Blixard orchestrates microvm.nix VMs across the cluster
# The nginx.nix file is a standard microvm.nix configuration
blixard vm create nginx --microvm-config nginx.nix --replicas 3 \
  --storage-class ssd
# Blixard selects nodes and calls microvm.nix on each to create VMs

blixard vm start nginx
# Triggers 'microvm -c nginx.nix' on selected nodes

blixard vm status nginx --format json
blixard vm logs nginx --follow --tail 100
blixard vm migrate nginx-1 --target-node node2 --live

# Storage management
blixard storage pool create ssd-pool --type replicated --size 1TB
blixard storage volume create app-data --pool ssd-pool --size 50G
blixard storage fs create shared-data --pool ssd-pool

# Cluster operations
blixard cluster status --verbose
blixard cluster join --node-id node4 --address 10.0.0.4
blixard cluster storage add-osd /dev/nvme1n1 --weight 1.0
```