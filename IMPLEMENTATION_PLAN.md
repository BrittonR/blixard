# Blixard Implementation Plan

## Overview
This document outlines a practical implementation plan for Blixard, a distributed service management system that leverages microvm.nix, Tailscale, and RocksDB/redb for a working MVP.

## 1. Project Structure

```
blixard/
├── src/
│   ├── blixard.gleam              # Main application entry
│   ├── cli.gleam                  # CLI command parsing
│   ├── node_manager.gleam         # Node lifecycle management
│   ├── service_manager.gleam      # Service operations
│   ├── storage/
│   │   ├── storage.gleam          # Storage interface
│   │   ├── rocksdb_store.gleam    # RocksDB implementation
│   │   └── redb_store.gleam       # Alternative redb implementation
│   ├── network/
│   │   ├── tailscale.gleam        # Tailscale integration
│   │   └── cluster_discovery.gleam # Node discovery
│   ├── vm/
│   │   ├── microvm.gleam          # MicroVM operations
│   │   └── vm_config.gleam        # VM configuration
│   └── replication/
│       └── sync.gleam             # State synchronization
├── test/
│   ├── integration/
│   │   └── cluster_test.gleam     # Real cluster tests
│   └── unit/
│       └── storage_test.gleam     # Component tests
├── scripts/
│   ├── setup_dev_cluster.sh       # Local development setup
│   ├── test_failover.sh           # Failover testing
│   └── deploy_node.sh             # Node deployment
└── examples/
    ├── simple_service.toml        # Example service config
    └── multi_node_cluster.toml    # Example cluster setup
```

## 2. First Working Prototype Goals

### MVP Features (Phase 1 - 2 weeks)
1. **Single Node Operations**
   - Start/stop services using microvm.nix
   - Store service state in RocksDB
   - Basic CLI interface
   - Tailscale connectivity check

2. **Two-Node Cluster (Phase 2 - 1 week)**
   - Manual cluster join
   - State replication between nodes
   - Basic failover (manual)

3. **Service Management (Phase 3 - 1 week)**
   - Service health monitoring
   - Automatic restart on failure
   - Resource limits configuration

## 3. Core Components Implementation

### 3.1 Storage Layer (RocksDB)

```gleam
// src/storage/rocksdb_store.gleam
import gleam/result
import gleam/dynamic
import rocksdb_ffi

pub type Store {
  Store(db: rocksdb_ffi.Database)
}

pub fn open(path: String) -> Result(Store, String) {
  rocksdb_ffi.open(path)
  |> result.map(Store)
}

pub fn put_service(
  store: Store,
  service_id: String,
  state: ServiceState,
) -> Result(Nil, String) {
  let key = "service:" <> service_id
  let value = encode_service_state(state)
  rocksdb_ffi.put(store.db, key, value)
}

pub fn get_service(
  store: Store,
  service_id: String,
) -> Result(ServiceState, String) {
  let key = "service:" <> service_id
  rocksdb_ffi.get(store.db, key)
  |> result.then(decode_service_state)
}

pub fn list_services(store: Store) -> Result(List(String), String) {
  rocksdb_ffi.prefix_scan(store.db, "service:")
  |> result.map(fn(keys) {
    list.map(keys, fn(key) {
      string.drop_left(key, 8)  // Remove "service:" prefix
    })
  })
}
```

### 3.2 MicroVM Integration

