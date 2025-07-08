# Blixard VM flake-parts module
{ self, lib, flake-parts-lib, ... }:
let
  inherit (lib) mkOption types;
  inherit (flake-parts-lib) mkPerSystemOption;
in
{
  options = {
    flake = mkOption {
      type = types.submodule {
        options = {
          nixosModules = mkOption {
            type = types.attrsOf types.anything;
            default = { };
            description = "NixOS modules exported by blixard";
          };
        };
      };
    };

    perSystem = mkPerSystemOption ({ config, self', inputs', pkgs, system, ... }: {
      options = {
        blixardVMs = mkOption {
          type = types.attrsOf (types.submodule {
            options = {
              hypervisor = mkOption {
                type = types.enum [ "cloud-hypervisor" "firecracker" "qemu" ];
                default = "cloud-hypervisor";
                description = "Hypervisor backend to use";
              };

              vcpus = mkOption {
                type = types.int;
                default = 2;
                description = "Number of virtual CPUs";
              };

              memory = mkOption {
                type = types.int;
                default = 1024;
                description = "Memory in MB";
              };

              networking = mkOption {
                type = types.submodule {
                  options = {
                    type = mkOption {
                      type = types.enum [ "tap" "user" "none" ];
                      default = "tap";
                      description = "Network type";
                    };

                    tapInterface = mkOption {
                      type = types.nullOr types.str;
                      default = null;
                      description = "TAP interface name";
                    };

                    macAddress = mkOption {
                      type = types.nullOr types.str;
                      default = null;
                      description = "MAC address";
                    };

                    ipAddress = mkOption {
                      type = types.nullOr types.str;
                      default = null;
                      description = "IP address";
                    };
                  };
                };
                default = { };
                description = "Network configuration";
              };

              volumes = mkOption {
                type = types.listOf (types.submodule {
                  options = {
                    type = mkOption {
                      type = types.enum [ "root" "data" "share" ];
                      description = "Volume type";
                    };

                    size = mkOption {
                      type = types.nullOr types.int;
                      default = null;
                      description = "Size in MB (for root/data volumes)";
                    };

                    path = mkOption {
                      type = types.nullOr types.str;
                      default = null;
                      description = "Path to volume file or share directory";
                    };

                    mountPoint = mkOption {
                      type = types.str;
                      default = "/";
                      description = "Mount point in the VM";
                    };

                    readOnly = mkOption {
                      type = types.bool;
                      default = false;
                      description = "Whether the volume is read-only";
                    };
                  };
                });
                default = [
                  {
                    type = "root";
                    size = 10240;
                    mountPoint = "/";
                  }
                ];
                description = "Storage volumes";
              };

              modules = mkOption {
                type = types.listOf types.str;
                default = [ ];
                description = "List of blixard modules to include";
              };

              extraConfig = mkOption {
                type = types.deferredModule;
                default = { };
                description = "Additional NixOS configuration";
              };

              sshKeys = mkOption {
                type = types.listOf types.str;
                default = [ ];
                description = "SSH public keys for root access";
              };

              initCommand = mkOption {
                type = types.nullOr types.str;
                default = null;
                description = "Command to run on VM startup";
              };
            };
          });
          default = { };
          description = "Blixard VM definitions";
        };
      };
    });
  };

  config = {
    # Export standard NixOS modules
    flake.nixosModules = {
      # Core module that can be imported into any NixOS config
      blixardVM = { config, lib, pkgs, ... }: {
        options.blixard.vm = {
          enable = mkOption {
            type = types.bool;
            default = false;
            description = "Enable Blixard VM configuration";
          };
        };

        config = lib.mkIf config.blixard.vm.enable {
          # Standard Blixard VM configuration
          services.getty.autologinUser = "root";
          networking.firewall.enable = false;

          # Enable SSH with permissive settings for development
          services.openssh = {
            enable = true;
            settings = {
              PermitRootLogin = "yes";
              PasswordAuthentication = true;
              PermitEmptyPasswords = true;
            };
          };
        };
      };

      # Webserver profile
      webserver = { config, lib, pkgs, ... }: {
        services.nginx = {
          enable = lib.mkDefault true;
          virtualHosts.default = {
            default = true;
            root = "/var/www";
            locations."/" = {
              index = "index.html";
            };
          };
        };

        networking.firewall.allowedTCPPorts = [ 80 443 ];
      };

      # Database profile
      database = { config, lib, pkgs, ... }: {
        services.postgresql = {
          enable = lib.mkDefault true;
          enableTCPIP = true;
          authentication = ''
            host all all 10.0.0.0/24 trust
          '';
        };

        networking.firewall.allowedTCPPorts = [ 5432 ];
      };

      # Monitoring profile
      monitoring = { config, lib, pkgs, ... }: {
        services.prometheus = {
          enable = lib.mkDefault true;
          port = 9090;
        };

        services.grafana = {
          enable = lib.mkDefault true;
          settings.server = {
            http_port = 3000;
            http_addr = "0.0.0.0";
          };
        };

        networking.firewall.allowedTCPPorts = [ 9090 3000 ];
      };

      # Container runtime profile
      containerRuntime = { config, lib, pkgs, ... }: {
        virtualisation.docker = {
          enable = lib.mkDefault true;
          daemon.settings = {
            bip = "172.17.0.1/16";
          };
        };
      };
    };

    perSystem = { config, self', inputs', pkgs, system, ... }: {
      # Generate NixOS configurations for each defined VM
      nixosConfigurations = lib.mapAttrs
        (name: vmConfig:
          inputs'.nixpkgs.lib.nixosSystem {
            inherit system;
            specialArgs = { inherit inputs'; };
            modules = [
              inputs'.microvm.nixosModules.microvm
              self.nixosModules.blixardVM

              # Include requested modules
              (lib.mkMerge (map
                (moduleName:
                  self.nixosModules.${moduleName} or { }
                )
                vmConfig.modules))

              # VM-specific configuration
              {
                networking.hostName = name;
                blixard.vm.enable = true;

                microvm = {
                  hypervisor = vmConfig.hypervisor;
                  vcpu = vmConfig.vcpus;
                  mem = vmConfig.memory;

                  interfaces = lib.optional (vmConfig.networking.type == "tap") {
                    type = "tap";
                    id = vmConfig.networking.tapInterface or name;
                    mac = vmConfig.networking.macAddress or "02:00:00:00:00:01";
                  };

                  volumes = map
                    (vol:
                      if vol.type == "root" then {
                        image = "rootdisk.img";
                        mountPoint = "/";
                        size = vol.size;
                      } else if vol.type == "data" then {
                        image = vol.path or "data.img";
                        mountPoint = vol.mountPoint;
                        size = vol.size;
                        readOnly = vol.readOnly;
                      } else {
                        tag = baseNameOf vol.path;
                        socket = "${vol.path}.virtiofs.sock";
                        mountPoint = vol.mountPoint;
                      }
                    )
                    vmConfig.volumes;

                  socket = "/tmp/${name}-console.sock";
                };

                # SSH keys
                users.users.root.openssh.authorizedKeys.keys = vmConfig.sshKeys;

                # Init command
                systemd.services.blixard-init = lib.mkIf (vmConfig.initCommand != null) {
                  description = "Blixard VM initialization";
                  wantedBy = [ "multi-user.target" ];
                  after = [ "network.target" ];
                  serviceConfig = {
                    Type = "oneshot";
                    ExecStart = vmConfig.initCommand;
                    RemainAfterExit = true;
                  };
                };

                # Network configuration for TAP interfaces
                networking.useNetworkd = vmConfig.networking.type == "tap";
                systemd.network.networks."10-eth" = lib.mkIf (vmConfig.networking.type == "tap" && vmConfig.networking.ipAddress != null) {
                  matchConfig.MACAddress = vmConfig.networking.macAddress or "02:00:00:00:00:01";
                  address = [ "${vmConfig.networking.ipAddress}/32" ];
                  routes = [
                    {
                      Destination = "10.0.0.0/32";
                      GatewayOnLink = true;
                    }
                    {
                      Destination = "0.0.0.0/0";
                      Gateway = "10.0.0.0";
                      GatewayOnLink = true;
                    }
                  ];
                };

                system.stateVersion = "23.11";
              }

              # User's extra configuration
              vmConfig.extraConfig
            ];
          }
        )
        config.blixardVMs;

      # Generate packages for VM runners
      packages = lib.mapAttrs
        (name: vmConfig:
          config.nixosConfigurations.${name}.config.microvm.runner.${vmConfig.hypervisor}
        )
        config.blixardVMs;
    };
  };
}
