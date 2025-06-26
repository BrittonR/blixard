# TUI Development Guide

This guide covers the comprehensive TUI development environment for Blixard, including testing, debugging, and hot-reload workflows.

## Quick Start

### 1. Start Full Development Environment
```bash
./scripts/dev-workflow.sh start
```

This starts:
- Development cluster on port 9000
- TUI with test data
- Hot-reload file watcher
- Automatic TUI restart on code changes

### 2. TUI-Only Development
```bash
# Start cluster and test data
./scripts/dev-workflow.sh start-cluster

# In another terminal, start hot-reload
./scripts/dev-workflow.sh hot-reload

# Manually start TUI when needed
cargo run -- tui
```

### 3. Testing Mode
```bash
# Run all TUI tests
./scripts/dev-workflow.sh run-tests

# Start test watcher for continuous testing
./scripts/dev-workflow.sh test-watch
```

## TUI Architecture

### Core Components

1. **App State** (`blixard/src/tui/app_v2.rs`)
   - Main application state management
   - Event handling and key bindings
   - Debug mode integration
   - Cluster data management

2. **UI Rendering** (`blixard/src/tui/ui_v2.rs`)
   - Tab-based interface rendering
   - Debug mode visualization
   - Raft state display
   - Real-time metrics

3. **Event System** (`blixard/src/tui/events.rs`)
   - Keyboard and mouse event handling
   - Tick-based updates
   - Custom event types

4. **VM Client** (`blixard/src/tui/vm_client.rs`)
   - gRPC client for cluster communication
   - Async operation handling

### Debug Mode Features

The TUI includes an advanced debug mode accessible via:
- Press `6` to switch to Debug tab
- Press `d` to toggle debug mode
- Press `Shift+D` for enhanced debug mode

#### Debug Views

1. **Raft Debug** (`r` key)
   - Current term, voted for, log length
   - Commit index and last applied
   - Peer state visualization
   - Election timeout status
   - Snapshot metadata

2. **Debug Metrics** (`m` key)
   - Message statistics (sent/received)
   - Proposal success rates
   - Election and leadership change counts
   - Storage operations (snapshots, compactions)
   - Network partition detection

3. **Debug Logs** (`l` key)
   - Real-time debug log entries
   - Color-coded by severity level
   - Component-specific filtering
   - Timestamp information

#### Debug Controls

- `s` - Simulate Raft debug data
- `R` - Reset debug metrics
- `c` - Clear debug logs
- `Esc` - Exit debug mode

## Testing Framework

### 1. TUI Integration Tests

Located in `tests/tui_integration_tests.rs`:

```bash
# Run TUI integration tests
cargo test --test tui_integration_tests

# Run specific test
cargo test --test tui_integration_tests test_tui_tab_navigation
```

#### Test Categories

- **Navigation Tests**: Tab switching, vim mode, keyboard shortcuts
- **Search and Filtering**: VM/node filtering, search mode
- **Form Handling**: VM creation, node management forms
- **Cluster Operations**: Discovery, connection, status
- **Debug Mode**: Debug visualization, state simulation
- **Error Handling**: Connection failures, graceful degradation

### 2. Real Cluster Testing

```bash
# Full integration test with real cluster
./scripts/test-tui-integration.sh

# TUI tests only (no cluster setup)
./scripts/test-tui-integration.sh --tui-only

# Keep cluster running for manual testing
./scripts/test-tui-integration.sh --keep-cluster
```

### 3. Test Helpers

The `TuiTestHarness` provides:
- Simulated key events
- State verification
- Mock cluster data
- Async event handling

```rust
let mut harness = TuiTestHarness::new().await?;

// Send key events
harness.send_char('2').await?; // Switch to VMs tab
harness.send_string("test-vm").await?; // Type string

// Verify state
assert_eq!(harness.app().current_tab, AppTab::VirtualMachines);

// Simulate cluster state
let mut harness = simulate_cluster_with_vms().await?;
```

## Hot-Reload Development

### File Watching