```gleam
// src/vm/microvm.gleam
import gleam/process
import gleam/result
import gleam/json

pub type VMConfig {
  VMConfig(
    name: String,
    memory_mb: Int,
    vcpus: Int,
    image: String,
    network: NetworkConfig,
  )
}

pub fn create_vm(config: VMConfig) -> Result(String, String) {
  // Generate nix expression for microvm
  let nix_config = generate_nix_config(config)
  
  // Write to temporary file
  let config_path = "/tmp/blixard-vm-" <> config.name <> ".nix"
  file.write(config_path, nix_config)
  
  // Execute microvm.nix
  let result = process.run(
    "microvm",
    ["create", "--flake", ".#" <> config.name],
    [process.WorkingDirectory("/tmp")]
  )
  
  case result {
    Ok(output) -> extract_vm_id(output)
    Error(e) -> Error("Failed to create VM: " <> e)
  }
}

pub fn start_vm(vm_id: String) -> Result(Nil, String) {
  process.run("microvm", ["start", vm_id], [])
  |> result.map(fn(_) { Nil })
}

fn generate_nix_config(config: VMConfig) -> String {
  "
  {
    microvm.vms." <> config.name <> " = {
      config = {
        microvm = {
          vcpu = " <> int.to_string(config.vcpus) <> ";
          mem = " <> int.to_string(config.memory_mb) <> ";
          hypervisor = \"qemu\";
          shares = [{
            tag = \"ro-store\";
            source = \"/nix/store\";
            mountPoint = \"/nix/.ro-store\";
          }];
        };
        
        networking.interfaces.eth0 = {
          useDHCP = false;
          addresses = [{
            address = \"10.0.2.\" <> generate_ip_suffix(config.name) <> "\";
            prefixLength = 24;
          }];
        };
        
        services.tailscale.enable = true;
      };
    };
  }
  "
}
```

### 3.3 Tailscale Network Discovery

```gleam
// src/network/tailscale.gleam
import gleam/http
import gleam/json
import gleam/result

pub type Node {
  Node(
    hostname: String,
    tailscale_ip: String,
    online: Bool,
    tags: List(String),
  )
}

pub fn get_local_status() -> Result(Node, String) {
  // Use tailscale CLI to get status
  process.run("tailscale", ["status", "--json"], [])
  |> result.then(parse_tailscale_status)
}

pub fn discover_cluster_nodes() -> Result(List(Node), String) {
  // Get all nodes with blixard tag
  process.run("tailscale", ["status", "--json"], [])
  |> result.then(fn(output) {
    json.parse(output)
    |> result.then(extract_blixard_nodes)
  })
}

fn extract_blixard_nodes(status: Dynamic) -> Result(List(Node), String) {
  // Look for nodes tagged with "tag:blixard-cluster"
  use peers <- result.try(dynamic.field("Peer", dynamic.dict)(status))
  
  peers
  |> dict.to_list
  |> list.filter_map(fn(peer) {
    let #(_, peer_data) = peer
    case has_blixard_tag(peer_data) {
      True -> Ok(parse_node(peer_data))
      False -> Error(Nil)
    }
  })
  |> Ok
}
```

### 3.4 Service Manager

```gleam
// src/service_manager.gleam
import storage/rocksdb_store
import vm/microvm
import network/tailscale

pub type Service {
  Service(
    id: String,
    name: String,
    vm_config: microvm.VMConfig,
    state: ServiceState,
    node: String,
  )
}

pub type ServiceState {
  Stopped
  Starting
  Running(vm_id: String)
  Failed(reason: String)
}

pub fn start_service(
  store: rocksdb_store.Store,
  service: Service,
) -> Result(Service, String) {
  // Check if already running
  case service.state {
    Running(_) -> Error("Service already running")
    _ -> {
      // Create and start VM
      use vm_id <- result.try(microvm.create_vm(service.vm_config))
      use _ <- result.try(microvm.start_vm(vm_id))
      
      // Update state in storage
      let updated_service = Service(..service, state: Running(vm_id))
      use _ <- result.try(rocksdb_store.put_service(
        store,
        service.id,
        updated_service.state
      ))
      
      Ok(updated_service)
    }
  }
}

pub fn stop_service(
  store: rocksdb_store.Store,
  service: Service,
) -> Result(Service, String) {
  case service.state {
    Running(vm_id) -> {
      // Stop the VM
      use _ <- result.try(microvm.stop_vm(vm_id))
      
      // Update state
      let updated_service = Service(..service, state: Stopped)
      use _ <- result.try(rocksdb_store.put_service(
        store,
        service.id,
        updated_service.state
      ))
      
      Ok(updated_service)
    }
    _ -> Error("Service not running")
  }
}
```

### 3.5 CLI Interface

