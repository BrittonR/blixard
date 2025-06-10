# Blixard Documentation

This directory contains comprehensive documentation for Blixard, a distributed microVM orchestration platform for NixOS clusters.

## 📚 Documentation Structure

### Architecture
- [**Project Overview**](./architecture/overview.md) - Vision, principles, and high-level architecture
- [**Technical Architecture**](./architecture/technical-architecture.md) - Core components and microvm.nix integration
- [**Advanced Features**](./architecture/advanced-features.md) - Scheduling, migration, monitoring, and backup

### Features & Examples
- [**Capabilities**](./features/capabilities.md) - Primary features and capabilities
- [**Usage Scenarios**](./examples/usage-scenarios.md) - Real-world examples and use cases

### Operations
- [**Installation Guide**](./operations/installation.md) - Setup instructions for single-node and clusters
- [**Configuration**](./operations/configuration.md) - Configuration management and operational procedures
- [**Monitoring & Security**](./operations/monitoring.md) - Observability, logging, and security

### Development
- [**Testing Strategy**](./testing/testing-strategy.md) - Comprehensive testing approach and QA
- [**Migration Guide**](./migration/migration-guide.md) - Migrating from Gleam to Rust version

### Planning
- [**Roadmap**](./roadmap.md) - Development phases and future enhancements

## 🚀 Quick Start

1. Read the [Project Overview](./architecture/overview.md) to understand Blixard's vision
2. Check [Installation Guide](./operations/installation.md) for setup instructions
3. Explore [Usage Scenarios](./examples/usage-scenarios.md) for practical examples
4. Review [Technical Architecture](./architecture/technical-architecture.md) for implementation details

## 🎯 Key Concepts

Blixard is designed to provide:
- **NixOS-native** distributed microVM orchestration
- **Dual VM models**: Long-lived VMs and serverless functions
- **Raft consensus** for strong consistency
- **Ceph integration** for distributed storage
- **Deterministic testing** for reliability

For detailed information on any topic, navigate to the respective documentation file above.