# Gleam to Rust Migration Plan: MicroVM Orchestration Platform

This document outlines the migration strategy for converting Blixard from a Gleam/Erlang service manager to a **Rust-based distributed microVM orchestration platform** with **microvm.nix** and **Ceph storage** integration.

## Executive Summary

Blixard is evolving from a distributed service management system to a **NixOS-native distributed microVM orchestration platform** built on Rust. This migration plan details the conversion from Gleam/Erlang to Rust while **significantly expanding** capabilities to include microVM management with **microvm.nix**, **Ceph distributed storage**, and enterprise-grade virtualization features—all designed exclusively for **NixOS** environments.

**Vision**: Transform Blixard into \"Kubernetes for MicroVMs on NixOS\" - providing enterprise-grade orchestration for virtual machines with the immutability and reproducibility of the Nix ecosystem, stronger security isolation than containers, and completely declarative infrastructure management.

## Current Architecture Analysis

### Core Components (Gleam/Erlang)
- **service_manager.gleam** - CLI entry point and argument parsing
- **node_manager.gleam** - Erlang distribution and cluster formation
- **khepri_store.gleam** - Khepri distributed key-value store interface
- **service_handlers.gleam** - Service management business logic
- **systemd.gleam** - systemd interaction via shell commands
- **cluster_discovery.gleam** - Tailscale-based node discovery
- **replication_monitor.gleam** - Cluster health monitoring

### Key Dependencies
- **Khepri**: Raft-based distributed consensus store
- **Erlang/OTP**: Distribution and clustering primitives
- **Tailscale**: Secure networking between nodes
- **systemd**: Service lifecycle management

## Migration Strategy

### Phase 1: Foundation, Architecture, and Testing Infrastructure

#### 1.1 Rust Ecosystem Selection
- **Distributed Systems**: Replace Erlang distribution with custom Rust solution
- **Consensus Store**: **CRITICAL - RETAIN RAFT CONSENSUS PROPERTIES**
  - **Primary**: `tikv/raft-rs` - production-grade Raft implementation used by TiKV
  - **Alternative**: `raft` crate if tikv/raft-rs proves too complex
  - **Storage**: `sled` or `redb` for embedded database
  - **Non-negotiable**: Must maintain strong consistency, partition tolerance, and consensus safety
  - **Testing**: Implement deterministic simulation testing from day one (see Phase 1.3)
- **Networking**: `tokio` + `tonic` (gRPC) or `tarpc` for RPC
- **Service Discovery**: Continue with Tailscale integration
- **CLI Framework**: `clap` for argument parsing
- **Async Runtime**: `tokio`

#### 1.2 Deterministic Testing Infrastructure (Critical Foundation)

**MUST IMPLEMENT BEFORE ANY CORE LOGIC**

Based on TigerBeetle/FoundationDB methodologies documented in `docs/advanced_testing_*`:

```
tests/
├── simulation/
│   ├── mod.rs               # Simulation framework
│   ├── environment.rs       # Deterministic environment
│   ├── clock.rs             # Simulated time control
│   ├── network.rs           # Network simulation
│   └── faults.rs            # Fault injection
├── consensus/
│   ├── raft_properties.rs   # Raft consensus properties
│   ├── linearizability.rs   # Consistency checking
│   └── partition_tests.rs   # Network partition scenarios
├── property/
│   ├── service_mgmt.rs      # Service management properties
│   └── cluster_safety.rs    # Cluster safety invariants
└── integration/
    ├── deterministic.rs     # End-to-end deterministic tests
    └── chaos.rs             # Chaos engineering tests
```

**Core Testing Dependencies**:
```toml
[dev-dependencies]
madsim = "0.2"              # Deterministic simulation
turmoil = "0.6"             # Network simulation
proptest = "1.4"            # Property-based testing
fail = { version = "0.5", features = ["failpoints"] }
stateright = "0.30"         # Model checking
```

