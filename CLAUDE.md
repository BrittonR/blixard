# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Quick Start Commands

### Development
```bash
# Build and test
gleam build                 # Build the project
gleam test                  # Run tests

# Development helpers (from flake.nix)
blixard-build              # Build wrapper
blixard-test               # Test wrapper

# Scripts for testing and deployment
sh scripts/manual_test.sh          # Manual testing
sh scripts/test_cluster_with_service.sh  # Cluster testing
sh scripts/deploy.sh              # Deployment
```

### CLI Usage
```bash
# Service management
blixard start <service>     # Start a service
blixard stop <service>      # Stop a service
blixard list               # List all services
blixard status <service>   # Check service status

# Cluster operations
blixard --init-primary     # Initialize primary cluster node
blixard --init-secondary   # Initialize secondary cluster node
blixard --join-cluster     # Join existing cluster

# Key options: --user (user-level services), --verbose
```

## Commands and Usage

This repo is in active development with evolving commands. For current commands:
- Check `src/service_manager.gleam` main() function for CLI argument parsing
- Check `flake.nix` shellHook for development helper functions (blixard-*)
- Check `README.md` for usage examples
- Check `scripts/` directory for deployment and operational scripts

## Core Architecture

Blixard is a distributed service management system built on Gleam/Erlang with Khepri distributed storage.

### Migration Planning

The project is planning a migration from Gleam to Rust with **critical emphasis on retaining Raft consensus safety**. See:
- `docs/GLEAM_TO_RUST_MIGRATION.md` - Comprehensive migration plan with testing-first approach
- `docs/RUST_GUIDELINES.md` - Rust coding standards and patterns
- `docs/advanced_testing_methodologies.md` - TigerBeetle/FoundationDB-style testing approaches
- `docs/advanced_testing_implementation.md` - Deterministic simulation testing implementation
- `docs/DIOXUS_PATTERNS.md` - UI framework patterns (for future web interface)
- `docs/STATE_MANAGEMENT.md` - State management approaches for Rust

**Key Migration Principles**:
- **Consensus Safety First**: Zero tolerance for Raft consensus violations
- **Deterministic Testing**: All features developed with simulation testing from day 1
- **Testing-Driven Development**: Implement testing infrastructure before any business logic

### Key Components

1. **service_manager.gleam** - Main CLI entry point, parses args and dispatches commands
2. **node_manager.gleam** - Manages Erlang distribution and cluster formation
3. **khepri_store.gleam** - Interface to Khepri distributed key-value store  
4. **service_handlers.gleam** - Business logic for service management operations
5. **systemd.gleam** - Low-level systemd interaction via shell commands
6. **cluster_discovery.gleam** - Node discovery using Tailscale networking
7. **replication_monitor.gleam** - Monitors cluster health and replication

### Distributed System Design

- **Consensus**: Uses Khepri (Raft-based) for distributed state consensus
- **Node Discovery**: Tailscale provides secure networking between cluster nodes
- **Service Control**: Manages systemd services (both user and system level)
- **CLI Architecture**: Ephemeral CLI nodes connect to persistent cluster nodes

### Data Flow

1. CLI command → service_manager.gleam argument parsing
2. Ensure Erlang distribution active (node_manager.gleam)  
3. Dispatch to service handler (service_handlers.gleam)
4. Execute systemd operation (systemd.gleam)
5. Store/update state in Khepri (khepri_store.gleam)
6. State automatically replicates across cluster

### Configuration

Environment variables (set in flake.nix):
- `BLIXARD_STORAGE_MODE` - Storage backend selection
- `BLIXARD_KHEPRI_OPS` - Which operations use distributed storage

### Node Types

- **Cluster Nodes**: Persistent nodes running Khepri cluster (--join-cluster, --init-primary/secondary)
- **CLI Nodes**: Ephemeral nodes for individual commands that connect to cluster
- **User vs System**: Services can be managed at user level (--user flag) or system level

### Development Notes

- Uses `khepri_gleam` binding to Erlang Khepri library
- Erlang distribution handled by `distribution_helper.erl` 
- Tests use gleeunit framework
- Deployment creates symlinks in /usr/local/bin for system-wide access

### Khepri Storage Patterns

