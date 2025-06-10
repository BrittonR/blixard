# Monitoring and Observability

This guide covers monitoring, observability, and security aspects of Blixard deployments.

## Metrics and Monitoring

### System Metrics

Blixard exposes comprehensive metrics for all components:

- **Cluster Health**: Node status, leader election, consensus performance
- **VM Metrics**: Running VMs, health status, resource usage, migration statistics
- **Storage Metrics**: Ceph cluster health, OSD performance, replication status
- **Hypervisor Metrics**: KVM/Firecracker performance, VM density, migration latency
- **Network Metrics**: Virtual network performance, VLAN utilization, load balancer stats
- **Performance Metrics**: Operation latency, throughput, error rates

### Prometheus Integration

Export metrics to Prometheus for collection and alerting:

```bash
# Enable Prometheus metrics export
blixard metrics export --format prometheus --endpoint http://prometheus:9090

# Export Ceph metrics via mgr module
blixard storage metrics export --format prometheus --ceph-mgr-module

# Configure scrape target in prometheus.yml
scrape_configs:
  - job_name: 'blixard'
    static_configs:
      - targets: ['blixard-node1:9090', 'blixard-node2:9090']
```

### Grafana Dashboards

Generate and import pre-built dashboards:

```bash
# Generate Grafana dashboards
blixard dashboard generate --format grafana --type cluster > cluster-dashboard.json
blixard dashboard generate --format grafana --type storage > ceph-dashboard.json
blixard dashboard generate --format grafana --type vms > vm-dashboard.json

# Import into Grafana
curl -X POST http://grafana:3000/api/dashboards/db \
  -H "Content-Type: application/json" \
  -d @cluster-dashboard.json
```

### Custom Webhooks and Alerting

Configure alerts for critical events:

```bash
# Configure webhook endpoint
blixard alerts configure --webhook https://alerts.example.com/webhook

# Set up alert rules
blixard alerts rule --vm-cpu-high 80% --storage-full 90% --migration-failure

# Define custom alert conditions
blixard alerts custom --condition "cluster.nodes < 3" \
  --message "Cluster below minimum nodes" \
  --severity critical
```

## Logging and Audit Trail

### Structured Logging

Blixard uses structured JSON logging for easy parsing:

```json
{
  "timestamp": "2024-12-09T15:30:00Z",
  "level": "INFO",
  "component": "consensus",
  "event": "service_start_proposed",
  "service_name": "nginx",
  "node_id": "node1",
  "raft_term": 15,
  "trace_id": "abc123"
}
```

### Log Aggregation

Configure centralized logging:

```toml
# /etc/blixard/config.toml
[logging]
format = "json"
level = "info"
output = "stdout"

[logging.aggregation]
enabled = true
endpoint = "https://logs.example.com"
batch_size = 1000
flush_interval = "5s"
```

### Audit Trail

Blixard maintains a comprehensive audit trail:

- **All Operations**: Complete record of all service management operations
- **Security Events**: Authentication, authorization, configuration changes
- **System Events**: Node joins/leaves, leader elections, failures
- **Compliance**: Immutable audit log for regulatory requirements

Access audit logs:

```bash
# View recent audit events
blixard audit list --since 1h

# Export audit logs
blixard audit export --format json --start 2024-01-01 --end 2024-12-31

# Search audit logs
blixard audit search --user alice --action "vm.create"
```

## Security and Access Control

### Authentication and Authorization

#### Role-Based Access Control (RBAC)

Define roles and permissions:

```bash
# Create roles
blixard rbac create-role service-operator \
  --permissions "service:start,service:stop,service:status"

blixard rbac create-role cluster-admin \
  --permissions "cluster:*,service:*,user:*"

# Assign roles to users
blixard rbac assign-role alice service-operator
blixard rbac assign-role bob cluster-admin

# View role assignments
blixard rbac list-roles
blixard rbac show-role service-operator
```

#### API Key Management

Generate and manage API keys for automation:

```bash
# Generate API keys for automation
blixard auth create-key --name ci-cd --role service-operator
API_KEY=blix_ak_1234567890abcdef

# List active keys
blixard auth list-keys

# Revoke compromised key
blixard auth revoke-key ci-cd

# Rotate keys periodically
blixard auth rotate-key ci-cd --expire-old-in 7d
```

### Network Security

Blixard implements multiple layers of network security:

- **mTLS**: All inter-node communication encrypted and authenticated
- **Tailscale Integration**: Secure mesh networking with identity-based access
- **Network Policies**: Control which services can communicate
- **Firewall Rules**: Automatic iptables integration for service ports

Configure network security:

```bash
# Enable mTLS for cluster communication
blixard security enable-mtls --ca-cert /etc/blixard/ca.crt

# Configure network policies
blixard network-policy create --name web-to-db \
  --from-label app=web \
  --to-label app=database \
  --ports 5432

# View security status
blixard security status
```

## Monitoring Best Practices

### Essential Metrics to Monitor

1. **Cluster Health**
   - Node availability and status
   - Leader election frequency
   - Consensus round-trip time
   - Split-brain detection

2. **VM Performance**
   - CPU and memory utilization
   - Disk I/O and network throughput
   - Live migration success rate
   - VM density per node

3. **Storage Health**
   - Ceph cluster status
   - OSD up/down status
   - Replication lag
   - Available capacity

4. **Application Performance**
   - Request latency percentiles
   - Error rates
   - Throughput
   - Resource saturation

### Alert Configuration

Configure meaningful alerts:

```yaml
# Example alert rules
groups:
  - name: blixard_cluster
    rules:
      - alert: NodeDown
        expr: up{job="blixard"} == 0
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "Blixard node {{ $labels.instance }} is down"
          
      - alert: HighCPUUsage
        expr: blixard_vm_cpu_usage > 0.9
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "VM {{ $labels.vm_name }} high CPU usage"
          
      - alert: StorageNearFull
        expr: blixard_storage_usage > 0.85
        for: 30m
        labels:
          severity: warning
        annotations:
          summary: "Storage pool {{ $labels.pool }} is {{ $value }}% full"
```

### Dashboard Examples

Key metrics to include in dashboards:

1. **Cluster Overview**
   - Node status grid
   - Consensus metrics
   - Resource utilization
   - Operation throughput

2. **VM Dashboard**
   - VM count and distribution
   - Resource allocation
   - Migration activity
   - Performance metrics

3. **Storage Dashboard**
   - Ceph health status
   - OSD distribution
   - I/O performance
   - Capacity planning

## Troubleshooting with Metrics

### Common Issues and Metrics

1. **Slow Consensus**
   - Check: `blixard_consensus_round_duration`
   - Look for network latency or CPU saturation

2. **VM Migration Failures**
   - Check: `blixard_vm_migration_success_rate`
   - Review storage performance and network bandwidth

3. **Storage Performance**
   - Check: `ceph_osd_latency` and `ceph_osd_throughput`
   - Identify slow OSDs or network issues

### Debug Commands

```bash
# Enable debug logging for specific component
blixard debug enable --component consensus --duration 5m

# Capture performance profile
blixard debug profile --duration 30s --output profile.pprof

# Generate debug bundle
blixard debug bundle --include-logs --include-metrics --output debug.tar.gz
```

## See Also

- [Installation Guide](installation.md) - Initial setup
- [Configuration Guide](configuration.md) - Configuration options
- [Security Best Practices](../administrator-guide/security.md) - Detailed security guide