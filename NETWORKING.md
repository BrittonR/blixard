# Blixard MicroVM Networking

Blixard uses **routed networking** following the [microvm.nix routed network pattern](https://astro.github.io/microvm.nix/routed-network.html). This provides full VM-to-VM and host-to-VM connectivity while maintaining security and isolation.

## Network Architecture

```
Host (10.0.0.0)
├── vm1 interface → VM 1 (10.0.0.1)
├── vm2 interface → VM 2 (10.0.0.2)  
├── vm3 interface → VM 3 (10.0.0.3)
└── ...
```

### IPv4 Layout
- **Host gateway**: `10.0.0.0`
- **VM addresses**: `10.0.0.1` to `10.0.0.64`
- **Subnet**: Each VM gets a `/32` address with routes to the host

### IPv6 Layout  
- **Host gateway**: `fec0::`
- **VM addresses**: `fec0::1` to `fec0::40` (hex)
- **Subnet**: Each VM gets a `/128` address

## Setup Instructions

### 1. Host Network Setup (One-time)

Run the setup script as root to configure the host networking:

```bash
sudo ./scripts/setup-tap-networking.sh
```

This script:
- ✅ Creates `vm1`, `vm2`, ..., `vm64` tap interfaces
- ✅ Configures IP forwarding for routing
- ✅ Sets up NAT for VM internet access  
- ✅ Assigns proper ownership to your user account
- ✅ Configures routes for each VM subnet

### 2. VM Configuration

VMs are automatically configured with:
- **Interface**: `vm{N}` (e.g., VM with index 3 uses `vm3`)
- **IP Address**: `10.0.0.{N}/32` (e.g., VM 3 gets `10.0.0.3`)
- **Gateway**: `10.0.0.0` (the host)
- **DNS**: Quad9 (9.9.9.9, 149.112.112.112)

### 3. Testing Connectivity

Once VMs are running:

```bash
# Check VM services
sudo systemctl status blixard-vm-*

# Test VM reachability
ping 10.0.0.1  # VM 1
ping 10.0.0.3  # VM 3

# SSH into VMs
ssh root@10.0.0.1  # VM 1 (password: empty)
ssh root@10.0.0.3  # VM 3

# Test VM-to-VM connectivity (from inside a VM)
ping 10.0.0.2  # From VM 1 to VM 2
```

## VM Index Assignment

Each VM gets a unique index that determines its network address:

| VM Index | Interface | IPv4 Address | IPv6 Address |
|----------|-----------|--------------|--------------|
| 1        | vm1       | 10.0.0.1     | fec0::1      |
| 2        | vm2       | 10.0.0.2     | fec0::2      |
| 3        | vm3       | 10.0.0.3     | fec0::3      |
| ...      | ...       | ...          | ...          |
| 64       | vm64      | 10.0.0.64    | fec0::40     |

**Important**: VM indices must be unique across the cluster to avoid IP conflicts.

## Networking Features

### ✅ Full Connectivity
- **Host ↔ VM**: Direct IP connectivity
- **VM ↔ VM**: Full mesh connectivity  
- **VM → Internet**: NAT through host
- **IPv4 + IPv6**: Dual-stack support

### ✅ Security & Isolation
- Each VM has isolated network namespace
- Host firewall rules control access
- No shared network segments
- Clean separation between VMs

### ✅ Performance
- Direct kernel routing (no bridge overhead)
- Minimal network stack
- Low latency VM-to-VM communication

## Troubleshooting

### Check Host Network Setup
```bash
# Verify tap interfaces exist
ip link show | grep vm

# Check routes to VMs
ip route show | grep "10.0.0"

# Test host-side connectivity
ping 10.0.0.1  # Should work if VM 1 is running
```

### Check VM Network Setup
```bash
# From inside a VM
ip addr show  # Should see 10.0.0.X address
ip route show  # Should see route to 10.0.0.0 gateway
ping 10.0.0.0  # Test gateway connectivity
```

### Reset Networking
```bash
# Clean up everything and start over
sudo ./scripts/setup-tap-networking.sh cleanup
sudo ./scripts/setup-tap-networking.sh
```

### Common Issues

1. **"Operation not permitted" errors**
   - Solution: Run the setup script as root
   - VMs now use system services with proper permissions

2. **Service start failures**
   - Check: `sudo systemctl status blixard-vm-<name>` for detailed error info
   - Check: Service files are in `/etc/systemd/system/`
   - Solution: `sudo systemctl daemon-reload` after changes

3. **No internet in VMs**  
   - Check: `sudo iptables -t nat -L` should show MASQUERADE rules
   - Check: `/proc/sys/net/ipv4/ip_forward` should be `1`

4. **Can't reach VMs from host**
   - Check: VM is actually running and has configured its IP
   - Check: `ip route show` has routes to VM subnets

5. **VM-to-VM connectivity fails**
   - Check: Both VMs have different indices (unique IPs)
   - Check: Both VMs can reach the gateway (10.0.0.0)

## Advanced Configuration

### Custom VM Index Range
Edit the setup script to create more interfaces:
```bash
sudo ./scripts/setup-tap-networking.sh $(whoami) 128  # Create 128 VM interfaces
```

### External Interface Selection
Specify the internet-facing interface:
```bash
sudo ./scripts/setup-tap-networking.sh $(whoami) 64 eth0  # Use eth0 for internet
```

### Firewall Rules
Add custom iptables rules for specific VM access control:
```bash
# Block VM 1 from accessing VM 2
sudo iptables -I FORWARD -s 10.0.0.1 -d 10.0.0.2 -j DROP

# Allow only SSH to VM 3 from outside
sudo iptables -I FORWARD -d 10.0.0.3 -p tcp --dport 22 -j ACCEPT
sudo iptables -I FORWARD -d 10.0.0.3 -j DROP
```

This routed networking setup provides a production-ready foundation for distributed microVM clusters with full connectivity and proper isolation.