#### Path Format
- Khepri paths are lists of binaries without colons: `["services", "service-name"]`
- In Gleam code, paths are written with colons: `"/:services/service-name"` 
- The `khepri_gleam:to_khepri_path()` function strips the colons and splits the path

#### RPC Calls to Khepri
When making RPC calls to remote Khepri nodes:
1. **Path encoding**: Pass paths as lists of binaries: `[<<"services">>]` not `[":services"]`
2. **Result decoding**: Erlang `{ok, Value}` tuples appear as Gleam Result types in dynamic values
3. **Tuple access**: Use `decode.at([1], decoder)` to extract the value from an `{ok, Value}` tuple

#### Common Issues and Solutions
1. **"Failed to decode RPC result"**: Check if the path format is correct (no colons in binaries)
2. **Empty results when services exist**: Verify the RPC result structure - it may be wrapped differently than expected
3. **Service state shows "Unknown"**: Ensure the service info decoder handles the 3-tuple format: `(state, node, timestamp)`

#### Testing RPC Decoding
When debugging RPC issues:
1. Print the raw result: `io.println("Raw RPC result: " <> string.inspect(result))`
2. Check the dynamic type: `io.println("RPC result type: " <> string.inspect(dynamic.classify(result)))`
3. Try multiple decode strategies if the first fails

#### Service Management Gotchas

1. **Stop vs Remove**: 
   - `stop` marks a service as "Stopped" but keeps it in Blixard's management
   - `remove` deletes the service from Khepri entirely
   - Services marked as "Stopped" will still appear in `list` output

2. **Node Name Recording**:
   - CLI nodes have ephemeral names like `service_cli_<timestamp>@localhost`
   - When storing service info, record the cluster node's name, not the CLI node
   - This ensures services can be found by subsequent CLI commands

3. **Service Not Found Issues**:
   - If stop shows "not managed by Blixard", check:
     - RPC error handling in `get_service_info` 
     - Node name consistency in stored data
     - Proper decoding of Erlang error tuples like `{error, not_found}`

4. **RPC Timeouts**:
   - Add timeout protection to RPC calls (5 seconds default)
   - Use `rpc_helper.erl` for consistent timeout handling
   - Handle `{badrpc, timeout}` responses gracefully

## AI Development Guidelines

### Core Rules
1. Keep this file and referenced docs updated
2. Use AIDEV-* comment prefixes for context:
   - `AIDEV-NOTE`: Important implementation details
   - `AIDEV-TODO`: Future work items
   - `AIDEV-QUESTION`: Clarification needed
3. Never change API contracts without approval
4. Follow existing patterns over cleverness
5. Create structured, actionable errors
6. Profile before optimizing

### What AI Must NEVER Do
- Change API contracts without explicit approval
- Alter database migrations or Khepri schema
- Commit secrets or API keys
- Remove AIDEV-* comments
- Assume business logic - always ask for clarification

### Error Handling Example
```gleam
ServiceError {
  code: "SVC_001",
  message: "Service not found in cluster",
  service: "nginx",
  node: "cluster-node-1",
  timestamp: "2025-01-01T12:00:00Z",
}
```

## Testing

### Framework
- **gleeunit**: Gleam's unit testing framework
- Run tests with: `gleam test`
- Test files: `test/*.gleam`

### Testing Patterns
- Unit tests covering all major systems
- Cluster test harness for distributed operations
- Manual testing scripts for integration scenarios
- Test both user and system service management

### Resources
- `test/blixard_test.gleam`: Core functionality tests
- `test/cluster_test_harness.gleam`: Distributed testing utilities
- `scripts/test_*.sh`: Integration test scripts

## Performance

### Optimization Tips
- Enable Khepri caching for repeated operations
- Use `--verbose` flag to identify bottlenecks
- Monitor RPC call latency in distributed setups
- Profile Erlang distribution overhead

### Monitoring
- Use `replication_monitor.gleam` for cluster health
- Track service state consistency across nodes
- Monitor Tailscale network latency between nodes

## Technology Stack

- **Gleam + Erlang/OTP**: Core framework and runtime
- **Khepri**: Raft-based distributed storage
- **Tailscale**: Secure networking for cluster discovery
- **systemd**: Service lifecycle management
- **gleeunit**: Testing framework
- **Nix**: Reproducible builds and development environment