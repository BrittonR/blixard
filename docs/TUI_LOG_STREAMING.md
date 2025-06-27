# TUI Log Streaming View

## Overview

The Blixard TUI now includes a comprehensive log streaming viewer that provides real-time visibility into cluster operations. This feature allows operators to monitor system events, debug issues, and track cluster activity with powerful filtering capabilities.

## Features

### 1. Multi-Source Log Aggregation
- **System Logs**: General system events and cluster operations
- **Node Logs**: Per-node events with individual node tracking
- **VM Logs**: Virtual machine lifecycle and operation logs
- **Raft Consensus Logs**: Distributed consensus protocol events
- **gRPC Server Logs**: API server activity and requests
- **Extensible Sources**: Easy to add new log sources

### 2. Real-Time Streaming
- **Follow Mode**: Automatically scrolls to show latest entries
- **Manual Scroll**: Pause following to examine specific entries
- **Buffer Management**: Configurable buffer size (default 1000 entries)
- **Live Updates**: Logs appear as events occur in the system

### 3. Advanced Filtering
- **Log Level Filtering**: Debug, Info, Warning, Error
- **Source Selection**: Enable/disable specific log sources
- **Text Search**: Filter logs by content
- **Timestamp Display**: Toggle timestamp visibility
- **Error Highlighting**: Visual emphasis on error entries

### 4. Interactive Controls
- **Keyboard Navigation**:
  - `Shift+L`: Open log viewer from anywhere
  - `Space`: Toggle follow mode
  - `L`: Cycle through log levels
  - `T`: Toggle timestamps
  - `E`: Toggle error highlighting
  - `↑/↓`: Select log sources
  - `Enter`: Enable/disable selected source
  - `C`: Clear log buffer
  - `S`: Simulate log entries (for testing)
  - `PageUp/PageDown`: Scroll through logs
  - `Esc`: Exit log viewer

### 5. Visual Design
- **Sidebar**: Log source selector with enable/disable status
- **Filter Bar**: Log level, search, and display options
- **Main View**: Scrollable log entries with color coding
- **Status Bar**: Entry counts, mode indicators, and help text
- **Color Coding**: Different colors for each log source and severity

## Implementation Details

### Log Entry Structure
```rust
pub struct LogEntry {
    pub timestamp: Instant,
    pub source: LogSourceType,
    pub level: LogLevel,
    pub message: String,
    pub details: Option<String>,
}
```

### Log Sources
```rust
pub enum LogSourceType {
    Node(u64),      // Node ID
    Vm(String),     // VM name
    System,         // System-wide logs
    Raft,           // Raft consensus logs
    GrpcServer,     // gRPC server logs
    All,            // All sources
}
```

### Integration with Event System
The log streaming view is automatically populated by the existing event system. Any call to `add_event()` will also create a corresponding log entry, ensuring comprehensive logging without code duplication.

## Usage Examples

### Opening the Log Viewer
1. Press `Shift+L` from any screen to open the log viewer
2. The viewer opens in follow mode by default
3. All log sources are displayed initially

### Filtering Logs
1. Press `L` to cycle through log levels (Debug → Info → Warning → Error)
2. Use arrow keys to select a log source in the sidebar
3. Press `Enter` to toggle the source on/off
4. Type in the search box to filter by content

### Examining Specific Events
1. Press `Space` to pause follow mode
2. Use `PageUp`/`PageDown` to scroll through history
3. Press `T` to show timestamps for timing analysis
4. Press `E` to highlight errors for quick identification

### Monitoring Specific Components
1. Disable unwanted sources using the sidebar
2. Set appropriate log level for noise reduction
3. Use search to filter for specific operations
4. Keep follow mode on for real-time updates

## Future Enhancements

1. **Persistent Filters**: Save and load filter configurations
2. **Export Functionality**: Save logs to file
3. **Remote Log Streaming**: Connect to remote node logs
4. **Log Persistence**: Store logs across TUI sessions
5. **Pattern Matching**: Regular expression support for advanced filtering
6. **Log Analytics**: Statistics and pattern detection
7. **Custom Log Sources**: Plugin system for external log sources
8. **Log Forwarding**: Send filtered logs to external systems

## Benefits

- **Real-Time Debugging**: Immediate visibility into system behavior
- **Comprehensive Coverage**: All system events in one place
- **Efficient Troubleshooting**: Powerful filtering reduces noise
- **Historical Analysis**: Buffer allows examining recent events
- **User-Friendly**: Intuitive controls and visual design
- **Performance**: Minimal overhead with configurable buffer size