#### 1.3 Project Structure
```
src/
├── main.rs                 # CLI entry point
├── lib.rs                  # Library exports
├── cli/
│   ├── mod.rs
│   ├── args.rs             # CLI argument parsing
│   └── commands.rs         # Command dispatch
├── cluster/
│   ├── mod.rs
│   ├── node.rs             # Node management
│   ├── discovery.rs        # Tailscale discovery
│   ├── consensus.rs        # Raft consensus (deterministic)
│   ├── monitor.rs          # Health monitoring
│   └── simulation.rs       # Simulation-aware abstractions
├── storage/
│   ├── mod.rs
│   ├── store.rs            # Distributed store interface
│   └── local.rs            # Local storage backend
├── services/
│   ├── mod.rs
│   ├── manager.rs          # Service management logic
│   ├── systemd.rs          # systemd integration
│   └── types.rs            # Service data structures
├── network/
│   ├── mod.rs
│   ├── rpc.rs              # RPC client/server
│   └── protocol.rs         # Message types
└── error.rs                # Error types
```

### Phase 2: Core Infrastructure Migration (Test-Driven)

#### 2.1 Deterministic Abstractions (Foundation)
- **Time**: All time operations must go through `SimulatedClock` trait
- **Randomness**: All randomness through `DeterministicRng` trait  
- **I/O**: All file/network I/O through simulation-aware traits
- **Error Handling**: Comprehensive error types with `thiserror`

```rust
// Example deterministic abstractions
trait TimeProvider: Send + Sync {
    fn now(&self) -> Instant;
    fn sleep(&self, duration: Duration) -> impl Future<Output = ()>;
}

trait NetworkProvider: Send + Sync {
    fn send_message(&self, to: NodeId, msg: Message) -> Result<()>;
    fn receive_messages(&self) -> impl Stream<Item = Message>;
}
```

#### 2.2 RPC System (Simulation-First)
- Design gRPC/tarpc protocol that works in simulation
- Implement deterministic timeout/retry logic
- **Critical**: All network operations must be deterministic for testing
- Connection pooling with fault injection support

#### 2.3 Distributed Storage (Raft Consensus - Non-Negotiable)
- **Primary Goal**: Implement tikv/raft-rs with full deterministic testing
- **Safety Properties**: Must pass all Raft safety tests from day one
- Key-value store interface compatible with Khepri path operations
- **Testing Strategy**: 
  - Implement Jepsen-style linearizability checking
  - Network partition simulation
  - Leader election chaos testing
  - Data persistence verification

### Phase 3: Service Management Core

#### 3.1 systemd Integration
- Port systemd shell command execution to Rust
- Implement robust process management
- Support both user and system-level services

#### 3.2 Service State Management
- Migrate service data structures from Gleam
- Implement state persistence and replication
- Maintain compatibility with existing service definitions

### Phase 4: Clustering and Discovery

#### 4.1 Node Management
- Replace Erlang distribution with custom clustering
- Implement node discovery via Tailscale API
- Support primary/secondary cluster initialization

#### 4.2 Cluster Health Monitoring
- Port replication monitoring from Gleam
- Implement cluster consensus health checks
- Add automatic node recovery mechanisms

### Phase 5: CLI and User Interface

#### 5.1 Command Line Interface
- Port all existing CLI commands using `clap`
- Maintain backward compatibility with existing scripts
- Preserve all command-line flags and options
- **Testing**: CLI commands must work in deterministic simulation

#### 5.2 Configuration Management
- Environment variable handling
- Configuration file support (TOML/YAML)
- Migration from existing Gleam configuration

### Phase 6: Advanced Testing and Verification

#### 6.1 Property-Based Testing
- Service management invariants
- Cluster formation properties
- Data consistency properties
- Implement using `proptest` framework

#### 6.2 Chaos Engineering
- Continuous fault injection during development
- Network partition testing
- Node failure scenarios
- Clock drift simulation

#### 6.3 Model Checking
- Use `stateright` for formal verification
- Model Raft consensus algorithm
- Verify safety and liveness properties

### Phase 7: microvm.nix Integration

#### 7.1 Understanding the Architecture
**CRITICAL**: Blixard does NOT replace microvm.nix. We are building an orchestration layer that uses microvm.nix as its foundation for VM management.

#### 7.2 Integration Strategy
```rust
// Blixard orchestrates THROUGH microvm.nix, not around it
pub trait MicroVMProvider {
    // Interface to microvm.nix functionality
    async fn create_vm(&self, node: &str, config: &VMConfig) -> Result<VMId>;
    async fn start_vm(&self, node: &str, vm_id: &VMId) -> Result<()>;
    async fn stop_vm(&self, node: &str, vm_id: &VMId) -> Result<()>;
    async fn migrate_vm(&self, vm_id: &VMId, from: &str, to: &str) -> Result<()>;
}

// Our value-add is distributed coordination
pub struct BlixardOrchestrator {
    microvm: Box<dyn MicroVMProvider>,
    consensus: RaftConsensus,
    placement: PlacementEngine,
    storage: CephCoordinator,
}
```

