# Raft Consensus Design for Blixard

## Overview

Blixard uses Khepri, a Raft-based distributed storage system, to maintain consensus across cluster nodes for critical metadata. This document explains how Raft consensus ensures consistency for VM management operations and service orchestration.

## 1. Metadata Requiring Consensus

### 1.1 Service State Metadata
All service state transitions require consensus to ensure cluster-wide consistency:

```gleam
// Service state stored in Khepri with consensus
type ServiceInfo {
  state: String        // "Running", "Stopped", "Failed"
  node: String        // Node managing the service
  timestamp: String   // Last state change timestamp
}
```

Key metadata paths in Khepri:
- `/:services/<service-name>` - Service state and ownership
- `/:nodes/<node-name>` - Node membership and health
- `/:placement/<service-name>` - Service placement decisions
- `/:config/<setting>` - Cluster-wide configuration

### 1.2 VM and MicroVM Metadata (Future)
When integrated with microvm.nix, the following metadata will require consensus:

```gleam
type VMMetadata {
  vm_id: String
  state: VMState          // Starting, Running, Stopping, Stopped
  host_node: String       // Physical node hosting the VM
  config_hash: String     // Hash of VM configuration
  resources: VMResources  // CPU, memory allocations
  network: NetworkConfig  // IP assignments, VLAN tags
}

type PlacementDecision {
  vm_id: String
  target_node: String
  migration_state: Option(MigrationState)
  constraints: List(PlacementConstraint)
}
```

### 1.3 Cluster Membership
Node membership changes must go through Raft consensus:

```gleam
type NodeInfo {
  node_name: String
  status: NodeStatus      // Active, Draining, Failed
  capabilities: List(String)
  last_heartbeat: Int
}
```

## 2. How Raft Ensures Consistency

### 2.1 Raft Protocol in Khepri

Khepri implements the Raft consensus algorithm with the following guarantees:

1. **Leader Election**: One node acts as the Raft leader, handling all write operations
2. **Log Replication**: All state changes are replicated to a majority of nodes
3. **Safety**: Only committed entries (replicated to majority) are applied
4. **Linearizability**: All operations appear to execute atomically at some point between their start and completion

### 2.2 Write Path Example

Here's how a service start operation flows through Raft consensus:

```gleam
// In service_handlers.gleam
pub fn start_service(service_name: String, node_info: NodeInfo) -> Result(Nil, String) {
  // Step 1: Check current state (read from local Khepri replica)
  let service_path = "/:services/" <> service_name
  
  case khepri_store.get(service_path) {
    Ok(Some(_)) -> {
      // Step 2: Propose state change to Raft leader
      let new_state = ServiceInfo(
        state: "Running",
        node: node_info.node_name,
        timestamp: current_timestamp()
      )
      
      // Step 3: Khepri handles Raft consensus internally
      // - Leader receives write request
      // - Leader appends to its log
      // - Leader replicates to followers
      // - Once majority ack, leader commits
      // - Leader notifies followers to commit
      case khepri_store.put(service_path, new_state) {
        Ok(_) -> {
          // Step 4: State change committed across cluster
          systemd.start_service(service_name, node_info.is_user)
        }
        Error(e) -> Error("Consensus failed: " <> e)
      }
    }
    _ -> Error("Service not found")
  }
}
```

### 2.3 Read Consistency

Blixard supports different read consistency levels:

```gleam
// Strong consistency - read from leader
pub fn get_service_info_consistent(service: String) -> Result(ServiceInfo, String) {
  khepri_store.get_consistent("/:services/" <> service)
}

// Eventual consistency - read from local replica
pub fn get_service_info_fast(service: String) -> Result(ServiceInfo, String) {
  khepri_store.get("/:services/" <> service)
}
```

## 3. Raft and MicroVM.nix Integration

### 3.1 VM Lifecycle Coordination

When integrated with microvm.nix, Raft consensus coordinates VM operations:

