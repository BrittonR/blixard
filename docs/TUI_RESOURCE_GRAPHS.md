# TUI Resource Usage Graphs

## Overview

The Blixard TUI now includes comprehensive resource usage monitoring with real-time graphs and health tracking. This feature provides visual insights into cluster performance and helps identify resource bottlenecks.

## Features Implemented

### 1. Resource Usage Graphs (Monitoring Tab - Tab 4)

#### Main Resource Charts
- **CPU Usage Graph**: Real-time CPU utilization with sparkline visualization
  - Shows current usage percentage and average
  - Color-coded thresholds (green < 60%, yellow 60-80%, red > 80%)
  - Historical data tracking with configurable retention

- **Memory Usage Graph**: Memory consumption tracking
  - Displays used/total memory in GB
  - Percentage-based visualization
  - Color-coded alerts for high memory usage

- **Network I/O Graph**: Network traffic monitoring
  - Tracks MB/s throughput
  - Simulates realistic network patterns based on VM activity
  - Automatic scaling for visibility

- **Disk I/O Graph**: Storage performance metrics
  - Read/write speeds in MB/s
  - Activity sparkline visualization

#### Per-Node Resource Grid
- Tabular view of all nodes with:
  - Node ID and role (Leader marked with 👑)
  - Health status icons (✅ Healthy, ⚠️ Warning, 🚨 Critical)
  - CPU and memory percentages with color coding
  - VM count per node
  - Network activity indicators

### 2. Health Monitoring Dashboard

#### Health Status Bar
- Overall cluster health status with visual indicators
- Node health distribution (healthy/degraded/failed counts)
- Active alert summary with severity counts
- Key health metrics (latency, replication lag, leader changes)

#### Health Alerts System
- Automatic alert generation for:
  - High CPU usage (> 80% warning, > 90% critical)
  - High memory usage (> 90% critical)
  - Node failures or critical states
  - Cluster-wide issues
- Alert retention and automatic cleanup of resolved alerts
- Severity-based color coding and icons

#### Cluster Health Tracking
- Uptime monitoring
- Leader stability tracking (leader change count)
- Network latency measurements
- Replication lag monitoring
- Historical health snapshots per node

### 3. Performance Metrics Display
- Real-time cluster health percentage
- Average resource utilization
- Total VM counts with running/stopped breakdown
- Integrated health alerts alongside metrics

## Technical Implementation

### Data Collection
- Resource metrics are updated on each refresh cycle
- Historical data maintained with configurable retention (default 100 points)
- Per-node health snapshots track individual node performance
- Simulated resource usage for demonstration (ready for real metric integration)

### UI Components
- Uses ratatui's Sparkline widget for graph rendering
- Responsive layout with proper constraint management
- Color-coded indicators for quick status assessment
- Efficient rendering with minimal redraw overhead

### Integration Points
- Ready for integration with actual system metrics collectors
- Placeholder for Prometheus/OpenTelemetry metric sources
- Extensible alert system for custom health checks

## Usage

1. **Access Monitoring**: Press `4` to switch to the Monitoring tab
2. **View Graphs**: Resource graphs update automatically during refresh cycles
3. **Check Alerts**: Health alerts appear in the dashboard and monitoring views
4. **Manual Refresh**: Press `r` to force data refresh
5. **Performance Modes**: Press `p` to cycle through update frequencies

## Configuration

The monitoring system respects the TUI performance mode settings:
- **HighRefresh**: Updates every 125ms (half of tick rate)
- **Balanced**: Standard refresh interval
- **PowerSaver**: Updates every 2x refresh rate
- **Debug**: Updates at configured frequency

## Future Enhancements

- Integration with real system metrics (CPU, memory, network, disk)
- Prometheus metric export/import
- Custom alert thresholds and rules
- Historical data persistence
- Zoom and pan controls for graphs
- Export graph data to CSV/JSON