The development environment watches these files for changes:
- `blixard/src/tui/**/*.rs` - TUI source code
- `blixard/src/main.rs` - CLI entry point
- `blixard/Cargo.toml` - Dependencies

### Automatic Actions

When files change:
1. Rebuild project
2. Stop existing TUI
3. Start new TUI with preserved cluster state
4. Maintain test data and connections

### Manual Controls

```bash
# Check status
./scripts/dev-workflow.sh status

# Restart TUI manually
./scripts/dev-workflow.sh restart-tui

# View logs
./scripts/dev-workflow.sh logs

# Stop everything
./scripts/dev-workflow.sh stop
```

## VS Code Integration

The `.vscode/tasks.json` provides tasks for:
- `TUI: Start Development Environment`
- `TUI: Hot Reload Only`
- `TUI: Run Tests`
- `TUI: Watch Tests`
- `TUI: Show Status/Logs`

Access via `Ctrl+Shift+P` → "Tasks: Run Task"

## Development Workflows

### 1. Feature Development

```bash
# Start development environment
./scripts/dev-workflow.sh start

# Edit TUI code - auto-restart on save
# Test in real-time with live cluster
# View debug information in TUI

# Run tests
./scripts/dev-workflow.sh run-tests
```

### 2. Debug Investigation

```bash
# Start with existing cluster
./scripts/dev-workflow.sh start-cluster

# Start TUI
cargo run -- tui

# In TUI:
# - Press '6' for Debug tab
# - Press 's' to simulate debug data
# - Press 'r' for Raft visualization
# - Press 'm' for metrics detail
```

### 3. UI/UX Development

```bash
# Start hot-reload without cluster
./scripts/dev-workflow.sh hot-reload

# Start TUI with mock data
cargo run -- tui

# Edit UI code - instant feedback
# Test with simulated cluster state
```

### 4. Performance Testing

```bash
# Start test watcher
./scripts/dev-workflow.sh test-watch

# Edit code and see test results immediately
# Performance metrics in debug mode
# Memory usage tracking
```

## Troubleshooting

### Common Issues

1. **Build Failures**
   ```bash
   # Check syntax errors
   cargo check
   
   # Reset development environment
   ./scripts/dev-workflow.sh stop
   ./scripts/dev-workflow.sh start
   ```

2. **TUI Not Responding**
   ```bash
   # Check if TUI is running
   ./scripts/dev-workflow.sh status
   
   # Restart TUI
   ./scripts/dev-workflow.sh restart-tui
   
   # Check logs for errors
   ./scripts/dev-workflow.sh logs
   ```

3. **Cluster Connection Issues**
   ```bash
   # Verify cluster is running
   cargo run -- status --address 127.0.0.1:9000
   
   # Restart cluster with fresh data
   ./scripts/dev-workflow.sh stop
   ./scripts/dev-workflow.sh start-cluster
   ```

4. **Hot-Reload Not Working**
   ```bash
   # Check if watchexec is installed
   cargo install watchexec-cli
   
   # Restart hot-reload
   ./scripts/dev-workflow.sh hot-reload
   ```

### Debug Tips

1. **Use Debug Mode**: Press `6` and `s` in TUI for rich debug information
2. **Check Logs**: Development logs are in `dev-data/` directory
3. **Test Isolation**: Use `--tui-only` flag for testing without cluster setup
4. **State Inspection**: Use debug mode to understand Raft state and metrics

## Architecture Notes

### State Management
- App state is centralized in `App` struct
- Event-driven updates with async handling
- Separation between UI state and cluster data
- Debug state overlaid on production data

### Rendering Pipeline
- Tab-based rendering with mode switching
- Efficient updates using ratatui widgets
- Color-coded information display
- Responsive layout adaptation

### Testing Strategy
- Unit tests for individual components
- Integration tests for user workflows
- Property-based testing for state transitions
- Real cluster testing for end-to-end validation

### Performance Considerations
- Configurable refresh rates
- Lazy data loading
- Efficient event handling
- Memory-conscious debug logging