#### 7.3 Implementation Options
1. **Rust FFI to microvm.nix** - Direct integration for performance
2. **CLI Wrapper** - Simple initial implementation
3. **Nix Daemon RPC** - Most "Nix-native" approach

## Technical Challenges and Solutions

### Challenge 1: Distributed Consensus Migration (HIGHEST PRIORITY)
**Problem**: Khepri provides battle-tested Raft consensus - cannot afford regression
**Solution**: 
- **Primary**: tikv/raft-rs (production-proven in TiKV database)
- **Deterministic testing from day 1**: Use madsim/turmoil for network simulation
- **Rigorous verification**: Implement Jepsen-style linearizability testing
**Non-Negotiable Requirements**:
- **Zero consensus safety violations** - system must never split-brain
- **Deterministic replay** - all bugs must be reproducible
- **Partition tolerance** - must handle network partitions correctly
- **Data durability** - committed data must never be lost
**Testing Strategy**:
- Implement all Raft consensus tests before any business logic
- Network partition simulation with deterministic outcomes
- Leader election chaos testing
- Data persistence verification across simulated crashes

### Challenge 2: Erlang Distribution Replacement
**Problem**: Losing OTP's battle-tested distribution primitives
**Solution**: 
- gRPC/tonic for reliable RPC with built-in retries
- Custom node discovery and health checking
- Connection pooling and load balancing
**Considerations**:
- Network partition handling
- Node failure detection
- Message ordering guarantees

### Challenge 3: Process Management Robustness
**Problem**: Ensuring reliable service lifecycle management
**Solution**:
- Use `tokio::process` for async process management
- Implement proper signal handling
- Add process supervision patterns
**Considerations**:
- Zombie process prevention
- Resource cleanup on failures
- Cross-platform compatibility

## Migration Timeline

### Week 1-2: Testing Foundation (CRITICAL FIRST STEP)
- **Week 1**: Set up deterministic simulation infrastructure
  - Implement madsim-based test harness
  - Create simulated time, network, and I/O abstractions
  - Set up property-based testing framework
- **Week 2**: Raft consensus testing framework
  - Implement basic Raft model checking
  - Create network partition simulation
  - Establish consensus safety property tests

### Week 3-4: Consensus Implementation (Test-Driven)
- **Week 3**: Implement tikv/raft-rs integration
  - All development guided by deterministic tests
  - Ensure perfect consensus safety from day one
- **Week 4**: Distributed storage with rigorous testing
  - Key-value store with Khepri-compatible interface
  - Data persistence and recovery testing
  - Linearizability verification

### Week 5-6: Service Management
- Port systemd integration to Rust
- Implement service state management
- Create service lifecycle operations

### Week 7-8: Clustering
- Implement node discovery with Tailscale
- Add cluster formation and management
- Port health monitoring functionality

### Week 9-10: Advanced Testing & Verification
- **Continuous verification**: 24/7 automated testing
- **Chaos engineering**: Production-like failure injection
- **Performance benchmarking**: vs Gleam version under faults
- **Bug minimization**: Automated reproduction of found issues
- **Documentation**: Testing methodologies and runbooks

## Compatibility and Migration Path

### Data Migration
- Export existing Khepri data to JSON/CBOR format
- Import data into new Rust-based storage
- Validate data integrity post-migration

### Configuration Migration
- Convert existing environment variables
- Update deployment scripts
- Maintain CLI command compatibility

### Deployment Strategy
- Blue-green deployment for cluster nodes
- Gradual rollout with fallback capability
- Comprehensive testing in staging environment

## Risk Assessment

### High Risk
- **Consensus safety violations**: Split-brain, data loss, inconsistency
  - **Mitigation**: Deterministic simulation testing from day 1
  - **Verification**: Jepsen-style linearizability checking
  - **Requirement**: Zero tolerance for consensus bugs
- **Network partition handling**: Different behavior from OTP
  - **Mitigation**: Comprehensive partition simulation testing
