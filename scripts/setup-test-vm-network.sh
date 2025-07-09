#!/bin/bash
# Setup network interfaces for Blixard VM testing
# This script creates tap interfaces required for microvm networking

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}=== Blixard Test VM Network Setup ===${NC}"

# Check if running as root
if [ "$EUID" -ne 0 ]; then 
    echo -e "${RED}Please run as root (use sudo)${NC}"
    exit 1
fi

# Function to create a tap interface
create_tap() {
    local name=$1
    local ip=$2
    
    # Delete if exists
    ip link delete "$name" 2>/dev/null || true
    
    # Create tap interface with multi-queue support
    echo -e "${YELLOW}Creating tap interface: $name${NC}"
    ip tuntap add dev "$name" mode tap group kvm multi_queue
    ip link set dev "$name" up
    
    # Add host IP
    ip addr add "${ip}/32" dev "$name"
    
    # Add route for VM
    ip route add "10.0.0.${name#vm}/32" dev "$name"
    
    echo -e "${GREEN}✓ Created $name with route to 10.0.0.${name#vm}${NC}"
}

# Create test VM interfaces
echo -e "\n${YELLOW}Creating test VM interfaces...${NC}"

# For test-vm-1 through test-vm-10
for i in {1..10}; do
    create_tap "test-vm-$i" "10.0.0.0"
done

# For test-vm-net (specific test)
create_tap "test-vm-net" "10.0.0.0"

# Enable IP forwarding
echo -e "\n${YELLOW}Enabling IP forwarding...${NC}"
sysctl -w net.ipv4.ip_forward=1
sysctl -w net.ipv6.conf.all.forwarding=1

# Setup NAT for outbound traffic
echo -e "\n${YELLOW}Setting up NAT...${NC}"
iptables -t nat -A POSTROUTING -s 10.0.0.0/24 ! -d 10.0.0.0/24 -j MASQUERADE
iptables -A FORWARD -s 10.0.0.0/24 -j ACCEPT
iptables -A FORWARD -d 10.0.0.0/24 -j ACCEPT

# Verify setup
echo -e "\n${GREEN}=== Network Setup Complete ===${NC}"
echo -e "\nCreated interfaces:"
ip link show | grep -E "test-vm-[0-9]+|test-vm-net" | awk '{print "  - " $2}'

echo -e "\nRoutes to VMs:"
ip route | grep "10.0.0." | grep "test-vm" | awk '{print "  - " $0}'

echo -e "\n${YELLOW}Note: Users must be in the 'kvm' group to use these interfaces${NC}"
echo -e "Run: ${GREEN}sudo usermod -a -G kvm \$USER${NC} and log out/in if needed\n"