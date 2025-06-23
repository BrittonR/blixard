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
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      nixosConfigurations."test-vm" = nixpkgs.lib.nixosSystem {
        inherit system;
        modules = [
          microvm.nixosModules.microvm
          {
            networking.hostName = "test-vm";
            
            microvm = {
              hypervisor = "qemu";
              vcpu = 1;
              mem = 512;
              
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
                  size = 2048;
                }
              ];
              
              # SSH port forwarding
              forwardPorts = [
                {
                  from = "host";
                  host.port = 2223;
                  guest.port = 22;
                }
              ];
            };
            
            # Enable SSH with password authentication
            services.openssh = {
              enable = true;
              settings = {
                PermitRootLogin = "yes";
                PasswordAuthentication = true;
                PermitEmptyPasswords = true;
              };
            };
            
            # Root user with no password
            users.users.root = {
              password = "";
            };
            
            # Disable firewall for testing
            networking.firewall.enable = false;
            
            # Auto-login on console
            services.getty.autologinUser = "root";
            
            system.stateVersion = "23.11";
          }
        ];
      };
    };
}