- **Data loss during migration**: Incomplete data transfer
  - **Mitigation**: Formal verification of migration procedures

### Medium Risk
- **Performance regression**: Rust implementation slower than Erlang
- **Integration issues**: Tailscale API changes
- **Testing coverage**: Missing edge cases from Gleam version

### Mitigation Strategies
- **Testing-First Development**: No feature without deterministic tests
- **Continuous Consensus Verification**: 24/7 Raft safety checking
- **Simulation-Driven Development**: All features developed in simulation first
- **Property-Based Testing**: Automated invariant checking
- **Chaos Engineering**: Continuous fault injection during development
- **Formal Verification**: Model checking for critical consensus paths
- **Phased rollout**: With comprehensive monitoring and instant rollback
- **Zero-downtime migration**: With formal verification of data transfer

## Success Criteria

### Functional Requirements
- [ ] All existing CLI commands work identically
- [ ] Service management operations maintain reliability  
- [ ] Cluster formation and discovery function correctly
- [ ] **Data consistency maintained across cluster nodes (CRITICAL)**
- [ ] **Raft consensus safety properties never violated**
- [ ] **System survives all simulated network partitions**
- [ ] **Zero data loss under any failure scenario**

### Performance Requirements
- [ ] CLI command response time ≤ current Gleam implementation
- [ ] Cluster consensus latency within 10% of Khepri
- [ ] Memory usage comparable or better than current system

### Reliability Requirements (Enhanced)
- [ ] **Zero consensus safety violations (split-brain, data loss, inconsistency)**
- [ ] **Perfect deterministic test reproducibility**
- [ ] **Survives 100% of simulated network partitions**
- [ ] **Automated recovery from all simulated node failures**
- [ ] **Linearizability maintained under all fault scenarios**
- [ ] **Zero data loss during migration (formally verified)**

## Post-Migration Benefits

### Performance Improvements
- Faster binary startup (no BEAM VM overhead)
- Lower memory footprint
- Better resource utilization

### Development Benefits
- Strong type system with compile-time guarantees
- Rich ecosystem of crates
- Better tooling and debugging support

### Operational Benefits
- Single binary deployment
- Easier cross-compilation
- Better integration with monitoring tools

## 🚀 MicroVM Platform Architecture Changes

### 🔧 CRITICAL: Blixard's Relationship with microvm.nix

**Blixard is an orchestration layer built ON TOP OF microvm.nix**, not a replacement. Here's the clear separation of concerns:

```
┌─────────────────────────────────────┐
│         Blixard CLI                 │  User Interface
├─────────────────────────────────────┤
│    Blixard Orchestration Layer      │  What WE build:
│  • Distributed Consensus (Raft)     │  • Cluster-wide VM state
│  • Intelligent Placement Engine     │  • Any-node scheduling
│  • Serverless Function Runtime      │  • Event-driven scaling
│  • Storage Coordination (Ceph)      │  • Multi-node coordination
├─────────────────────────────────────┤
│        microvm.nix                  │  What we USE:
│  • VM Creation/Management           │  • THE primary VM interface
│  • Hypervisor Abstraction          │  • QEMU/Firecracker/CH
│  • NixOS Module System              │  • Declarative configs
│  • Local VM Operations              │  • Single-node excellence
└─────────────────────────────────────┘
```

### Key Architectural Shifts

This migration transforms Blixard from a simple service manager into a full **dual-mode microVM orchestration platform** with both persistent and serverless execution:

#### 1. **From Services → Dual VM Architecture**
- **Current**: Manage systemd services on bare metal
- **Target**: Orchestrate both long-lived VMs AND serverless microVMs
- **Benefits**: 
  - **Long-lived VMs**: Stateful services, databases, caches
  - **Serverless VMs**: Event-driven functions, auto-scaling workloads
  - **Intelligent placement**: Automatic routing to optimal VM type

#### 2. **From Basic Storage → Ceph + Ephemeral Storage**
- **Current**: Local file storage with basic replication
- **Target**: 
  - **Persistent**: Distributed Ceph storage (RBD, CephFS, RADOS)
  - **Ephemeral**: RAM-backed tmpfs for serverless functions
- **Benefits**: Data durability for stateful, zero-latency for serverless

