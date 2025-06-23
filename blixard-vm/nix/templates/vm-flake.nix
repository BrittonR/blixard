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
            };
            
            # Basic NixOS configuration
            services.getty.autologinUser = "root";
            users.users.root.password = "";
            
            systemd.services.init-command = {
              description = "Run initialization command";
              wantedBy = [ "multi-user.target" ];
              after = [ "network.target" ];
              serviceConfig = {
                Type = "oneshot";
                ExecStart = "${pkgs.coreutils}/bin/echo 'Blixard VM started successfully!'";
                RemainAfterExit = true;
              };
            };
            
            system.stateVersion = "23.11";
          }
        ];
      };
    };
}