```gleam
// src/cli.gleam
import argv
import gleam/io
import service_manager
import storage/rocksdb_store

pub fn main() {
  let args = argv.load().arguments
  
  case args {
    ["start", service_name] -> handle_start(service_name)
    ["stop", service_name] -> handle_stop(service_name)
    ["list"] -> handle_list()
    ["status", service_name] -> handle_status(service_name)
    ["join", node_address] -> handle_join(node_address)
    _ -> show_usage()
  }
}

fn handle_start(service_name: String) {
  use store <- result.try(rocksdb_store.open("/var/lib/blixard/db"))
  use service <- result.try(load_service_config(service_name))
  
  case service_manager.start_service(store, service) {
    Ok(_) -> io.println("Service " <> service_name <> " started")
    Error(e) -> io.println("Error: " <> e)
  }
}
```

## 4. Development Workflow

### 4.1 Local Development Setup

```bash
#!/bin/bash
# scripts/setup_dev_cluster.sh

# Create data directories
mkdir -p /tmp/blixard-node-{1,2}/db

# Start first node
BLIXARD_NODE_ID=node1 \
BLIXARD_DB_PATH=/tmp/blixard-node-1/db \
BLIXARD_PORT=8001 \
gleam run -- daemon &

# Start second node
BLIXARD_NODE_ID=node2 \
BLIXARD_DB_PATH=/tmp/blixard-node-2/db \
BLIXARD_PORT=8002 \
gleam run -- daemon &

# Wait for nodes to start
sleep 5

# Join nodes
gleam run -- join localhost:8001 --from localhost:8002
```

### 4.2 Testing a Service

```bash
# Create a simple web service
cat > examples/nginx.toml << EOF
[service]
name = "nginx"
memory_mb = 256
vcpus = 1
image = "nginx:alpine"

[network]
ports = ["80:80"]
EOF

# Start the service
gleam run -- start nginx --config examples/nginx.toml

# Check status
gleam run -- status nginx

# Test failover
# Kill node1
pkill -f "BLIXARD_NODE_ID=node1"

# Service should still be accessible via node2
gleam run -- status nginx
```

## 5. Testing Approach

### 5.1 Unit Tests
- Storage layer operations
- Service state transitions
- Configuration parsing

### 5.2 Integration Tests
```gleam
// test/integration/cluster_test.gleam
import gleeunit
import test_helpers

pub fn main() {
  gleeunit.main()
}

pub fn test_service_failover() {
  // Start two-node cluster
  let cluster = test_helpers.start_test_cluster(2)
  
  // Create service on node1
  let service = test_helpers.create_test_service("test-web")
  assert Ok(_) = cluster.node1.start_service(service)
  
  // Verify running
  assert Ok(Running(_)) = cluster.node1.get_service_state("test-web")
  
  // Kill node1
  test_helpers.kill_node(cluster.node1)
  
  // Verify service migrated to node2
  assert Ok(Running(_)) = cluster.node2.get_service_state("test-web")
}
```

### 5.3 Performance Tests
- Measure service start time
- Test concurrent operations
- Benchmark replication latency

## 6. Incremental Milestones

### Week 1: Foundation
- [ ] Basic project structure
- [ ] RocksDB integration
- [ ] Simple CLI parsing
- [ ] Single service start/stop

### Week 2: MicroVM Integration
- [ ] MicroVM wrapper functions
- [ ] Service configuration parsing
- [ ] VM lifecycle management
- [ ] Basic health checks

### Week 3: Clustering
- [ ] Tailscale integration
- [ ] Node discovery
- [ ] State replication protocol
- [ ] Manual failover

### Week 4: Polish & Testing
- [ ] Automated failover
- [ ] Integration test suite
- [ ] Performance optimization
- [ ] Documentation

## 7. Key Design Decisions

### Storage Choice: RocksDB
- Proven reliability
- Good Erlang/Elixir bindings available
- Built-in compression and performance

### Network: Tailscale
- Zero-config secure networking
- Built-in node discovery
- Works across NAT/firewalls

### VM Management: Direct microvm.nix
- No abstraction layer initially
- Direct nix flake generation
- Leverage existing microvm.nix features

## 8. Next Steps

1. Set up development environment with nix flake
2. Implement basic RocksDB storage layer
3. Create minimal CLI with start/stop commands
4. Test with real microvm.nix installation
5. Add Tailscale integration for node discovery
6. Implement basic replication protocol