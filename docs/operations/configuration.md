# Configuration Management

This guide covers Blixard's configuration options and operational procedures for managing your cluster.

## Configuration File

Blixard uses TOML format for configuration. The main configuration file is located at `/etc/blixard/config.toml`.

### Complete Configuration Example

```toml
# /etc/blixard/config.toml
[cluster]
node_id = "node1"
data_dir = "/var/lib/blixard"
log_level = "info"

[discovery]
method = "tailscale"
port = 7946

[consensus]
election_timeout = "5s"
heartbeat_interval = "1s"
snapshot_threshold = 1000

[microvm]
default_hypervisor = "firecracker"
default_memory_mb = 512
default_vcpus = 1
max_vms_per_node = 100
nix_config_dir = "/etc/blixard/vm-configs"

[storage.ceph]
config_file = "/etc/ceph/ceph.conf"
default_pool = "ssd-pool"
replication_factor = 3
pg_num = 128
min_size = 2

[storage.pools.ssd-pool]
type = "replicated"
size = 3
min_size = 2
rule = "ssd_rule"

[storage.pools.hdd-pool]
type = "erasure"
k = 4
m = 2
rule = "hdd_rule"

[network]
vlan_range = "100-200"
bridge_name = "blixard0"
default_gateway = "10.0.0.1"
dhcp_pool = "10.0.1.0/24"

[monitoring]
metrics_enabled = true
metrics_port = 9090
ceph_dashboard = true
log_aggregation = true
grafana_enabled = true
```

## Configuration Sections

### Cluster Configuration

Basic cluster settings:

```toml
[cluster]
node_id = "node1"              # Unique identifier for this node
data_dir = "/var/lib/blixard"  # Directory for persistent data
log_level = "info"             # Log verbosity: debug, info, warn, error
```

### Discovery Configuration

How nodes find each other:

```toml
[discovery]
method = "tailscale"  # Options: tailscale, static, consul
port = 7946          # Port for cluster communication

# For static discovery
[discovery.static]
nodes = ["node1:7946", "node2:7946", "node3:7946"]

# For Tailscale discovery
[discovery.tailscale]
auth_key = "tskey-..."  # Tailscale auth key
tags = ["blixard"]      # Tags to filter nodes
```

### Consensus Configuration

Raft consensus parameters:

```toml
[consensus]
election_timeout = "5s"       # Time before starting new election
heartbeat_interval = "1s"     # Leader heartbeat frequency
snapshot_threshold = 1000     # Entries before creating snapshot
max_inflight_msgs = 256       # Maximum in-flight messages
```

### MicroVM Configuration

VM management settings:

```toml
[microvm]
default_hypervisor = "firecracker"  # Options: firecracker, qemu, cloud-hypervisor
default_memory_mb = 512             # Default VM memory
default_vcpus = 1                   # Default VM CPU count
max_vms_per_node = 100             # Maximum VMs per node
nix_config_dir = "/etc/blixard/vm-configs"  # Directory for VM Nix configs

# Firecracker-specific settings
[microvm.firecracker]
binary_path = "/usr/bin/firecracker"
jailer_path = "/usr/bin/jailer"
kernel_path = "/var/lib/blixard/vmlinux"
```

### Storage Configuration

Ceph storage settings:

```toml
[storage.ceph]
config_file = "/etc/ceph/ceph.conf"
default_pool = "ssd-pool"
replication_factor = 3
pg_num = 128
min_size = 2

# Define storage pools
[storage.pools.ssd-pool]
type = "replicated"
size = 3          # Number of replicas
min_size = 2      # Minimum replicas for I/O
rule = "ssd_rule" # CRUSH rule name

[storage.pools.hdd-pool]
type = "erasure"
k = 4  # Data chunks
m = 2  # Parity chunks
rule = "hdd_rule"
```

### Network Configuration

Virtual networking settings:

```toml
[network]
vlan_range = "100-200"          # Available VLAN IDs
bridge_name = "blixard0"        # Main bridge interface
default_gateway = "10.0.0.1"    # Gateway for VMs
dhcp_pool = "10.0.1.0/24"       # DHCP address pool
dns_servers = ["8.8.8.8", "8.8.4.4"]  # DNS for VMs
```

### Monitoring Configuration

Observability settings:

```toml
[monitoring]
metrics_enabled = true
metrics_port = 9090
ceph_dashboard = true
log_aggregation = true
grafana_enabled = true

[monitoring.prometheus]
scrape_interval = "15s"
retention = "30d"

[monitoring.grafana]
port = 3000
default_dashboards = true
```

## Operational Procedures

### Adding New Nodes

To add a new node to an existing cluster:

```bash
# On existing cluster node
blixard cluster add-node --node-id node4 --address 10.0.0.4 \
  --roles "hypervisor,ceph-osd"

# On new node (with storage)
blixard init --cluster-join cluster.example.com --node-id node4 \
  --ceph-osd /dev/nvme0n1 --osd-weight 1.0

# Verify integration and rebalancing
blixard cluster status --verbose
blixard storage status --show-rebalancing
```

### Removing Nodes

Gracefully remove a node from the cluster:

```bash
# Graceful node removal with VM and storage migration
blixard cluster remove-node node4 --migrate-vms --drain-osds

# Wait for rebalancing to complete
blixard storage wait-for-rebalance --timeout 30m

# Emergency removal (node is down)
blixard cluster force-remove-node node4 --mark-osds-down
blixard storage osd crush remove osd.4
```

### Upgrading Blixard

Perform rolling upgrades with zero downtime:

```bash
# Rolling upgrade with VM live migration (requires 3+ nodes)
blixard cluster upgrade --version v2.0.0 --strategy rolling \
  --migrate-vms --ceph-safe-mode

# Manual upgrade with maintenance mode
blixard cluster maintenance-mode enable
blixard storage set-flag noout  # Prevent Ceph rebalancing
# Upgrade Blixard binary on all nodes
blixard storage unset-flag noout
blixard cluster maintenance-mode disable
```

## Environment Variables

Blixard respects these environment variables:

- `BLIXARD_CONFIG`: Path to configuration file (default: `/etc/blixard/config.toml`)
- `BLIXARD_DATA_DIR`: Override data directory location
- `BLIXARD_LOG_LEVEL`: Override log level
- `BLIXARD_NODE_ID`: Override node ID
- `RUST_LOG`: Fine-grained Rust logging control

## Configuration Best Practices

1. **Version Control**: Keep configuration files in version control
2. **Consistent IDs**: Use meaningful, consistent node IDs across cluster
3. **Resource Limits**: Set appropriate VM limits per node
4. **Storage Planning**: Size Ceph PG numbers based on OSD count
5. **Network Isolation**: Use separate VLANs for management and VM traffic
6. **Monitoring**: Always enable metrics in production
7. **Backup Config**: Keep configuration backups before changes

## Validation

Validate configuration before applying:

```bash
# Check configuration syntax
blixard config validate --file /etc/blixard/config.toml

# Test configuration in dry-run mode
blixard config apply --dry-run --file /etc/blixard/config.toml

# Show effective configuration (with defaults)
blixard config show --effective
```

## Dynamic Configuration

Some settings can be changed at runtime:

```bash
# Update log level
blixard config set cluster.log_level debug

# Update VM defaults
blixard config set microvm.default_memory_mb 1024

# View current runtime configuration
blixard config get cluster.log_level
```

## See Also

- [Installation Guide](installation.md) - Initial setup procedures
- [Monitoring Guide](monitoring.md) - Observability configuration
- [Production Deployment](../administrator-guide/production-deployment.md) - Production considerations