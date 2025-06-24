# Host Bridge Setup for Routed VM Networking

Blixard uses routed networking with macvtap interfaces for VM connectivity. This requires one-time host setup to create a bridge for VM communication.

## NixOS Users (Recommended)

If you're using NixOS, use the provided NixOS module for automatic setup:

```nix
# Add to your /etc/nixos/configuration.nix
{ config, pkgs, ... }:

{
  imports = [
    /path/to/blixard/nix/modules/blixard-host.nix
  ];

  services.blixard.enable = true;
  
  # Add your user to necessary groups
  users.users.yourusername.extraGroups = [ "libvirtd" "kvm" ];
}
```

Then rebuild: `sudo nixos-rebuild switch`

See `nix/README.md` for detailed module documentation.

## Manual Setup (Other Linux Distributions)

### Prerequisites

- Linux host with root/sudo access
- Bridge utilities installed (`bridge-utils` or `iproute2`)
- IP forwarding enabled

## Setup Commands

### 1. Create Bridge Interface
```bash
# Create bridge for VM network
sudo ip link add br0 type bridge

# Assign gateway IP to bridge
sudo ip addr add 10.0.0.1/24 dev br0

# Bring bridge up
sudo ip link set br0 up
```

### 2. Enable IP Forwarding
```bash
# Enable IP forwarding (temporary)
sudo sysctl net.ipv4.ip_forward=1

# Make permanent by adding to /etc/sysctl.conf
echo 'net.ipv4.ip_forward=1' | sudo tee -a /etc/sysctl.conf
```

### 3. Configure NAT (Optional - for internet access)
```bash
# Add NAT rule for VM internet access
sudo iptables -t nat -A POSTROUTING -s 10.0.0.0/24 -j MASQUERADE

# Allow forwarding through bridge
sudo iptables -A FORWARD -i br0 -j ACCEPT
sudo iptables -A FORWARD -o br0 -j ACCEPT
```

### 4. Make Rules Persistent (Ubuntu/Debian)
```bash
# Install iptables-persistent
sudo apt install iptables-persistent

# Save current rules
sudo iptables-save > /etc/iptables/rules.v4
```

## Network Configuration

- **Subnet**: `10.0.0.0/24`
- **Gateway**: `10.0.0.1` (bridge interface)
- **VM Range**: `10.0.0.10` - `10.0.0.254`
- **DNS**: `8.8.8.8`, `1.1.1.1`

## Verification

```bash
# Check bridge status
ip addr show br0

# Verify IP forwarding
cat /proc/sys/net/ipv4/ip_forward

# List iptables rules
sudo iptables -t nat -L
sudo iptables -L FORWARD
```

## VM Connectivity

Once setup is complete, VMs will:
- Get assigned IPs automatically (10.0.0.10, 10.0.0.11, etc.)
- Be reachable directly via SSH: `ssh root@10.0.0.10`
- Have internet access through NAT
- Communicate with each other on the same subnet

## Cleanup (if needed)

```bash
# Remove bridge
sudo ip link delete br0

# Remove iptables rules
sudo iptables -t nat -D POSTROUTING -s 10.0.0.0/24 -j MASQUERADE
sudo iptables -D FORWARD -i br0 -j ACCEPT
sudo iptables -D FORWARD -o br0 -j ACCEPT
```

## Troubleshooting

**VMs can't reach internet:**
- Check IP forwarding: `cat /proc/sys/net/ipv4/ip_forward`
- Verify NAT rules: `sudo iptables -t nat -L`

**Can't SSH to VMs:**
- Check VM IP assignment: `ip addr` inside VM
- Verify bridge interface: `ip addr show br0`
- Test connectivity: `ping 10.0.0.10`

**Permission denied errors:**
- Ensure user has access to `/dev/vhost-net`
- Check bridge permissions with `ls -la /sys/class/net/br0`