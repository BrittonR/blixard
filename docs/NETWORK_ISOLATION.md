# VM Network Isolation in Blixard

This document describes the VM network isolation feature that ensures VMs cannot access cluster internal networks and provides tenant isolation.

## Overview

Blixard provides automatic network isolation for VMs using Linux firewall rules (iptables or nftables). This feature:

- **Prevents VMs from accessing cluster internal networks** - VMs cannot reach the control plane, Raft consensus, or management APIs
- **Provides tenant isolation** - VMs from different tenants cannot communicate with each other
- **Allows controlled internet access** - VMs can access the internet if configured
- **Works transparently** - No VM configuration changes needed

## Requirements

### Prerequisites

1. **Root privileges or CAP_NET_ADMIN capability** - Required to manage firewall rules
2. **Firewall backend** - Either `iptables` or `nftables` installed
3. **Network configuration** - VMs must use tap interfaces with routed networking

### Checking Requirements

```bash
# Check if you have required privileges
id -u  # Should be 0 for root
# OR check capabilities
capsh --print | grep cap_net_admin

# Check firewall availability
which nft     # For nftables (recommended)
which iptables  # For iptables
```

## Configuration

Network isolation is configured in the Blixard configuration file:

```toml
[vm.network_isolation]
enabled = true
backend = "nftables"  # or "iptables"

# Networks that VMs should not access (cluster internal)
cluster_networks = [
    "127.0.0.0/8",      # Loopback
    "169.254.0.0/16",   # Link-local
    "172.16.0.0/12",    # Private range for cluster
]

# VM network range
vm_network = "10.0.0.0/16"

# Isolation options
inter_vm_isolation = false  # Prevent all VM-to-VM communication
tenant_isolation = true     # Isolate VMs by tenant
allow_internet = true       # Allow VMs to access internet

# Allowed ports for VMs
[[vm.network_isolation.allowed_ports]]
protocol = "udp"
port = 53
direction = "outbound"  # DNS

[[vm.network_isolation.allowed_ports]]
protocol = "tcp"
port = 53
direction = "outbound"  # DNS over TCP
```

## How It Works

### Firewall Architecture

When network isolation is enabled, Blixard creates custom firewall chains:

#### nftables (Recommended)
```
table inet blixard {
    chain vm_forward {
        # Block VM access to cluster networks
        ip saddr 10.0.0.0/16 ip daddr 172.16.0.0/12 drop
        
        # Allow established connections
        ct state established,related accept
        
        # Per-VM rules added dynamically
    }
}
```

#### iptables
```
-N BLIXARD_VM_IN
-N BLIXARD_VM_OUT  
-N BLIXARD_VM_FWD

# Hook into main chains
-I FORWARD -j BLIXARD_VM_FWD
-I INPUT -j BLIXARD_VM_IN
-I OUTPUT -j BLIXARD_VM_OUT
```

### Per-VM Rules

When a VM is created, Blixard automatically:

1. **Blocks VM from accessing host** - Prevents VMs from reaching the node's services
2. **Applies tenant isolation** - Only allows communication within the same tenant
3. **Configures allowed ports** - Opens specific ports like DNS
4. **Manages internet access** - Controls external connectivity

### Tenant Isolation

VMs are isolated by tenant ID:
- VMs in the same tenant can communicate (unless `inter_vm_isolation` is enabled)
- VMs in different tenants cannot communicate
- Each VM's tenant is specified in its configuration

## Usage

### Basic Usage

Network isolation is applied automatically when:
1. The feature is enabled in configuration
2. The node has sufficient privileges
3. A VM is created through Blixard

```bash
# Create a VM with tenant isolation
blixard vm create --name my-vm --tenant production

# VMs in different tenants cannot communicate
blixard vm create --name vm1 --tenant tenant-a
blixard vm create --name vm2 --tenant tenant-b
# vm1 and vm2 cannot reach each other
```

### Verifying Isolation

```bash
# Check if network isolation is active
blixard security status

# View current firewall rules
# For nftables:
sudo nft list table inet blixard

# For iptables:
sudo iptables -L BLIXARD_VM_FWD -n -v
```

### Debugging

If network isolation isn't working:

1. **Check privileges**:
   ```bash
   # Must be root or have CAP_NET_ADMIN
   id -u
   getcap /path/to/blixard
   ```

2. **Check firewall backend**:
   ```bash
   # Ensure nft or iptables is installed
   which nft || which iptables
   ```

3. **Check logs**:
   ```bash
   # Look for network isolation messages
   journalctl -u blixard | grep -i "network isolation"
   ```

4. **Verify rules are applied**:
   ```bash
   # Should see VM-specific chains/rules
   sudo nft list ruleset | grep blixard
   ```

## Security Considerations

### Default Deny

By default, VMs are denied access to:
- All cluster internal networks
- The host node's services
- Other tenants' VMs

### Allowed Traffic

VMs are allowed to:
- Communicate within their tenant (configurable)
- Access configured ports (DNS by default)
- Access the internet (configurable)

### Best Practices

1. **Use separate network ranges** - Keep VM networks distinct from cluster networks
2. **Enable tenant isolation** - Always isolate VMs by tenant in multi-tenant environments
3. **Limit allowed ports** - Only open necessary ports for VM operation
4. **Monitor firewall logs** - Check for blocked connection attempts
5. **Regular audits** - Periodically review firewall rules

## Limitations

1. **Requires privileges** - Cannot run without root or CAP_NET_ADMIN
2. **Linux only** - Uses Linux netfilter (iptables/nftables)
3. **No migration support** - VM migration with network isolation is not yet implemented
4. **Static rules** - Rules are not dynamically updated if VM IPs change

## Troubleshooting

### VMs Cannot Access DNS
```toml
# Ensure DNS ports are allowed
[[vm.network_isolation.allowed_ports]]
protocol = "udp"
port = 53
direction = "outbound"
```

### VMs Cannot Access Internet
```toml
# Enable internet access
[vm.network_isolation]
allow_internet = true
```

### Firewall Rules Not Applied
```bash
# Check Blixard logs
journalctl -u blixard | grep -E "network isolation|firewall"

# Verify privileges
sudo -v || echo "Need sudo/root access"
```

### Performance Impact
- Minimal overhead for packet filtering
- Rules are optimized for connection tracking
- Use nftables for better performance on modern systems