```gleam
// Proposed VM management flow
pub fn start_vm(vm_config: VMConfig) -> Result(String, String) {
  // Step 1: Choose placement through consensus
  let placement = case choose_vm_placement(vm_config) {
    Ok(node) -> node
    Error(e) -> return Error(e)
  }
  
  // Step 2: Reserve resources via Raft
  let reservation_path = "/:reservations/" <> vm_config.vm_id
  let reservation = ResourceReservation(
    vm_id: vm_config.vm_id,
    node: placement,
    cpu: vm_config.cpu,
    memory: vm_config.memory,
    state: "Pending"
  )
  
  case khepri_store.put_if_absent(reservation_path, reservation) {
    Ok(_) -> {
      // Step 3: Trigger VM start on target node
      case rpc_to_node(placement, "start_microvm", [vm_config]) {
        Ok(_) -> {
          // Step 4: Update state in Khepri
          update_vm_state(vm_config.vm_id, "Running", placement)
        }
        Error(e) -> {
          // Rollback reservation
          khepri_store.delete(reservation_path)
          Error("Failed to start VM: " <> e)
        }
      }
    }
    Error(_) -> Error("Resource reservation failed")
  }
}
```

### 3.2 Placement Decision Consensus

VM placement decisions go through Raft to prevent conflicts:

```gleam
pub fn choose_vm_placement(vm_config: VMConfig) -> Result(String, String) {
  // Step 1: Get current cluster state (consistent read)
  let nodes = khepri_store.get_consistent("/:nodes/")
  let reservations = khepri_store.get_consistent("/:reservations/")
  
  // Step 2: Calculate available resources
  let available_nodes = calculate_available_capacity(nodes, reservations)
  
  // Step 3: Apply placement constraints
  let suitable_nodes = filter_by_constraints(available_nodes, vm_config.constraints)
  
  // Step 4: Select best node
  case suitable_nodes {
    [] -> Error("No suitable nodes available")
    [node, ..] -> Ok(node)
  }
}
```

## 4. Handling Network Partitions and Failures

### 4.1 Network Partition Behavior

Raft's design ensures safety during network partitions:

```
Partition Scenario:
[Node A, Node B] | [Node C, Node D, Node E]
     Minority    |        Majority

Behavior:
- Majority partition (C,D,E) elects new leader, continues operations
- Minority partition (A,B) cannot achieve consensus, becomes read-only
- Writes to minority fail with "no quorum" errors
```

### 4.2 Failure Detection and Recovery

```gleam
// In replication_monitor.gleam
pub fn monitor_cluster_health() -> Result(ClusterHealth, String) {
  // Periodic health checks
  let node_states = khepri_store.get("/:nodes/")
  
  case node_states {
    Ok(nodes) -> {
      let current_time = erlang.system_time(erlang.Second)
      let health_status = list.map(nodes, fn(node) {
        let last_seen = node.last_heartbeat
        let time_since = current_time - last_seen
        
        case time_since {
          t if t < 5 -> NodeHealth(node.name, "Healthy")
          t if t < 30 -> NodeHealth(node.name, "Degraded")
          _ -> NodeHealth(node.name, "Failed")
        }
      })
      
      // Update node states via Raft
      update_node_health_status(health_status)
    }
    Error(e) -> Error("Failed to check cluster health: " <> e)
  }
}
```

### 4.3 Split-Brain Prevention

Raft prevents split-brain through its majority requirement:

```gleam
// Example: Service failover with split-brain prevention
pub fn handle_node_failure(failed_node: String) -> Result(Nil, String) {
  // Get services on failed node (requires consensus read)
  let affected_services = khepri_store.get_consistent("/:placement/")
    |> result.map(fn(placements) {
      dict.filter(placements, fn(_, node) { node == failed_node })
    })
  
  // Reassign each service (requires consensus write)
  case affected_services {
    Ok(services) -> {
      list.try_each(dict.keys(services), fn(service) {
        // This will fail if we don't have quorum
        reassign_service(service, failed_node)
      })
    }
    Error(e) -> Error("Cannot access placement data: " <> e)
  }
}

fn reassign_service(service: String, failed_node: String) -> Result(Nil, String) {
  // Atomic compare-and-swap ensures no concurrent modifications
  let path = "/:placement/" <> service
  
  khepri_store.compare_and_swap(
    path,
    failed_node,  // Expected current value
    choose_new_node(service)  // New value
  )
}
```

