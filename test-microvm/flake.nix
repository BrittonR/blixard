{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    microvm = {
      url = "github:astro/microvm.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
  };

  outputs = inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        inputs.microvm.flakeModule
      ];
      
      systems = [ "x86_64-linux" ];
      
      perSystem = { config, pkgs, ... }: {
        microvm.vms = {
          test-vm = {
            inherit pkgs;
            config = {
              networking.hostName = "test-vm";
              
              microvm = {
                hypervisor = "qemu";
                vcpu = 2;
                mem = 1024;
                
                interfaces = [{
                  type = "user";
                  id = "user0";
                  mac = "02:00:00:00:00:01";
                }];
                
                volumes = [{
                  image = "rootdisk.img";
                  mountPoint = "/";
                  size = 2048;
                }];
              };
              
              # Basic NixOS configuration
              services.getty.autologinUser = "root";
              users.users.root.password = "";
              
              systemd.services.test-service = {
                description = "Test service to show VM is working";
                wantedBy = [ "multi-user.target" ];
                serviceConfig = {
                  Type = "oneshot";
                  ExecStart = "${pkgs.coreutils}/bin/echo 'Blixard microVM is running successfully!'";
                  RemainAfterExit = true;
                };
              };
              
              system.stateVersion = "23.11";
            };
          };
        };
      };
    };
}