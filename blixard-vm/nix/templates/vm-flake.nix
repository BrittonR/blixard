{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    microvm = {
      url = "github:astro/microvm.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, microvm }:
    let
      system = "{{ system }}";
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      nixosConfigurations."{{ vm_name }}" = nixpkgs.lib.nixosSystem {
        inherit system;
        modules = [
          microvm.nixosModules.microvm
          {
            networking.hostName = "{{ vm_name }}";
            
            microvm = {
              hypervisor = "{{ hypervisor }}";
              vcpu = {{ vcpus }};
              mem = {{ memory }};
              
              interfaces = [ {
                type = "tap";
                id = "blixard-tap{{ vm_index }}";
                mac = "{{ vm_mac }}";
              } ];
              
              volumes = [
                {
                  image = "rootdisk.img";
                  mountPoint = "/";
                  size = 10240;
                }
              ];
              
              # Console configuration using microvm.nix socket pattern
              socket = "/tmp/{{ vm_name }}-console.sock";
            };
            
            # Basic NixOS configuration
            services.getty.autologinUser = "root";
            users.users.root = {
              password = "";
              openssh.authorizedKeys.keys = [
                # Add your SSH public key here for key-based auth
                # "ssh-rsa AAAAB3NzaC1yc2E... your-email@example.com"
              ];
            };
            
            # Enable SSH
            services.openssh = {
              enable = true;
              settings = {
                PermitRootLogin = "yes";
                PasswordAuthentication = true;
                PermitEmptyPasswords = true;
              };
            };
            
            # Enable serial console
            systemd.services."serial-getty@ttyS0" = {
              enable = true;
              wantedBy = [ "getty.target" ];
            };
            
            # Network configuration using systemd-networkd
            networking.useNetworkd = true;
            networking.firewall.enable = false;
            
            systemd.network.networks."10-eth" = {
              matchConfig.MACAddress = "{{ vm_mac }}";
              # Static IP configuration based on VM index
              address = [
                "10.0.0.{{ vm_index }}/32"
                "fec0::{{ vm_index_hex }}/128"
              ];
              routes = [
                {
                  # A route to the host
                  Destination = "10.0.0.0/32";
                  GatewayOnLink = true;
                }
                {
                  # Default route
                  Destination = "0.0.0.0/0";
                  Gateway = "10.0.0.0";
                  GatewayOnLink = true;
                }
                {
                  # IPv6 default route
                  Destination = "::/0";
                  Gateway = "fec0::";
                  GatewayOnLink = true;
                }
              ];
              networkConfig = {
                # DNS servers
                DNS = [
                  # Quad9.net
                  "9.9.9.9"
                  "149.112.112.112"
                  "2620:fe::fe"
                  "2620:fe::9"
                ];
              };
            };
            
            # Configure kernel for serial console
            boot.kernelParams = [ 
              "console=ttyS0,115200"
              "console=tty0"
            ];
            
            systemd.services.init-command = {
              description = "Run initialization command";
              wantedBy = [ "multi-user.target" ];
              after = [ "network.target" ];
              serviceConfig = {
                Type = "oneshot";
                ExecStart = "${pkgs.coreutils}/bin/echo 'Blixard VM {{ vm_name }} started successfully! SSH: 10.0.0.{{ vm_index }}:22'";
                RemainAfterExit = true;
              };
            };
            
            system.stateVersion = "23.11";
          }
        ];
      };
    };
}