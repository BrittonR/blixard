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
              
              interfaces = [
                {% if networks -%}
                {% for network in networks -%}
                {
                  type = "macvtap";
                  id = "{{ network.id }}";
                  mac = "{{ network.mac }}";
                  macvtap = {
                    link = "br0";
                    mode = "bridge";
                  };
                }
                {% endfor -%}
                {% else -%}
                {
                  type = "macvtap";
                  id = "vm-default";
                  mac = "02:00:00:00:00:01";
                  macvtap = {
                    link = "br0";
                    mode = "bridge";
                  };
                }
                {% endif -%}
              ];
              
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
            
            # Static IP networking configuration for routed network
            networking.useDHCP = false;
            networking.firewall.enable = false;
            
            # Configure static IP based on allocated address
            {% if networks -%}
            {% for network in networks -%}
            {% if network.type == "routed" -%}
            networking.interfaces.eth0 = {
              ipv4.addresses = [
                {
                  address = "{{ network.ip }}";
                  prefixLength = 24;
                }
              ];
            };
            networking.defaultGateway = "{{ network.gateway }}";
            networking.nameservers = [ "8.8.8.8" "1.1.1.1" ];
            {% endif -%}
            {% endfor -%}
            {% else -%}
            # Fallback to DHCP if no network config
            networking.interfaces.eth0.useDHCP = true;
            {% endif -%}
            
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
                {% if networks -%}
                {% for network in networks -%}
                {% if network.type == "routed" -%}
                ExecStart = "${pkgs.coreutils}/bin/echo 'Blixard VM started successfully! SSH: {{ network.ip }}:22'";
                {% endif -%}
                {% endfor -%}
                {% else -%}
                ExecStart = "${pkgs.coreutils}/bin/echo 'Blixard VM started successfully!'";
                {% endif -%}
                RemainAfterExit = true;
              };
            };
            
            system.stateVersion = "23.11";
          }
        ];
      };
    };
}