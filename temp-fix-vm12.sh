#!/usr/bin/env bash
# Temporary fix for vm12 interface permissions using kvm group

set -euo pipefail

echo "Fixing vm12 interface for kvm group access..."

# Delete and recreate with kvm group
sudo ip link delete vm12 2>/dev/null || true
sudo ip tuntap add dev vm12 mode tap group kvm
sudo ip link set dev vm12 up

# Configure host networking
sudo ip addr add "10.0.0.0/32" dev vm12 2>/dev/null || true
sudo ip route add "10.0.0.12/32" dev vm12 2>/dev/null || true

echo "✓ vm12 recreated with kvm group ownership"
echo "Interface status:"
ip link show vm12