#### 3. **From Simple Config → Orchestration of microvm.nix**
- **Current**: Basic configuration files
- **Target**: 
  - **VM Layer**: microvm.nix handles ALL VM creation/management
  - **Orchestration Layer**: Blixard adds cluster-wide coordination
  - **Functions**: Serverless runtime on top of microvm.nix VMs
- **Benefits**: 
  - Leverage battle-tested microvm.nix for VM operations
  - Focus on distributed systems challenges
  - Maintain compatibility with existing microvm.nix deployments

#### 4. **microvm.nix as Foundation**
- **VM Interface**: All VMs created/managed through microvm.nix
- **Hypervisor Abstraction**: Support all microvm.nix hypervisors (qemu, firecracker, cloud-hypervisor, crosvm, kvmtool, stratovirt, alioth)
- **Blixard's Role**: 
  - Distributed consensus on which node runs which VM
  - Placement decisions and resource scheduling
  - Monitoring VMs created by microvm.nix
  - Orchestrating microvm.nix across multiple nodes
  - Adding serverless runtime on top

### New Platform Capabilities

#### Advanced VM Operations
```bash
# Live migration with zero downtime (long-lived VMs)
blixard vm migrate webapp-1 --target-node node2 --live

# Serverless function deployment
blixard function deploy api-handler --runtime rust --memory 128MB
blixard function invoke api-handler --data '{"action": "process"}'

# Any-node execution
blixard run --function data-processor --near-data /datasets/bigdata.csv

# Declarative VM management
blixard apply -f webapp-cluster.yaml

# Storage orchestration
blixard storage create-pool ssd-pool --replicas 3 --size 1TB
blixard volume create app-data --pool ssd-pool --size 50GB
```

#### Dual-Mode Features
- **Intelligent Workload Placement**: Automatic routing to serverless or persistent VMs
- **Sub-10ms Cold Starts**: Pre-warmed microVM pools with Firecracker
- **Any-Node Execution**: Functions run where data lives
- **Event-Driven Scaling**: 0-to-N scaling in milliseconds
- **Hybrid Workloads**: Mix persistent services with serverless functions

#### Enterprise Features
- **Multi-tenant isolation** through VMs and VLANs
- **Resource scheduling** with affinity/anti-affinity rules
- **Disaster recovery** with cross-region Ceph replication
- **Compliance** with immutable VM configurations
- **Cost Optimization** through aggressive serverless scaling

### System Dependencies

The migration requires these additional system components:

```bash
# Hypervisor stack
sudo apt install qemu-kvm libvirt-daemon-system

# Firecracker for serverless VMs
wget https://github.com/firecracker-microvm/firecracker/releases/download/v1.0.0/firecracker-v1.0.0-x86_64
sudo mv firecracker-v1.0.0-x86_64 /usr/local/bin/firecracker
sudo chmod +x /usr/local/bin/firecracker

# Ceph storage
sudo apt install ceph-common librados-dev libcephfs-dev

# Network virtualization
sudo apt install bridge-utils vlan openvswitch-switch

# NixOS support (optional but recommended)
curl -L https://nixos.org/nix/install | sh
```

### Serverless-Specific Architecture

#### VM Pool Management
- **Pre-warmed Pools**: Maintain ready microVMs for instant starts
- **Memory Tiering**: 128MB, 256MB, 512MB, 1GB pre-configured VMs
- **Network Pooling**: Pre-allocated TAP devices for zero-config networking
- **Storage Templates**: Copy-on-write base images for fast provisioning

#### Function Runtime Support
```rust
// Serverless function interface
pub trait ServerlessFunction {
    async fn handle(&self, event: Event) -> Result<Response>;
    fn memory_limit(&self) -> MemorySize;
    fn timeout(&self) -> Duration;
}

// Runtime environments
pub enum Runtime {
    Rust,      // Native Rust functions
    Wasm,      // WebAssembly for polyglot support
    Container, // OCI containers in microVMs
}
```

## Serverless Migration Considerations

### Phase 7: Serverless Platform Components

#### 7.1 microvm.nix Integration for Serverless
- **Template Management**: Pre-built microvm.nix templates for fast boot
- **Pool Orchestration**: Coordinate microvm.nix to maintain warm VM pools
- **Hypervisor Selection**: Tell microvm.nix to use Firecracker for serverless
- **Resource Allocation**: Parse microvm.nix configs to track resources

