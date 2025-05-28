# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands and Usage

This repo is in active development with evolving commands. For current commands:
- Check `src/service_manager.gleam` main() function for CLI argument parsing
- Check `flake.nix` shellHook for development helper functions (blixard-*)
- Check `README.md` for usage examples
- Check `scripts/` directory for deployment and operational scripts

## Core Architecture

Blixard is a distributed service management system built on Gleam/Erlang with Khepri distributed storage.

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