# TUI Connection Status Indicators

## Overview

The Blixard TUI now includes comprehensive connection status indicators that provide real-time visibility into the network connection between the TUI client and the cluster. This feature helps operators monitor connection health, identify network issues, and understand retry behavior.

## Features

### 1. Connection States
- **Connected** (🟢): Successfully connected to the cluster
- **Connecting** (🟡): Initial connection attempt in progress
- **Reconnecting** (🟠): Automatic reconnection after failure
- **Disconnected** (🔴): Not connected, waiting for retry
- **Failed** (❌): Connection failed after all retry attempts

### 2. Network Quality Indicators
Based on average latency measurements:
- **Excellent** (🟢): < 10ms latency
- **Good** (🟢): 10-50ms latency
- **Fair** (🟡): 50-100ms latency
- **Poor** (🟠): 100-200ms latency
- **Bad** (🔴): > 200ms latency

### 3. Real-Time Metrics
- **Latency**: Rolling average of operation response times
- **Retry Count**: Number of reconnection attempts
- **Next Retry**: Countdown to next reconnection attempt
- **Error Messages**: Detailed connection failure reasons

### 4. Visual Indicators

#### Status Bar
The bottom status bar displays connection information:
```
🟢 Connected (15ms) | Leader: Node 1 | VMs: 5/8 | Nodes: 3/3
```

#### Dashboard Cluster Status Card
Detailed connection information in the dashboard:
```
┌─ 🔗 Cluster Status ─┐
│ Network: 🟢 Good (127.0.0.1:7001)
│ Status: 🟢 Healthy
│ Nodes: 3/3
│ Leader: Node 1
│ Term: 42
│ Latency: 15ms
└────────────────────┘
```

## Implementation Details

### Connection Status Tracking
```rust
pub struct ConnectionStatus {
    pub state: ConnectionState,
    pub endpoint: String,
    pub connected_since: Option<Instant>,
    pub last_attempt: Option<Instant>,
    pub retry_count: u32,
    pub next_retry_in: Option<Duration>,
    pub latency_ms: Option<u64>,
    pub quality: NetworkQuality,
    pub error_message: Option<String>,
}
```

### Automatic Retry Logic
- Exponential backoff with configurable parameters
- Maximum retry attempts before marking as failed
- Automatic reconnection on network recovery
- Retry status visible in UI

### Latency Tracking
- Measures round-trip time for all operations
- Uses exponential moving average for stability
- Updates network quality based on average latency
- Displayed in milliseconds for precision

## Usage

### Monitoring Connection Health
1. Check the status bar for quick connection status
2. View the dashboard for detailed metrics
3. Monitor latency trends to identify network issues
4. Watch retry attempts during network problems

### Interpreting Indicators
- **Green indicators**: Connection is healthy
- **Yellow indicators**: Minor issues or connecting
- **Orange indicators**: Degraded performance or reconnecting
- **Red indicators**: Serious issues or disconnected

### Troubleshooting
1. **High Latency**: Check network congestion or server load
2. **Frequent Disconnects**: Investigate network stability
3. **Failed Connections**: Verify server is running and accessible
4. **Retry Loops**: Check for firewall or configuration issues

## Benefits

1. **Immediate Feedback**: Know connection status at a glance
2. **Network Monitoring**: Track latency and quality trends
3. **Automatic Recovery**: Resilient to temporary network issues
4. **Debugging Aid**: Detailed error messages for troubleshooting
5. **User Confidence**: Clear indication of system responsiveness

## Future Enhancements

1. **Historical Graphs**: Latency trends over time
2. **Multi-Endpoint Support**: Connect to multiple clusters
3. **Custom Alerts**: Threshold-based notifications
4. **Connection Logs**: Persistent connection history
5. **Network Diagnostics**: Built-in ping and traceroute
6. **Failover Support**: Automatic cluster endpoint switching