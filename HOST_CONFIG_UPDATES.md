# NixOS Host Configuration Updates for Blixard VM Networking

## Key Changes Required

Based on our VM networking debugging session, here are the essential updates needed for your NixOS host configuration:

### 1. **Automatic Tap Interface Creation with Multi-Queue Support**

**Problem**: Manual tap interface creation was required with `multi_queue` flag.

**Solution**: Add systemd services to automatically create tap interfaces on boot:

```nix
systemd.services = builtins.listToAttrs (
  map (index: {
    name = "create-vm${toString index}-tap";
    value = {
      description = "Create vm${toString index} tap interface with multi-queue support";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        ExecStart = pkgs.writeShellScript "create-vm${toString index}-tap" ''
          # Create with multi-queue support and kvm group ownership
          ${pkgs.iproute2}/bin/ip tuntap add dev vm${toString index} mode tap group kvm multi_queue
          # ... rest of setup
        '';
      };
    };
  }) (lib.genList (i: i + 1) maxVMs)
);
```

### 2. **TUN/TAP Device Permissions**

**Problem**: TUN device needs proper group permissions for kvm group access.

**Solution**: Add udev rule to ensure correct permissions:

```nix
services.udev.extraRules = ''
  KERNEL=="tun", GROUP="kvm", MODE="0660"
'';
```

### 3. **Systemd-Networkd Configuration Updates**

**Problem**: Tap interfaces need special handling (no DHCP, carrier-less operation).

**Solution**: Enhanced network configuration:

```nix
systemd.network.networks = builtins.listToAttrs (
  map (index: {
    name = "30-vm${toString index}";
    value = {
      matchConfig.Name = "vm${toString index}";
      networkConfig = {
        IPv4Forwarding = true;
        IPv6Forwarding = true;
        # Important: Keep interface up without carrier
        ConfigureWithoutCarrier = true;
      };
      # Disable DHCP on tap interfaces
      dhcpV4Config.Enable = false;
      dhcpV6Config.Enable = false;
    };
  }) (lib.genList (i: i + 1) maxVMs)
);
```

### 4. **Kernel Modules and Optimizations**

**Problem**: Missing kernel modules and suboptimal network settings.

**Solution**: Add required modules and optimizations:

```nix
boot.kernelModules = [ "tun" "tap" "vhost_net" "vhost" ];

boot.kernel.sysctl = {
  "net.ipv4.ip_forward" = 1;
  "net.ipv6.conf.all.forwarding" = 1;
  # Network buffer optimizations for VM performance
  "net.core.rmem_max" = 134217728;
  "net.core.wmem_max" = 134217728;
};
```

### 5. **Firewall Rules for VM Traffic**

**Problem**: Default firewall may block VM-to-VM and VM-to-internet traffic.

**Solution**: Add explicit firewall rules:

```nix
networking.firewall.extraCommands = ''
  # Allow traffic from VMs to internet
  iptables -A FORWARD -s 10.0.0.0/24 -j ACCEPT
  iptables -A FORWARD -d 10.0.0.0/24 -j ACCEPT
  
  # Allow VM-to-VM communication
  iptables -A FORWARD -s 10.0.0.0/24 -d 10.0.0.0/24 -j ACCEPT
'';
```

### 6. **Helper Commands**

**Problem**: Manual setup was tedious during debugging.

**Solution**: Add shell aliases for easy management:

```nix
environment.shellAliases = {
  blixard-setup-taps = "...script to create all tap interfaces...";
  blixard-cleanup-taps = "...script to remove all tap interfaces...";
};
```

## Implementation Steps

1. **Replace your current configuration** with the updated version in `nixos-host-config-updates.nix`

2. **Rebuild your system**:
   ```bash
   sudo nixos-rebuild switch
   ```

3. **Verify tap interfaces are created**:
   ```bash
   ip link show | grep vm
   cat /sys/class/net/vm1/tun_flags  # Should show multi-queue flags
   ```

4. **Test VM startup**:
   ```bash
   cargo run -- vm start --name test-vm
   systemctl --user status blixard-vm-test-vm
   ```

5. **Verify networking**:
   ```bash
   ping 10.0.0.12
   ssh root@10.0.0.12
   ```

## Critical Changes Summary

| Issue | Root Cause | NixOS Solution |
|-------|------------|----------------|
| Multi-queue tap creation | Manual `ip tuntap add` with `multi_queue` | Automated systemd services |
| TUN device permissions | Default 600 permissions | udev rules for kvm group |
| Interface configuration | Missing `ConfigureWithoutCarrier` | Enhanced systemd.network config |
| Network forwarding | Manual sysctl settings | Declarative kernel.sysctl |
| VM traffic blocking | Default firewall rules | Custom iptables rules |

These changes make the VM networking setup **fully declarative and automatic**, eliminating the manual setup steps we had to perform during debugging.