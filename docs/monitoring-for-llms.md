# Monitoring Blixard (LLM-Optimized Guide)

> **Status**: ✅ All monitoring commands tested and working

This guide provides commands optimized for LLMs to quickly understand cluster state.

## Quick Status Commands

### 1. One-Line Health Check
```bash
# Most concise status - perfect for LLMs
blixard monitor summary

# Output format:
# BLIXARD STATUS @ 2025-01-25 10:30:45 UTC
# Node: 127.0.0.1:7001
# Status: HEALTHY
# Leader: YES
# Term: 42
# VMs: 3/5 running
# CPU: 45.2%
# Memory: 62.1%
# Peers: 2
```

### 2. JSON Output (Best for Parsing)
```bash
# Get structured data
blixard monitor summary --format json

# Output:
{
  "timestamp": "2025-01-25T10:30:45Z",
  "node": "127.0.0.1:7001",
  "status": "healthy",
  "summary": {
    "metrics_available": true,
    "health_available": true,
    "is_leader": true,
    "raft_term": 42,
    "total_vms": 5,
    "running_vms": 3,
    "cpu_percent": 45.2,
    "memory_percent": 62.1,
    "active_peers": 2
  }
}
```

### 3. Specific Metrics (CSV Format)
```bash
# Get metrics as CSV for easy parsing
blixard monitor metrics --format csv

# Output:
metric,value
blixard_raft_is_leader,1
blixard_raft_term,42
blixard_vm_total_count,5
blixard_vm_running_count,3
```

## Nix-Based Monitoring Tools

### Quick Install
```bash
# Enter monitoring shell with all tools
nix develop ./monitoring

# Or run directly
nix run ./monitoring#blixard-health
```

### Available Tools

1. **blixard-health** - Single line health check
   ```bash
   $ blixard-health
   Checking 127.0.0.1:7001... ✅ HEALTHY
   Leader:Y Term:42 VMs:3/5 CPU:45.2% Mem:62.1% Peers:2
   ```

2. **blixard-metrics-export** - Export in various formats
   ```bash
   # JSON format
   blixard-metrics-export 127.0.0.1:7001 json
   
   # CSV format  
   blixard-metrics-export 127.0.0.1:7001 csv
   
   # Raw Prometheus format
   blixard-metrics-export 127.0.0.1:7001 prometheus
   ```

3. **blixard-cluster-status** - Multi-node overview
   ```bash
   $ BLIXARD_NODES="node1:7001,node2:7001,node3:7001" blixard-cluster-status
   
   BLIXARD CLUSTER STATUS
   ======================
   
   Node node1:7001: ✅ HEALTHY Leader:Y Term:42 VMs:3/5 CPU:45.2% Mem:62.1% Peers:2
   Node node2:7001: ✅ HEALTHY Leader:N Term:42 VMs:2/5 CPU:32.1% Mem:55.3% Peers:2
   Node node3:7001: ❌ UNREACHABLE
   ```

## Key Metrics Explained

For LLMs analyzing the system:

| Metric | Meaning | Healthy Range |
|--------|---------|---------------|
| is_leader | Is this node the Raft leader? | One node should be leader |
| raft_term | Current consensus term | Should increase rarely |
| total_vms | Total VMs registered | - |
| running_vms | Currently active VMs | Should match expected |
| cpu_percent | Node CPU usage | < 80% |
| memory_percent | Node memory usage | < 90% |
| active_peers | Connected cluster nodes | Should be N-1 |

## Quick Diagnostics

```bash
# Is cluster healthy?
blixard monitor summary | grep "Status: HEALTHY"

# Who is the leader?
blixard monitor metrics --format csv | grep is_leader

# How many VMs are running?
blixard monitor metrics --format json | jq '.metrics.blixard_vm_running_count'

# Is this node overloaded?
blixard monitor summary --format json | jq '.summary | {cpu: .cpu_percent, mem: .memory_percent}'
```

## Automation-Friendly Commands

```bash
# Exit code indicates health (0=healthy, 1=unhealthy)
blixard-health && echo "Cluster OK" || echo "Cluster Problem"

# Watch for state changes
while true; do
  STATE=$(blixard monitor summary --format json | jq -c '.summary')
  echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) $STATE"
  sleep 5
done

# Alert on leader changes
LAST_LEADER=""
while true; do
  LEADER=$(blixard monitor summary --format json | jq -r '.summary.is_leader')
  if [[ "$LEADER" != "$LAST_LEADER" ]]; then
    echo "LEADER CHANGED: $LAST_LEADER -> $LEADER"
    LAST_LEADER=$LEADER
  fi
  sleep 10
done
```

## Enable Monitoring

Add to your Blixard config:

```toml
[observability.metrics]
enabled = true
runtime_metrics = true

[observability.metrics.http_server]
enabled = true
bind = "0.0.0.0:9090"
```

## Quick Reference

```bash
# Most useful commands for LLMs:
blixard monitor summary                    # Human-readable status
blixard monitor summary --format json      # Structured data
blixard monitor metrics --format csv       # Metrics table
blixard monitor health                     # Component health
blixard-health                            # One-line status (Nix tool)
```