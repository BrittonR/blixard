# Blixard Architecture Overview

## Project Vision

Blixard is a production-grade **distributed microVM orchestration platform** that provides **reliable, consistent, and fault-tolerant** orchestration of both **long-lived microVMs** and **serverless microVMs** across multiple **NixOS** machines in a cluster. Built with Rust, powered by Raft consensus, and **orchestrating through microvm.nix** as the foundational VM management layer with **Ceph** for distributed storage, it ensures that VM state and storage remain consistent even during network partitions, node failures, and other distributed system challenges.

**Architecture**: Blixard builds on top of microvm.nix, which handles the actual VM creation and lifecycle management through various hypervisors (qemu, firecracker, cloud-hypervisor, etc.). Blixard adds the distributed orchestration layer - consensus, intelligent placement, serverless runtime, and cluster-wide management.

**Think "Kubernetes + AWS Lambda for MicroVMs on NixOS"** - providing enterprise-grade orchestration for:
- **Long-lived VMs**: Traditional services, databases, stateful applications
- **Serverless VMs**: Sub-second startup, event-driven functions, auto-scaling to zero

**🎯 NixOS-First Design**: Blixard is designed exclusively for NixOS environments, leveraging the Nix package manager, NixOS modules, and microvm.nix for a truly declarative and reproducible virtualization platform.

**⚡ Serverless Innovation**: Blixard brings AWS Lambda-like serverless computing to on-premise NixOS clusters, with intelligent scheduling that runs functions on any available node based on resource availability, data locality, and network proximity.

## System Architecture Overview

### Core Principles
- **NixOS-Native**: Built exclusively for NixOS with deep integration into the Nix ecosystem
- **Consensus-First Design**: Every VM state change goes through Raft consensus
- **MicroVM Isolation**: Strong security boundaries through lightweight virtualization
- **Distributed Storage**: Ceph-backed persistent storage with replication and fault tolerance
- **Declarative Everything**: microvm.nix handles VM definitions, Blixard orchestrates them across the cluster
- **Immutable Infrastructure**: Leverage Nix's immutability for VM configurations and deployments
- **Fault Tolerance**: Survives network partitions, node failures, and storage failures
- **Deterministic Testing**: All functionality verified through simulation testing
- **Zero Downtime**: VM management operations never require cluster downtime
- **Strong Consistency**: All nodes always agree on VM and storage state

### High-Level Architecture
```
┌─────────────────────────────────┐    ┌─────────────────────────────────┐    ┌─────────────────────────────────┐
│         Cluster Node            │    │         Cluster Node            │    │         Cluster Node            │
│                                 │    │                                 │    │                                 │
│  ┌───────────┐  ┌───────────┐   │    │  ┌───────────┐  ┌───────────┐   │    │  ┌───────────┐  ┌───────────┐   │
│  │    CLI    │  │  Blixard  │   │    │  │    CLI    │  │  Blixard  │   │    │  │    CLI    │  │  Blixard  │   │
│  │ Interface │  │Orchestrator│   │    │  │ Interface │  │Orchestrator│   │    │  │ Interface │  │Orchestrator│   │
│  └───────────┘  └─────┬─────┘   │    │  └───────────┘  └─────┬─────┘   │    │  └───────────┘  └─────┬─────┘   │
│                       │          │    │                       │          │    │                       │          │
│  ┌────────────────────▼────────┐ │    │  ┌────────────────────▼────────┐ │    │  ┌────────────────────▼────────┐ │
│  │       microvm.nix Layer     │ │    │  │       microvm.nix Layer     │ │    │  │       microvm.nix Layer     │ │
│  │  (VM Creation & Lifecycle)  │ │    │  │  (VM Creation & Lifecycle)  │ │    │  │  (VM Creation & Lifecycle)  │ │
│  └─────────────────────────────┘ │    │  └─────────────────────────────┘ │    │  └─────────────────────────────┘ │
│  ┌─────────────────────────────┐ │    │  ┌─────────────────────────────┐ │    │  ┌─────────────────────────────┐ │
│  │         Raft Consensus      │◄┼────┼─►│         Raft Consensus      │◄┼────┼─►│         Raft Consensus      │ │
│  │    (VM State + Storage)     │ │    │  │    (VM State + Storage)     │ │    │  │    (VM State + Storage)     │ │
│  └─────────────────────────────┘ │    │  └─────────────────────────────┘ │    │  └─────────────────────────────┘ │
│  ┌─────────────────────────────┐ │    │  ┌─────────────────────────────┐ │    │  ┌─────────────────────────────┐ │
│  │        Ceph Storage         │ │    │  │        Ceph Storage         │ │    │  │        Ceph Storage         │ │
│  │   (OSD + Monitor + MDS)     │◄┼────┼─►│   (OSD + Monitor + MDS)     │◄┼────┼─►│   (OSD + Monitor + MDS)     │ │
│  └─────────────────────────────┘ │    │  └─────────────────────────────┘ │    │  └─────────────────────────────┘ │
│  ┌─────────────────────────────┐ │    │  ┌─────────────────────────────┐ │    │  ┌─────────────────────────────┐ │
│  │    Hypervisor (KVM)         │ │    │  │    Hypervisor (KVM)         │ │    │  │    Hypervisor (KVM)         │ │
│  │  ┌─────┐ ┌─────┐ ┌─────┐    │ │    │  │  ┌─────┐ ┌─────┐ ┌─────┐    │ │    │  │  ┌─────┐ ┌─────┐ ┌─────┐    │ │
│  │  │ VM1 │ │ VM2 │ │ VM3 │    │ │    │  │  │ VM4 │ │ VM5 │ │ VM6 │    │ │    │  │  │ VM7 │ │ VM8 │ │ VM9 │    │ │
│  │  └─────┘ └─────┘ └─────┘    │ │    │  │  └─────┘ └─────┘ └─────┘    │ │    │  │  └─────┘ └─────┘ └─────┘    │ │
│  └─────────────────────────────┘ │    │  └─────────────────────────────┘ │    │  └─────────────────────────────┘ │
└─────────────────────────────────┘    └─────────────────────────────────┘    └─────────────────────────────────┘
                 │                                       │                                       │
                 └───────────────────────────────────────┼───────────────────────────────────────┘
                                                         │
                            ┌─────────────────────────────────────────┐
                            │           Network Layer                │
                            │  • Service Discovery (Tailscale)      │
                            │  • Virtual Networking (VXLANs)        │
                            │  • Load Balancing & Ingress           │
                            └─────────────────────────────────────────┘
```

## What Blixard Will Do Once Complete

Once fully operational, Blixard will provide a comprehensive distributed microVM orchestration platform that combines the best of traditional VM management with modern serverless computing patterns. The system will handle everything from long-lived database clusters to ephemeral function invocations, all while maintaining strong consistency and fault tolerance across the cluster.

The platform leverages NixOS's declarative configuration model and microvm.nix's lightweight virtualization to create a truly cloud-native experience on bare metal infrastructure. With sub-100ms cold starts for serverless functions and enterprise-grade features like live migration and distributed storage, Blixard brings public cloud capabilities to on-premise NixOS deployments.