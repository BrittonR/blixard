#!/bin/bash
# Setup script for tap networking with user permissions
# This script sets up tap interfaces that can be used by regular users

set -euo pipefail

USER=${1:-$(whoami)}
NUM_INTERFACES=${2:-10}

echo "Setting up tap networking for user: $USER"
echo "Creating $NUM_INTERFACES tap interfaces..."

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

# Create tap interfaces
for i in $(seq 1 $NUM_INTERFACES); do
    IFACE="blixard-tap$i"
    
    # Create tap interface if it doesn't exist
    if ! ip link show "$IFACE" >/dev/null 2>&1; then
        echo "Creating tap interface: $IFACE"
        ip tuntap add dev "$IFACE" mode tap user "$USER"
        ip link set dev "$IFACE" up
        
        # Optional: Add to a bridge (uncomment if you want bridge networking)
        # BRIDGE="blixard-br0"
        # if ! ip link show "$BRIDGE" >/dev/null 2>&1; then
        #     ip link add name "$BRIDGE" type bridge
        #     ip link set dev "$BRIDGE" up
        # fi
        # ip link set dev "$IFACE" master "$BRIDGE"
        
        echo "✓ Created $IFACE (owned by $USER)"
    else
        echo "✓ $IFACE already exists"
    fi
done

echo "Tap networking setup complete!"
echo ""
echo "To use with blixard VMs:"
echo "1. Update VM configs to use pre-created interfaces"
echo "2. Assign static IPs in the 10.0.0.x range"
echo ""
echo "To remove all interfaces later:"
echo "  sudo $0 cleanup"

# Cleanup function
if [[ "${1:-}" == "cleanup" ]]; then
    echo "Cleaning up tap interfaces..."
    for i in $(seq 1 $NUM_INTERFACES); do
        IFACE="blixard-tap$i"
        if ip link show "$IFACE" >/dev/null 2>&1; then
            echo "Removing $IFACE"
            ip link delete "$IFACE"
        fi
    done
    echo "Cleanup complete!"
fi