{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.microvm.url = "github:astro/microvm.nix";
  inputs.microvm.inputs.nixpkgs.follows = "nixpkgs";

  outputs = { self, nixpkgs, microvm }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      nixosConfigurations.test-vm = nixpkgs.lib.nixosSystem {
        inherit system;
        modules = [
          microvm.nixosModules.microvm
          {
            networking.hostName = "test-vm";
            users.users.root.password = "";
            
            microvm = {
              hypervisor = "qemu";
              vcpu = 1;
              mem = 256;
              
              # Use user networking for simplicity
              interfaces = [{
                type = "user";
                id = "eth0";
                mac = "02:00:00:00:00:01";
              }];
              
              # Minimal volumes
              volumes = [{
                image = "/tmp/test-vm.img";
                mountPoint = "/";
                size = 512;
              }];
              
              # Graphics off for headless
              graphics.enable = false;
            };
            
            # Basic services
            services.getty.autologinUser = "root";
            
            # Minimal packages
            environment.systemPackages = with pkgs; [ 
              busybox 
            ];
            
            system.stateVersion = "24.05";
          }
        ];
      };
    };
}
