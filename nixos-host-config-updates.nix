{ inputs, config, pkgs, lib, ... }:
let
  inherit (inputs) clan-core;
  maxVMs = 64;
in
{
  imports = [
    clan-core.clanModules.sshd
    clan-core.clanModules.root-password
    clan-core.clanModules.state-version
    inputs.microvm.nixosModules.host
  ];

  # Use systemd-networkd for routed networking
  networking.useNetworkd = true;

  # Allow blixard group full systemctl access (like docker group)
  security.sudo.extraRules = [
    {
      groups = [ "blixard" ];
      commands = [
        {
          command = "ALL";
          options = [ "NOPASSWD" ];
        }
      ];
    }
  ];

  # Configure individual tap interfaces for each VM with multi-queue support
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
            # Delete existing interface if it exists
            ${pkgs.iproute2}/bin/ip link delete vm${toString index} 2>/dev/null || true
            
            # Create tap interface with multi-queue support and kvm group ownership
            ${pkgs.iproute2}/bin/ip tuntap add dev vm${toString index} mode tap group kvm multi_queue
            
            # Bring interface up
            ${pkgs.iproute2}/bin/ip link set dev vm${toString index} up
            
            # Add host address (gateway for the VM)
            ${pkgs.iproute2}/bin/ip addr add "10.0.0.0/32" dev vm${toString index} 2>/dev/null || true
            
            # Add route to VM
            ${pkgs.iproute2}/bin/ip route add "10.0.0.${toString index}/32" dev vm${toString index} 2>/dev/null || true
          '';
          ExecStop = pkgs.writeShellScript "destroy-vm${toString index}-tap" ''
            ${pkgs.iproute2}/bin/ip link delete vm${toString index} 2>/dev/null || true
          '';
        };
      };
    }) (lib.genList (i: i + 1) maxVMs)
  );

  # Configure systemd-networkd for the tap interfaces
  systemd.network.networks = builtins.listToAttrs (
    map (index: {
      name = "30-vm${toString index}";
      value = {
        matchConfig.Name = "vm${toString index}";
        # Host's addresses (gateway for VMs)
        address = [
          "10.0.0.0/32"
          "fec0::/128"
        ];
        # Setup routes to the VM
        routes = [ {
          Destination = "10.0.0.${toString index}/32";
        } {
          Destination = "fec0::${lib.toHexString index}/128";
        } ];
        # Enable routing and configure interface
        networkConfig = {
          IPv4Forwarding = true;
          IPv6Forwarding = true;
          # Keep interface up even without carrier (important for tap)
          ConfigureWithoutCarrier = true;
        };
        # Don't try to get DHCP on tap interfaces
        dhcpV4Config.Enable = false;
        dhcpV6Config.Enable = false;
      };
    }) (lib.genList (i: i + 1) maxVMs)
  );

  # NAT configuration for VM internet access
  networking.nat = {
    enable = true;
    internalIPs = [ "10.0.0.0/24" ];
    # Change this to your external interface
    externalInterface = "enp10s0";  # Adjust to your interface
  };

  # Firewall configuration to allow VM traffic
  networking.firewall = {
    # Allow traffic between host and VMs
    extraCommands = ''
      # Allow traffic from VMs to internet (MASQUERADE should handle this via NAT)
      iptables -A FORWARD -s 10.0.0.0/24 -j ACCEPT
      iptables -A FORWARD -d 10.0.0.0/24 -j ACCEPT
      
      # Allow VM-to-VM communication
      iptables -A FORWARD -s 10.0.0.0/24 -d 10.0.0.0/24 -j ACCEPT
    '';
  };

  # Required kernel modules and packages
  boot.kernelModules = [ "tun" "tap" "vhost_net" "vhost" ];
  
  # Ensure TUN/TAP device has proper permissions
  services.udev.extraRules = ''
    KERNEL=="tun", GROUP="kvm", MODE="0660"
  '';

  # Required packages
  environment.systemPackages = with pkgs; [ 
    iproute2 
    iptables 
    # Useful for debugging networking
    tcpdump
    netcat
    socat
  ];

  # User and group configuration
  users.groups.blixard = {};
  users.users.brittonr.extraGroups = [ "blixard" "kvm" "libvirtd" ];
  
  # Enable virtualization support
  virtualisation = {
    libvirtd.enable = true;
    # Allow building VMs
    vmware.guest.enable = false;  # Disable if not needed
  };

  # System-level networking optimizations for VM performance
  boot.kernel.sysctl = {
    # Enable IP forwarding
    "net.ipv4.ip_forward" = 1;
    "net.ipv6.conf.all.forwarding" = 1;
    
    # Optimize for tap interfaces
    "net.core.rmem_max" = 134217728;
    "net.core.wmem_max" = 134217728;
    "net.ipv4.tcp_rmem" = "4096 65536 134217728";
    "net.ipv4.tcp_wmem" = "4096 65536 134217728";
    
    # Allow VMs to use more ports
    "net.ipv4.ip_local_port_range" = "1024 65535";
  };

  # Additional systemd configuration for user services
  systemd.user.services = {
    # Example: Enable lingering for blixard user services
    # This ensures user systemd services can run without user login
  };

  # Optional: Add a helper script for manual tap interface management
  environment.shellAliases = {
    blixard-setup-taps = "${pkgs.writeShellScript "setup-taps" ''
      echo "Setting up tap interfaces for Blixard VMs..."
      for i in {1..${toString maxVMs}}; do
        echo "Creating vm$i interface..."
        sudo ip link delete vm$i 2>/dev/null || true
        sudo ip tuntap add dev vm$i mode tap group kvm multi_queue
        sudo ip link set dev vm$i up
        sudo ip addr add "10.0.0.0/32" dev vm$i 2>/dev/null || true
        sudo ip route add "10.0.0.$i/32" dev vm$i 2>/dev/null || true
      done
      echo "Tap interfaces created successfully!"
    ''}";
    
    blixard-cleanup-taps = "${pkgs.writeShellScript "cleanup-taps" ''
      echo "Cleaning up tap interfaces..."
      for i in {1..${toString maxVMs}}; do
        sudo ip link delete vm$i 2>/dev/null || true
      done
      echo "Tap interfaces cleaned up!"
    ''}";
  };
}