
            {
            system, microvm, nixpkgs }:

            {
              nixpkgs.hostPlatform = system;
              environment.noXlibs = false;
              microvm.hypervisor = "cloud-hypervisor";

              users.users.root.password = "p";

              sops.secrets."ssh_public_key" = {
                sopsFile = "${inputs.self}/admin_ssh_key.enc.yaml";
              };

              services.openssh = {
                enable = true;
                settings.PermitRootLogin = "yes";
              };

              users.groups.admin = {};
              users.users.admin = {
                isNormalUser = true;
                group = "admin";
                extraGroups = ["wheel" "admin" "sudo"];
                password = "test";
                home = "/home/admin";
                createHome = true;
                openssh.authorizedKeys.keys = ["${config.sops.secrets."ssh_public_key".value}"];
              };

              security.sudo.extraRules = [{
                groups = [ "admin" ];
                commands = [{
                  command = "ALL";
                  options = [ "NOPASSWD" ];
                }];
              }];

              networking.useNetworkd = false;
              networking.interfaces.eth0.useDHCP = false;
              networking.defaultGateway = "10.0.0.1";
              networking.nameservers = ["1.1.1.1", "8.8.8.8"];

              microvm.writableStoreOverlay = "/nix/.rwstore";

              microvm.volumes = [ {
                image = "nix-store-overlay.img";
                mountPoint = config.microvm.writableStoreOverlay;
                size = 2048;
              } ];
            }
            