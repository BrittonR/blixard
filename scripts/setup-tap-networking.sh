#!/bin/bash
# Setup script for routed microVM networking following microvm.nix documentation
# Based on: https://astro.github.io/microvm.nix/routed-network.html

set -euo pipefail

USER=${1:-$(whoami)}
NUM_VMS=${2:-64}
EXTERNAL_INTERFACE=${3:-$(ip route show default | awk '/default/ {print $5}' | head -n1)}

echo "Setting up routed microVM networking"
echo "User: $USER"
echo "Number of VMs: $NUM_VMS"
echo "External interface: $EXTERNAL_INTERFACE"

# Check if running as root
if [[ $EUID -ne 0 ]]; then
   echo "This script must be run as root (use sudo)"
   exit 1
fi

# Load tun module if not already loaded
if ! lsmod | grep -q "^tun "; then
    echo "Loading tun module..."
    modprobe tun
fi

# Enable IP forwarding
echo "Enabling IP forwarding..."
echo 'net.ipv4.ip_forward = 1' > /etc/sysctl.d/99-blixard-routing.conf
echo 'net.ipv6.conf.all.forwarding = 1' >> /etc/sysctl.d/99-blixard-routing.conf
sysctl -p /etc/sysctl.d/99-blixard-routing.conf

# Create tap interfaces for each VM
for i in $(seq 1 $NUM_VMS); do
    IFACE="vm$i"
    
    # Create tap interface if it doesn't exist
    if ! ip link show "$IFACE" >/dev/null 2>&1; then
        echo "Creating tap interface: $IFACE"
        ip tuntap add dev "$IFACE" mode tap user "$USER"
        ip link set dev "$IFACE" up
        
        # Configure host-side networking for this VM
        ip addr add "10.0.0.0/32" dev "$IFACE" 2>/dev/null || true
        ip addr add "fec0::/128" dev "$IFACE" 2>/dev/null || true
        
        # Add routes to the VM
        ip route add "10.0.0.$i/32" dev "$IFACE" 2>/dev/null || true
        ip route add "fec0::$(printf '%x' $i)/128" dev "$IFACE" 2>/dev/null || true
        
        echo "✓ Created $IFACE → 10.0.0.$i"
    else
        echo "✓ $IFACE already exists"
    fi
done

# Set up NAT for internet access
echo "Setting up NAT for VM internet access..."
iptables -t nat -A POSTROUTING -s 10.0.0.0/24 -o "$EXTERNAL_INTERFACE" -j MASQUERADE 2>/dev/null || true
iptables -A FORWARD -i vm+ -o "$EXTERNAL_INTERFACE" -j ACCEPT 2>/dev/null || true
iptables -A FORWARD -i "$EXTERNAL_INTERFACE" -o vm+ -m state --state RELATED,ESTABLISHED -j ACCEPT 2>/dev/null || true

# IPv6 NAT (if available)
if ip6tables --version >/dev/null 2>&1; then
    ip6tables -t nat -A POSTROUTING -s fec0::/64 -o "$EXTERNAL_INTERFACE" -j MASQUERADE 2>/dev/null || true
    ip6tables -A FORWARD -i vm+ -o "$EXTERNAL_INTERFACE" -j ACCEPT 2>/dev/null || true
    ip6tables -A FORWARD -i "$EXTERNAL_INTERFACE" -o vm+ -m state --state RELATED,ESTABLISHED -j ACCEPT 2>/dev/null || true
fi

echo ""
echo "✅ Routed microVM networking setup complete!"
echo ""
echo "VM Network Layout:"
echo "  Host: 10.0.0.0 (gateway)"
echo "  VMs:  10.0.0.1-10.0.0.$NUM_VMS"
echo ""
echo "Usage:"
echo "  VM 1: Uses vm1 interface → 10.0.0.1"
echo "  VM 3: Uses vm3 interface → 10.0.0.3"
echo "  VM N: Uses vmN interface → 10.0.0.N"
echo ""
echo "Test connectivity:"
echo "  ping 10.0.0.1  # Once VM 1 is running"
echo "  ssh root@10.0.0.3  # SSH to VM 3"
echo ""
echo "To remove all interfaces later:"
echo "  sudo $0 cleanup"

# Cleanup function
if [[ "${1:-}" == "cleanup" ]]; then
    echo "Cleaning up routed microVM networking..."
    
    # Remove iptables rules
    iptables -t nat -D POSTROUTING -s 10.0.0.0/24 -o "$EXTERNAL_INTERFACE" -j MASQUERADE 2>/dev/null || true
    iptables -D FORWARD -i vm+ -o "$EXTERNAL_INTERFACE" -j ACCEPT 2>/dev/null || true
    iptables -D FORWARD -i "$EXTERNAL_INTERFACE" -o vm+ -m state --state RELATED,ESTABLISHED -j ACCEPT 2>/dev/null || true
    
    if ip6tables --version >/dev/null 2>&1; then
        ip6tables -t nat -D POSTROUTING -s fec0::/64 -o "$EXTERNAL_INTERFACE" -j MASQUERADE 2>/dev/null || true
        ip6tables -D FORWARD -i vm+ -o "$EXTERNAL_INTERFACE" -j ACCEPT 2>/dev/null || true
        ip6tables -D FORWARD -i "$EXTERNAL_INTERFACE" -o vm+ -m state --state RELATED,ESTABLISHED -j ACCEPT 2>/dev/null || true
    fi
    
    # Remove tap interfaces
    for i in $(seq 1 $NUM_VMS); do
        IFACE="vm$i"
        if ip link show "$IFACE" >/dev/null 2>&1; then
            echo "Removing $IFACE"
            ip link delete "$IFACE"
        fi
    done
    
    # Remove sysctl config
    rm -f /etc/sysctl.d/99-blixard-routing.conf
    
    echo "Cleanup complete!"
fi