## 5. Complete Consensus Flow Example

Here's a complete example showing how consensus works for a service migration:

```gleam
// Service migration with full consensus flow
pub fn migrate_service(service: String, from_node: String, to_node: String) -> Result(Nil, String) {
  // Phase 1: Prepare migration (consensus write)
  let migration_id = generate_migration_id()
  let migration_path = "/:migrations/" <> migration_id
  let migration_state = MigrationState(
    service: service,
    from: from_node,
    to: to_node,
    state: "Preparing",
    started_at: current_timestamp()
  )
  
  // This write goes through Raft consensus
  case khepri_store.put(migration_path, migration_state) {
    Error(e) -> Error("Failed to initiate migration: " <> e)
    Ok(_) -> {
      // Phase 2: Stop service on source (consensus write)
      let service_path = "/:services/" <> service
      case khepri_store.compare_and_swap(
        service_path,
        ServiceInfo("Running", from_node, _),
        ServiceInfo("Migrating", from_node, current_timestamp())
      ) {
        Error(_) -> {
          rollback_migration(migration_id)
          Error("Service state changed during migration")
        }
        Ok(_) -> {
          // Phase 3: Execute migration
          case execute_migration_steps(service, from_node, to_node) {
            Ok(_) -> {
              // Phase 4: Update final state (consensus write)
              khepri_store.put(
                service_path,
                ServiceInfo("Running", to_node, current_timestamp())
              )
              
              // Phase 5: Clean up migration record
              khepri_store.delete(migration_path)
            }
            Error(e) -> {
              // Rollback on failure
              rollback_migration(migration_id)
              Error("Migration failed: " <> e)
            }
          }
        }
      }
    }
  }
}

fn execute_migration_steps(service: String, from: String, to: String) -> Result(Nil, String) {
  // Step 1: Pre-flight checks on target
  case rpc_to_node(to, "can_accept_service", [service]) {
    Error(e) -> Error("Target node cannot accept service: " <> e)
    Ok(_) -> {
      // Step 2: Stop on source
      case rpc_to_node(from, "stop_service", [service]) {
        Error(e) -> Error("Failed to stop on source: " <> e)
        Ok(_) -> {
          // Step 3: Start on target
          case rpc_to_node(to, "start_service", [service]) {
            Error(e) -> Error("Failed to start on target: " <> e)
            Ok(_) -> Ok(Nil)
          }
        }
      }
    }
  }
}
```

## 6. Performance Considerations

### 6.1 Write Latency
- Writes require round-trip to majority of nodes
- Typical latency: 10-50ms in LAN environment
- Batching updates can improve throughput

### 6.2 Read Optimization
```gleam
// Use eventual consistency for non-critical reads
pub fn list_all_services() -> Result(List(String), String) {
  // Read from local replica (no consensus overhead)
  khepri_store.get("/:services/")
}

// Use strong consistency for critical decisions
pub fn verify_service_ownership(service: String, node: String) -> Result(Bool, String) {
  // Read through leader (consensus overhead)
  khepri_store.get_consistent("/:services/" <> service)
    |> result.map(fn(info) { info.node == node })
}
```

### 6.3 Consensus Optimization Strategies
1. **Batch Operations**: Group related updates into single consensus round
2. **Read Leases**: Cache read results with TTL for frequently accessed data
3. **Local Reads**: Use eventual consistency where strong consistency isn't required
4. **Pre-computed Views**: Maintain denormalized views for complex queries

## 7. Future Enhancements

### 7.1 Multi-Raft Support
For scaling beyond single Raft group:
- Shard services across multiple Raft groups
- Use consistent hashing for shard assignment
- Cross-shard transactions via 2PC

### 7.2 Geo-Distribution
For multi-datacenter deployments:
- Raft learners for read replicas
- Preferred leader election based on geography
- Async replication between regions

### 7.3 Performance Monitoring
- Raft operation latency metrics
- Consensus round success rates
- Leader election frequency tracking

## Conclusion

Blixard's use of Raft consensus through Khepri provides strong consistency guarantees for critical metadata while maintaining high availability. The system gracefully handles failures and network partitions, ensuring that service management operations remain safe and consistent across the cluster.