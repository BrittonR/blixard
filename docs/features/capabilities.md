# Blixard Primary Capabilities

## 1. Distributed MicroVM Orchestration

### Dual VM Models

Blixard supports two distinct VM operational models:

- **Long-lived VMs**: Traditional services, databases, stateful applications with persistent lifecycle
- **Serverless VMs**: Event-driven functions with <100ms cold start, auto-scaling to zero

### Intelligent Scheduling

The scheduling system adapts to the VM type:

**Long-lived VMs**: Placement based on resource requirements and affinity rules

**Serverless VMs**: Dynamic placement on any available node based on:
- Current resource availability (CPU, memory, network)
- Data locality (run near the data being processed)
- Network proximity (minimize latency to event sources)
- Node specialization (GPU nodes for ML functions, etc.)

### Core Orchestration Features

- **VM Lifecycle Management**: Orchestrate microvm.nix VMs across cluster nodes
- **Declarative Configuration**: VMs defined through microvm.nix, orchestrated by Blixard
- **Real-time Monitoring**: VM health, resource usage, and performance metrics
- **Live Migration**: Move long-lived VMs between nodes with zero downtime
- **Resource Management**: CPU, memory, disk, and network allocation with overcommit for serverless
- **Security Isolation**: Strong boundaries through hardware virtualization (via microvm.nix)
- **Multi-Hypervisor Support**: All microvm.nix hypervisors - Firecracker (serverless), Cloud Hypervisor, QEMU, crosvm, kvmtool, stratovirt, alioth

## 2. Distributed Storage with Ceph

Blixard integrates deeply with Ceph to provide enterprise-grade distributed storage:

- **Block Storage (RBD)**: Persistent volumes for VM disks with replication
- **Object Storage (RADOS)**: Application data with versioning and lifecycle management
- **File Storage (CephFS)**: Shared filesystems across VMs
- **Storage Orchestration**: Automatic pool creation, placement groups, and rebalancing
- **Data Durability**: Configurable replication and erasure coding
- **Performance Tiers**: SSD/NVMe for hot data, HDD for cold storage
- **Snapshot Management**: Point-in-time snapshots and clones

## 3. High Availability and Fault Tolerance

The system is designed to handle failures gracefully:

- **VM Automatic Failover**: VMs automatically restart on healthy nodes
- **Storage Redundancy**: Ceph replication protects against disk/node failures
- **Split-Brain Prevention**: Raft consensus for VM state, Ceph quorum for storage
- **Network Partition Tolerance**: VMs continue running, storage maintains consistency
- **Live Migration**: Move VMs away from failing hardware
- **Disaster Recovery**: Cross-region Ceph replication and VM image backup
- **Rolling Updates**: Update hypervisors without VM downtime

## 4. Developer Experience

Blixard prioritizes developer productivity with:

### Declarative VM Definitions
Use microvm.nix configurations for both long-lived and serverless VMs

### Function Development
Simple Nix expressions for serverless functions:
```nix
# serverless-function.nix
{ event, context }: { 
  statusCode = 200;
  body = "Hello from NixOS serverless! Event: ${event.type}";
}
```

### Additional Developer Features
- **CLI Interface**: Kubectl-like commands + Lambda-like function management
- **GitOps Workflow**: Infrastructure as Code with automatic deployment
- **Rich Status Information**: VM metrics, function invocations, cold/warm starts
- **Local Testing**: Test serverless functions locally before deployment
- **Development Environments**: Instant dev VMs with shared storage
- **Function Triggers**: HTTP endpoints, event streams, scheduled invocations