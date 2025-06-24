#!/usr/bin/env bash
# Script to fix tap interface permissions for blixard group

set -euo pipefail

echo "Fixing tap interface permissions for blixard group..."

# Recreate vm12 with proper group ownership
echo "Recreating vm12 with blixard group ownership..."
ip link delete vm12 2>/dev/null || true
ip tuntap add dev vm12 mode tap group blixard
ip link set dev vm12 up

# Configure host-side networking
ip addr add "10.0.0.0/32" dev vm12 2>/dev/null || true
ip addr add "fec0::/128" dev vm12 2>/dev/null || true

# Add routes to the VM
ip route add "10.0.0.12/32" dev vm12 2>/dev/null || true
ip route add "fec0::c/128" dev vm12 2>/dev/null || true

echo "✓ vm12 recreated with blixard group ownership"
echo "✓ Host-side networking configured"

# Verify ownership
echo "Interface details:"
ls -la /sys/class/net/vm12/tun_flags 2>/dev/null || echo "No tun_flags (expected for tap interfaces)"
cat /sys/class/net/vm12/owner 2>/dev/null || echo "No owner file"

echo "✓ Tap interface vm12 is ready for blixard group access"