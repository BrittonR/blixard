# Blixard Monitoring Quick Start

This guide helps you quickly set up monitoring for your Blixard cluster.

## 1. Enable Monitoring (30 seconds)

Add to your node configuration:

```toml
[observability.metrics]
enabled = true
runtime_metrics = true

[observability.metrics.http_server]
enabled = true
bind = "0.0.0.0:9090"
```

## 2. View Metrics Instantly

Open your browser:
- **Metrics**: http://localhost:9090/metrics
- **Health**: http://localhost:9090/health

## 3. Quick Monitoring Commands

```bash
# Check node health
curl -s http://localhost:9090/health | jq .

# View key metrics
curl -s http://localhost:9090/metrics | grep -E "(raft_leader|vm_count|node_cpu|peer_connections)"

# Watch metrics in real-time
watch -n 1 'curl -s http://localhost:9090/metrics | grep -E "(raft_leader|vm_count)"'
```

## 4. Launch Full Monitoring Stack (Optional)

```bash
# Start Prometheus + Grafana
docker-compose -f monitoring/docker-compose.yml up -d

# Access dashboards
open http://localhost:3000  # Grafana (admin/admin)
```

## 5. Key Metrics to Watch

| Metric | What it tells you |
|--------|------------------|
| `raft_is_leader` | Which node is the current leader |
| `vm_total_count` | Number of VMs in cluster |
| `node_cpu_usage_percent` | CPU usage per node |
| `peer_active_connections` | P2P network health |
| `raft_proposals_total` | Cluster write activity |

## 6. Quick Troubleshooting

```bash
# No metrics showing?
blixard node status --monitoring

# Connection refused?
netstat -tlnp | grep 9090

# Check logs
journalctl -u blixard -f | grep -i metric
```

## 7. Production Setup

For production, see:
- [Full Monitoring Guide](./monitoring.md)
- [Grafana Dashboards](../monitoring/grafana/dashboards/)
- [Alert Runbooks](./runbooks/)