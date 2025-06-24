#!/usr/bin/env bash
# Validation script for Blixard VM networking setup

set -euo pipefail

echo "🔍 Validating Blixard VM networking setup..."
echo

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Track overall status
ISSUES_FOUND=0

check_status() {
    if [ $1 -eq 0 ]; then
        echo -e "${GREEN}✓${NC} $2"
    else
        echo -e "${RED}✗${NC} $2"
        ((ISSUES_FOUND++))
    fi
}

warn_status() {
    echo -e "${YELLOW}⚠${NC} $1"
}

echo "1. Checking user group membership..."
groups | grep -q "kvm" && check_status 0 "User in kvm group" || check_status 1 "User NOT in kvm group"
groups | grep -q "blixard" && check_status 0 "User in blixard group" || check_status 1 "User NOT in blixard group"
echo

echo "2. Checking TUN device permissions..."
if [ -c /dev/net/tun ]; then
    check_status 0 "TUN device exists"
    TUN_PERMS=$(stat -c "%a" /dev/net/tun)
    if [[ "$TUN_PERMS" =~ ^66[0-6]$ ]]; then
        check_status 0 "TUN device has group write permissions ($TUN_PERMS)"
    else
        check_status 1 "TUN device permissions may be restrictive ($TUN_PERMS)"
    fi
else
    check_status 1 "TUN device does not exist"
fi
echo

echo "3. Checking kernel modules..."
lsmod | grep -q "^tun " && check_status 0 "TUN module loaded" || check_status 1 "TUN module not loaded"
lsmod | grep -q "vhost_net" && check_status 0 "vhost_net module loaded" || check_status 1 "vhost_net module not loaded"
echo

echo "4. Checking system networking configuration..."
if sysctl net.ipv4.ip_forward | grep -q "= 1"; then
    check_status 0 "IPv4 forwarding enabled"
else
    check_status 1 "IPv4 forwarding disabled"
fi

if sysctl net.ipv6.conf.all.forwarding | grep -q "= 1"; then
    check_status 0 "IPv6 forwarding enabled"
else
    check_status 1 "IPv6 forwarding disabled"
fi
echo

echo "5. Checking for existing tap interfaces..."
TAP_COUNT=$(ip link show | grep -c "^[0-9]*: vm[0-9]*:" || true)
if [ "$TAP_COUNT" -gt 0 ]; then
    check_status 0 "Found $TAP_COUNT tap interfaces"
    
    # Check a few specific interfaces for multi-queue
    for vm in vm1 vm12; do
        if ip link show "$vm" >/dev/null 2>&1; then
            FLAGS=$(cat "/sys/class/net/$vm/tun_flags" 2>/dev/null || echo "unknown")
            if [[ "$FLAGS" =~ ^0x[0-9a-fA-F]*[1-9a-fA-F][0-9a-fA-F]*$ ]] && ((0x$FLAGS & 0x0100)); then
                check_status 0 "$vm has multi-queue support (flags: $FLAGS)"
            else
                check_status 1 "$vm missing multi-queue support (flags: $FLAGS)"
            fi
        fi
    done
else
    warn_status "No tap interfaces found - this may be normal if using NixOS services"
fi
echo

echo "6. Checking NAT/firewall configuration..."
if iptables -t nat -L POSTROUTING | grep -q "MASQUERADE"; then
    check_status 0 "NAT/MASQUERADE rules found"
else
    check_status 1 "No NAT/MASQUERADE rules found"
fi

if iptables -L FORWARD | grep -q "10.0.0.0/24"; then
    check_status 0 "VM subnet forwarding rules found"
else
    warn_status "No specific VM forwarding rules found (may use default ACCEPT)"
fi
echo

echo "7. Checking systemd-networkd status..."
if systemctl is-active systemd-networkd >/dev/null 2>&1; then
    check_status 0 "systemd-networkd is active"
else
    check_status 1 "systemd-networkd is not active"
fi
echo

echo "8. Checking for Blixard-specific services..."
VM_SERVICES=$(systemctl --user list-units --all | grep -c "blixard-vm-" || true)
if [ "$VM_SERVICES" -gt 0 ]; then
    check_status 0 "Found $VM_SERVICES Blixard VM services"
else
    warn_status "No Blixard VM services found (normal if no VMs created yet)"
fi

# Check if NixOS auto-creation services exist
TAP_SERVICES=$(systemctl list-units --all | grep -c "create-vm.*-tap.service" || true)
if [ "$TAP_SERVICES" -gt 0 ]; then
    check_status 0 "Found $TAP_SERVICES NixOS tap creation services"
else
    warn_status "No NixOS tap creation services found - may need host config update"
fi
echo

echo "9. Testing basic networking tools..."
command -v ip >/dev/null && check_status 0 "iproute2 (ip command) available" || check_status 1 "iproute2 missing"
command -v iptables >/dev/null && check_status 0 "iptables available" || check_status 1 "iptables missing"
echo

echo "📋 SUMMARY:"
echo "=========="
if [ $ISSUES_FOUND -eq 0 ]; then
    echo -e "${GREEN}✓ All checks passed! VM networking should work correctly.${NC}"
    echo
    echo "To test VM creation:"
    echo "  cargo run -- vm start --name test-vm"
    echo "  systemctl --user status blixard-vm-test-vm"
else
    echo -e "${RED}✗ Found $ISSUES_FOUND issues that need attention.${NC}"
    echo
    echo "Common fixes:"
    echo "  1. Add user to groups: sudo usermod -a -G kvm,blixard \$USER"
    echo "  2. Update NixOS config with the changes in HOST_CONFIG_UPDATES.md"
    echo "  3. Rebuild system: sudo nixos-rebuild switch"
    echo "  4. Manual tap setup: sudo ./scripts/setup-tap-networking.sh"
fi
echo

if [ "$TAP_COUNT" -eq 0 ] && [ "$TAP_SERVICES" -eq 0 ]; then
    echo -e "${YELLOW}💡 TIP:${NC} Consider updating your NixOS configuration to automatically"
    echo "   create tap interfaces on boot. See nixos-host-config-updates.nix"
fi