# Blixard Testing Guide

This document describes the various testing and diagnostic tools available for Blixard.

## Quick Start

After entering the Nix shell (`nix develop`), you have several testing commands available:

### Basic Commands

```bash
# Quick diagnostics - shows current state
blixard-diag

# Run automated test sequence
blixard-test-manual

# Launch interactive Zellij testing environment
blixard-test-zellij

# Clean up all processes and logs
blixard-clean

# Show recent logs from all nodes
blixard-logs
```

## Testing Tools

### 1. Diagnostic Script (`blixard-diag`)

Quick health check that shows:
- Running BEAM processes
- Registered Erlang nodes
- Test service status
- Currently managed services
- Recent log entries

### 2. Manual Test Script (`blixard-test-manual`)

Automated test sequence that:
1. Cleans up existing processes
2. Starts 2 cluster nodes
3. Tests service management (start/list/status/stop)
4. Verifies HTTP endpoint
5. Cleans up

Output is color-coded and shows each step clearly.

### 3. Zellij Test Environment (`blixard-test-zellij`)

Interactive multi-pane environment with:

**Tab 1: Cluster** - Two panes showing live cluster nodes
**Tab 2: Services** - Command pane + HTTP test + logs monitoring  
**Tab 3: Monitor** - Live process, service, and network monitoring
**Tab 4: Debug** - REPL and debug commands

Navigation:
- `Ctrl+p` then arrow keys to switch panes
- `Ctrl+t` then number to switch tabs
- `Ctrl+q` to quit

### 4. Test Scripts

Individual test scripts in `scripts/`:
- `test_cluster_with_service.sh` - Full integration test
- `manual_test.sh` - Step-by-step manual testing
- `blixard_diag.sh` - Quick diagnostics

## Common Test Scenarios

### Test Service Management
```bash
# Start nodes
gleam run -m service_manager -- --join-cluster &
sleep 5

# Manage service
gleam run -m service_manager -- start --user test-http-server
gleam run -m service_manager -- list
gleam run -m service_manager -- status --user test-http-server
gleam run -m service_manager -- stop --user test-http-server
```

### Test Cluster Formation
```bash
# Terminal 1
gleam run -m service_manager -- --join-cluster

# Terminal 2 
gleam run -m service_manager -- --join-cluster

# Terminal 3
gleam run -m service_manager -- list-cluster
```

### Debug RPC Issues
```bash
# Check what's in Khepri
gleam run -m service_manager -- list

# Look for decode errors in output
# Check raw RPC results in logs
```

## Troubleshooting

### No services showing up
1. Check nodes are connected: `epmd -names`
2. Verify Khepri is running: `blixard-diag`
3. Check for decode errors in list output

### Service state shows "Unknown"
- Service info might not be properly stored
- Check the service was started through Blixard

### Cluster won't form
1. Kill all processes: `killall beam.smp`
2. Remove Khepri data: `rm -rf khepri#*`
3. Start fresh

### Port 8888 already in use
```bash
# Find what's using it
lsof -i :8888

# Kill the process or use a different port
```

## Log Locations

- Cluster node logs: `/tmp/blixard_node*.log`
- Service logs: `journalctl --user -u <service-name>`
- Khepri data: `./khepri#<node-name>/`