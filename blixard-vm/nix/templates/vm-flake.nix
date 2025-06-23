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
                {
                  type = "user";
                  id = "user0";
                  mac = "02:00:00:00:00:01";
                }
              ];
              
              volumes = [
                {
                  image = "rootdisk.img";
                  mountPoint = "/";
                  size = 10240;
                }
              ];
              
              # Enable SSH port forwarding  
              forwardPorts = [
                {
                  from = "host";
                  host.port = 2222;
                  guest.port = 22;
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
            
            # Enable networking
            networking.useDHCP = false;
            networking.interfaces.eth0.useDHCP = true;
            networking.firewall.enable = false;
            
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
                ExecStart = "${pkgs.coreutils}/bin/echo 'Blixard VM started successfully! SSH: localhost:2222'";
                RemainAfterExit = true;
              };
            };
            
            system.stateVersion = "23.11";
          }
        ];
      };
    };
}