#### 7.2 Function Runtime Engine
```rust
// Core serverless components to implement
pub mod serverless {
    pub struct FunctionManager {
        vm_pool: VmPool,
        scheduler: Scheduler,
        metrics: MetricsCollector,
    }
    
    pub struct VmPool {
        warm_vms: HashMap<MemorySize, Vec<MicroVM>>,
        network_pool: NetworkDevicePool,
        storage_templates: StorageTemplateCache,
    }
}
```

#### 7.3 Event-Driven Architecture
- **Event Router**: Route events to appropriate functions
- **Queue Manager**: Handle async invocations and retries
- **Trigger System**: HTTP, timers, storage events, custom triggers
- **Result Store**: Temporary storage for function outputs

### Testing Serverless Components

#### Deterministic Serverless Testing
```rust
#[cfg(test)]
mod serverless_tests {
    use super::*;
    
    #[test]
    fn test_cold_start_performance() {
        let sim = ServerlessSimulator::new();
        sim.set_vm_boot_time(Duration::from_millis(5));
        
        let start = sim.now();
        sim.invoke_function("test-func", Event::Http(req));
        let latency = sim.now() - start;
        
        assert!(latency < Duration::from_millis(10));
    }
    
    #[test] 
    fn test_concurrent_scaling() {
        let sim = ServerlessSimulator::new();
        
        // Simulate 1000 concurrent requests
        for i in 0..1000 {
            sim.spawn_request(i);
        }
        
        sim.run_until_idle();
        assert_eq!(sim.successful_invocations(), 1000);
    }
}
```

## Documentation Updates Required

- Update CLAUDE.md with VM management commands and patterns
- Create microvm.nix configuration guide
- Add Ceph storage administration documentation
- Update deployment guide for VM platform
- Create network virtualization setup guide
- Add live migration operational procedures
- **Create serverless function development guide**
- **Document function deployment workflows**
- **Add performance tuning guide for cold starts**
- **Create any-node execution patterns documentation**

## Appendix: Alternative Approaches Considered

### Approach 1: Keep Erlang, Rewrite in Elixir
**Pros**: Maintain OTP benefits, simpler migration
**Cons**: Still tied to BEAM ecosystem, team Rust preference

### Approach 2: Hybrid Approach (Rust CLI + Erlang Cluster)
**Pros**: Gradual migration, lower risk
**Cons**: Increased complexity, two runtimes to maintain

### Approach 3: Complete Rewrite from Scratch
**Pros**: Clean slate, optimal Rust design
**Cons**: High risk, longer timeline, feature regression risk

**Selected Approach**: Direct migration with **architectural expansion** to microVM orchestration for maximum long-term value.

---

## 🎯 Migration Summary

This migration plan transforms Blixard from a basic service manager into a **production-grade microVM orchestration platform** that competes with container orchestrators while providing superior security isolation through hardware virtualization.

### Success Metrics
- **Zero consensus safety violations** during the entire migration
- **Deterministic test coverage** for all VM and storage operations  
- **Live migration capability** with <1 second downtime for persistent VMs
- **Serverless cold starts** under 10ms with pre-warmed pools
- **Function density** of 1000+ concurrent functions per host
- **Any-node execution** with data locality optimization
- **Ceph integration** with automatic replication and disaster recovery
- **Performance parity** with AWS Lambda for serverless workloads
- **VM density** exceeding container orchestrators through Firecracker

### Platform Vision
**Blixard will become the "Kubernetes for MicroVMs"** - providing enterprise-grade orchestration for virtual machines with a unique **dual-mode architecture**, all built on top of the proven microvm.nix foundation:

1. **Long-lived VMs**: For stateful services, databases, and persistent workloads with full Ceph storage integration
2. **Serverless VMs**: For event-driven functions, auto-scaling APIs, and ephemeral workloads with sub-10ms cold starts

**Key Architecture**: Blixard orchestrates through microvm.nix rather than reimplementing VM management. microvm.nix handles the actual VM lifecycle and hypervisor interactions, while Blixard adds the distributed orchestration layer - consensus, intelligent placement, serverless runtime, and cluster-wide management.

This hybrid approach delivers the best of both worlds: the security and resource isolation of VMs with the agility and cost-efficiency of serverless, all while maintaining the ease of use and automation that Kubernetes brings to containers.