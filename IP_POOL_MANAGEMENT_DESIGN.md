# IP Pool Management Design for Blixard

## Overview

This document outlines the design for a distributed IP pool management system for Blixard that integrates with the existing Raft consensus mechanism to ensure consistent IP address allocation across the cluster.

## Current State Analysis

### Existing Network Implementation

1. **VM Configuration**: 
   - Core VmConfig has `ip_address: Option<String>` field in `blixard-core/src/types.rs`
   - VM backend (blixard-vm) uses a more detailed network configuration with routed networking

2. **IP Allocation**:
   - Currently implemented in `MicrovmBackend` with a local `IpAddressPool` struct
   - Uses hardcoded subnet 10.0.0.0/24 with gateway at 10.0.0.1
   - VMs allocated IPs starting from 10.0.0.10
   - MAC addresses generated deterministically from VM name hash

3. **Storage**: 
   - VM states stored in Raft-managed storage via `VM_STATE_TABLE`
   - IP addresses stored as part of VmState but not tracked globally

4. **Consensus Integration**:
   - VM operations go through Raft consensus via `ProposalData::CreateVm`
   - State changes applied in `apply_vm_command` method

## Design Goals

1. **Distributed Consistency**: Ensure IP addresses are never double-allocated across nodes
2. **Multiple Subnet Support**: Allow configuration of multiple IP pools/subnets
3. **Efficient Allocation**: Quick IP allocation without requiring Raft consensus for each query
4. **Graceful Recovery**: Handle node failures and IP release on VM deletion
5. **Backward Compatibility**: Integrate smoothly with existing VM creation flow

## Proposed Architecture

### 1. Data Structures

```rust
// New types to add to blixard-core/src/types.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpPool {
    /// Unique identifier for the pool
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Subnet CIDR (e.g., "10.0.0.0/24")
    pub subnet: String,
    /// Gateway IP address
    pub gateway: Ipv4Addr,
    /// First allocatable IP
    pub allocation_start: Ipv4Addr,
    /// Last allocatable IP
    pub allocation_end: Ipv4Addr,
    /// Optional VLAN tag
    pub vlan_tag: Option<u16>,
    /// Datacenter/zone affinity
    pub topology_affinity: Option<NodeTopology>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpAllocation {
    /// IP address
    pub ip: Ipv4Addr,
    /// Pool ID this IP belongs to
    pub pool_id: String,
    /// VM name holding this IP
    pub vm_name: String,
    /// Node ID that allocated this IP
    pub allocated_by_node: u64,
    /// Allocation timestamp
    pub allocated_at: DateTime<Utc>,
    /// MAC address associated with this IP
    pub mac_address: String,
}

// New commands for Raft consensus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpPoolCommand {
    CreatePool { pool: IpPool },
    DeletePool { pool_id: String },
    AllocateIp { 
        pool_id: String, 
        vm_name: String,
        preferred_ip: Option<Ipv4Addr>,
    },
    ReleaseIp { 
        ip: Ipv4Addr,
        vm_name: String,
    },
    UpdatePool { 
        pool_id: String, 
        updates: IpPoolUpdate,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpPoolUpdate {
    pub name: Option<String>,
    pub vlan_tag: Option<Option<u16>>,
    pub topology_affinity: Option<Option<NodeTopology>>,
}
```

### 2. Storage Tables

Add new tables to `blixard-core/src/storage.rs`:

```rust
// IP pool management tables
pub const IP_POOL_TABLE: TableDefinition<&str, &[u8]> = 
    TableDefinition::new("ip_pools");
pub const IP_ALLOCATION_TABLE: TableDefinition<&str, &[u8]> = 
    TableDefinition::new("ip_allocations");
// Index by IP for fast lookup
pub const IP_ALLOCATION_BY_IP_TABLE: TableDefinition<&[u8], &[u8]> = 
    TableDefinition::new("ip_allocations_by_ip");
```

### 3. Raft Integration

Extend `ProposalData` enum in `raft_manager.rs`:

```rust
pub enum ProposalData {
    // ... existing variants ...
    IpPoolOperation(IpPoolCommand),
}
```

### 4. IP Pool Manager Component

Create new file `blixard-core/src/ip_pool_manager.rs`:

```rust
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::sync::RwLock;
use redb::Database;

pub struct IpPoolManager {
    database: Arc<Database>,
    // Local cache of pool states for fast allocation queries
    pool_cache: Arc<RwLock<HashMap<String, IpPoolState>>>,
}

#[derive(Debug, Clone)]
struct IpPoolState {
    pool: IpPool,
    allocated_ips: HashSet<Ipv4Addr>,
    next_ip: Ipv4Addr,
}

impl IpPoolManager {
    pub fn new(database: Arc<Database>) -> Self {
        Self {
            database,
            pool_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initialize cache from database
    pub async fn initialize(&self) -> BlixardResult<()> {
        self.rebuild_cache_from_db().await
    }

    /// Get allocation suggestions without going through Raft
    pub async fn suggest_ip_allocation(
        &self,
        pool_id: &str,
        topology_hint: Option<&NodeTopology>,
    ) -> BlixardResult<Ipv4Addr> {
        let cache = self.pool_cache.read().await;
        
        // Find best matching pool based on topology affinity
        let pool_state = if let Some(topology) = topology_hint {
            self.find_best_pool_for_topology(&cache, topology, pool_id)?
        } else {
            cache.get(pool_id)
                .ok_or_else(|| BlixardError::NotFound {
                    resource: format!("IP pool: {}", pool_id),
                })?
        };

        // Find next available IP
        self.find_next_available_ip(pool_state)
    }

    /// Apply IP pool command from Raft consensus
    pub async fn apply_ip_pool_command(
        &self,
        command: &IpPoolCommand,
        txn: WriteTransaction,
    ) -> BlixardResult<()> {
        match command {
            IpPoolCommand::CreatePool { pool } => {
                self.apply_create_pool(txn, pool).await
            }
            IpPoolCommand::AllocateIp { pool_id, vm_name, preferred_ip } => {
                self.apply_allocate_ip(txn, pool_id, vm_name, preferred_ip).await
            }
            IpPoolCommand::ReleaseIp { ip, vm_name } => {
                self.apply_release_ip(txn, ip, vm_name).await
            }
            // ... other commands ...
        }
    }

    /// Rebuild local cache from database (called after Raft updates)
    async fn rebuild_cache_from_db(&self) -> BlixardResult<()> {
        // Implementation to read all pools and allocations from DB
        // and rebuild the in-memory cache
    }
}
```

