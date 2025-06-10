# Installation and Setup Guide

This guide covers installation and setup procedures for Blixard on NixOS systems. Blixard is designed exclusively for NixOS to leverage its declarative configuration and reliable system management.

## Prerequisites

- NixOS 23.11 or later
- At least 8GB RAM (16GB recommended for production)
- 50GB+ available disk space
- Network connectivity between cluster nodes

## Single-Node Setup

For development or small deployments, you can run Blixard on a single node.

### NixOS Configuration

Add the following to your `/etc/nixos/configuration.nix`:

```nix
{ config, pkgs, ... }: {
  # Enable Blixard
  services.blixard = {
    enable = true;
    mode = "single-node";
    storage.ceph = {
      enable = true;
      osds = [ "/dev/sdb" ];
    };
  };
  
  # Enable required virtualization
  virtualisation = {
    libvirtd.enable = true;
    kvmgt.enable = true;
  };
  
  # Enable microvm support
  microvm.host.enable = true;
  
  # Install Blixard CLI
  environment.systemPackages = with pkgs; [
    blixard
  ];
}
```

### Apply Configuration

```bash
# Rebuild NixOS with new configuration
sudo nixos-rebuild switch

# Verify Blixard is running
systemctl status blixard

# Start managing VMs with microvm.nix
blixard vm create nginx --config examples/nginx.nix --storage-size 10G
blixard vm start nginx
```

## Multi-Node Cluster Setup

For production deployments, run Blixard across multiple nodes for high availability.

### Primary Node Configuration

Configure the first node as the cluster primary in `/etc/nixos/configuration.nix`:

```nix
{ config, pkgs, ... }: {
  services.blixard = {
    enable = true;
    mode = "cluster-primary";
    nodeId = "nixos-node1";
    storage.ceph = {
      enable = true;
      role = [ "mon" "osd" "mgr" ];
      osds = [ "/dev/nvme0n1" ];
    };
  };
  
  networking.hostName = "nixos-node1";
  virtualisation.microvm.host.enable = true;
}
```

### Additional Node Configuration

Configure additional nodes to join the cluster:

```nix
{ config, pkgs, ... }: {
  services.blixard = {
    enable = true;
    mode = "cluster-join";
    nodeId = "nixos-node2";
    primaryNode = "nixos-node1.local";
    storage.ceph = {
      enable = true;
      role = [ "osd" "mds" ];
      osds = [ "/dev/nvme0n1" ];
    };
  };
  
  networking.hostName = "nixos-node2";
  virtualisation.microvm.host.enable = true;
}
```

### Deploy Cluster

```bash
# Deploy configuration on all nodes
sudo nixos-rebuild switch

# Verify cluster formation
blixard cluster status
blixard storage status

# Create storage pool
blixard storage pool create ssd-pool --type replicated --size 3
```

## Production Deployment

For production environments, use enhanced configuration with monitoring and security features.

### Production Configuration

```nix
# /etc/nixos/configuration.nix (Production)
{ config, pkgs, ... }: {
  services.blixard = {
    enable = true;
    mode = "production";
    
    cluster = {
      discovery.method = "tailscale";
      discovery.tailscale.authkey = "$TAILSCALE_KEY";
    };
    
    storage.ceph = {
      enable = true;
      replicationFactor = 3;
      pgNum = 128;
      defaultPool = "ssd-pool";
    };
    
    monitoring = {
      prometheus = {
        enable = true;
        port = 9090;
      };
      ceph.dashboard = true;
      grafana.enable = true;
    };
  };
  
  # NixOS security hardening
  security.polkit.enable = true;
  virtualisation.libvirtd.qemu.swtpm.enable = true;
  
  # Automatic updates for security
  system.autoUpgrade = {
    enable = true;
    channel = "nixos-23.11";
  };
}
```

### Deployment Options

Deploy your production configuration using one of these methods:

1. **Manual Deployment**: `sudo nixos-rebuild switch` on each node
2. **NixOps**: Declarative multi-node deployment
3. **deploy-rs**: Modern deployment tool for NixOS
4. **Terraform**: Using the NixOS provider

## Verification Steps

After installation, verify your deployment:

```bash
# Check cluster health
blixard cluster status

# Verify storage is operational
blixard storage status

# Test VM creation
blixard vm create test-vm --config examples/minimal.nix
blixard vm start test-vm
blixard vm status test-vm

# Check monitoring endpoints
curl http://localhost:9090/metrics  # Prometheus
```

## Troubleshooting

### Common Issues

1. **Storage OSD Not Starting**
   - Verify disk device exists: `lsblk`
   - Check disk is not mounted: `mount | grep /dev/sdb`
   - Review logs: `journalctl -u ceph-osd@0`

2. **Cluster Nodes Not Discovering**
   - Check network connectivity: `ping nixos-node2`
   - Verify Tailscale status: `tailscale status`
   - Check firewall rules: `sudo iptables -L`

3. **VM Creation Fails**
   - Verify microvm.nix is installed: `nix-env -q microvm`
   - Check virtualization support: `lscpu | grep Virtualization`
   - Review Blixard logs: `journalctl -u blixard`

### Getting Help

- Check logs: `journalctl -u blixard -f`
- Enable debug logging: Set `log_level = "debug"` in config
- Community support: GitHub issues and discussions
- Enterprise support: Contact for production deployments

## Next Steps

- [Configuration Guide](configuration.md) - Detailed configuration options
- [Monitoring Setup](monitoring.md) - Configure observability
- [Operations Guide](../administrator-guide/production-deployment.md) - Production best practices