### 5. Integration with VM Creation Flow

Modify the VM creation process to integrate IP allocation:

1. **Pre-allocation Phase** (before Raft proposal):
   ```rust
   // In vm_scheduler.rs or similar
   let suggested_ip = ip_pool_manager.suggest_ip_allocation(
       &pool_id,
       Some(&selected_node.topology)
   ).await?;
   ```

2. **Raft Proposal Phase**:
   ```rust
   // Create batch proposal for VM creation + IP allocation
   let proposals = vec![
       ProposalData::IpPoolOperation(IpPoolCommand::AllocateIp {
           pool_id: pool_id.clone(),
           vm_name: vm_config.name.clone(),
           preferred_ip: Some(suggested_ip),
       }),
       ProposalData::CreateVm(VmCommand::Create {
           config: vm_config_with_ip,
           node_id: selected_node,
       }),
   ];
   
   // Submit as batch for atomicity
   raft_manager.propose(ProposalData::Batch(proposals)).await?;
   ```

3. **Rollback on Failure**:
   - If VM creation fails, automatically release the allocated IP
   - Use Raft's transaction semantics to ensure atomicity

### 6. Network Configuration Integration

Update `MicrovmBackend` to use the centrally allocated IP:

```rust
impl MicrovmBackend {
    async fn convert_config(&self, core_config: &CoreVmConfig) -> BlixardResult<vm_types::VmConfig> {
        // Use IP from core_config if present, otherwise error
        let vm_ip = core_config.ip_address.as_ref()
            .ok_or_else(|| BlixardError::VmOperationFailed {
                operation: "convert_config".to_string(),
                details: "No IP address allocated for VM".to_string(),
            })?;

        // Parse IP and generate network config
        let ip_addr: Ipv4Addr = vm_ip.parse()?;
        let mac_address = self.generate_mac_for_ip(ip_addr);
        
        // Look up pool info to get gateway and subnet
        let pool_info = self.get_pool_info_for_ip(ip_addr).await?;
        
        // Create routed network config
        let network = NetworkConfig::Routed {
            id: format!("vm-{}", core_config.name),
            mac: mac_address,
            ip: vm_ip.clone(),
            gateway: pool_info.gateway.to_string(),
            subnet: pool_info.subnet.clone(),
        };
        
        // ... rest of conversion ...
    }
}
```

### 7. CLI Commands

Add new CLI commands for IP pool management:

```bash
# Create a new IP pool
blixard ip-pool create --name production \
  --subnet 10.0.0.0/24 \
  --gateway 10.0.0.1 \
  --start 10.0.0.10 \
  --end 10.0.0.250 \
  --datacenter us-east-1

# List IP pools
blixard ip-pool list

# Show pool details and allocations
blixard ip-pool show production

# Delete unused pool
blixard ip-pool delete staging

# List all IP allocations
blixard ip-pool allocations

# Manual IP allocation (for debugging)
blixard ip-pool allocate --pool production --vm test-vm
```

### 8. Migration Strategy

For existing deployments:

1. **Phase 1**: Deploy code with IP pool support but keep using local allocation
2. **Phase 2**: Create default IP pool matching current subnet (10.0.0.0/24)
3. **Phase 3**: Migrate existing VMs to track their IPs in the new system
4. **Phase 4**: Switch to centralized IP allocation for new VMs
5. **Phase 5**: Remove old local IP allocation code

### 9. Monitoring and Observability

Add metrics for:
- IP pool utilization percentage
- IP allocation/release rate
- IP allocation failures
- Pool cache hit rate
- Raft proposal latency for IP operations

## Benefits

1. **Consistency**: All IP allocations go through Raft consensus
2. **Flexibility**: Support for multiple subnets and pools
3. **Performance**: Local cache allows fast allocation suggestions
4. **Reliability**: Automatic cleanup on VM deletion
5. **Scalability**: Can handle thousands of VMs across multiple pools

## Future Enhancements

1. **IPv6 Support**: Extend to support IPv6 address pools
2. **DHCP Integration**: Option to use external DHCP servers
3. **IP Reservation**: Pre-reserve IPs for specific VMs
4. **Pool Quotas**: Per-tenant IP allocation limits
5. **Automatic Pool Expansion**: Dynamically add new subnets when pools fill up