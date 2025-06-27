# Example of using flake-parts for Blixard VM configuration
#
# This demonstrates how users can define VMs using the flake-parts module system
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    microvm = {
      url = "github:astro/microvm.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    blixard-modules = {
      url = "path:../nix/modules";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs@{ self, flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        inputs.blixard-modules.flakeModule
      ];
      
      systems = [ "x86_64-linux" ];
      
      perSystem = { config, self', inputs', pkgs, system, ... }: {
        # Define multiple VMs using the blixardVMs option
        blixardVMs = {
          # Web server VM
          webserver = {
            hypervisor = "cloud-hypervisor";
            vcpus = 2;
            memory = 1024;
            networking = {
              type = "tap";
              tapInterface = "vm1";
              macAddress = "02:00:00:00:01:01";
              ipAddress = "10.0.0.1";
            };
            modules = [ "webserver" ];
            sshKeys = [
              "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILYzh3yIsSTOYXkJMFHBKzkakoDfonm3/RED5rqMqhIO britton@framework"
            ];
            extraConfig = {
              services.nginx.virtualHosts."blixard.example" = {
                root = "/var/www/blixard";
                locations."/" = {
                  return = "200 'Blixard VM with flake-parts!'";
                  extraConfig = "add_header Content-Type text/plain;";
                };
              };
            };
          };
          
          # Database VM
          database = {
            hypervisor = "cloud-hypervisor";
            vcpus = 4;
            memory = 4096;
            networking = {
              type = "tap";
              tapInterface = "vm2";
              macAddress = "02:00:00:00:02:01";
              ipAddress = "10.0.0.2";
            };
            volumes = [
              {
                type = "root";
                size = 10240;
              }
              {
                type = "data";
                path = "/var/lib/postgresql";
                mountPoint = "/data";
                size = 20480;
              }
            ];
            modules = [ "database" ];
            sshKeys = [
              "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILYzh3yIsSTOYXkJMFHBKzkakoDfonm3/RED5rqMqhIO britton@framework"
            ];
            extraConfig = {
              services.postgresql = {
                dataDir = "/data/postgresql";
                settings = {
                  max_connections = 200;
                  shared_buffers = "1GB";
                };
              };
            };
          };
          
          # Development VM with multiple services
          dev-env = {
            hypervisor = "qemu";  # Use QEMU for development
            vcpus = 8;
            memory = 8192;
            networking = {
              type = "tap";
              tapInterface = "vm3";
              macAddress = "02:00:00:00:03:01";
              ipAddress = "10.0.0.3";
            };
            modules = [ 
              "webserver" 
              "database" 
              "containerRuntime" 
              "monitoring"
            ];
            initCommand = ''
              ${pkgs.coreutils}/bin/echo "Development VM started!" | ${pkgs.systemd}/bin/systemd-cat -t blixard
            '';
            extraConfig = {
              # Development tools
              environment.systemPackages = with pkgs; [
                git
                vim
                tmux
                htop
                docker-compose
              ];
              
              # Enable Docker
              virtualisation.docker.enable = true;
              
              # Custom services for development
              services.code-server = {
                enable = true;
                host = "0.0.0.0";
                port = 8080;
              };
            };
          };
        };
        
        # Convenience apps for managing VMs
        apps = {
          start-all = {
            type = "app";
            program = toString (pkgs.writeShellScript "start-all" ''
              echo "Starting all VMs..."
              nix run .#webserver &
              nix run .#database &
              nix run .#dev-env &
              wait
            '');
          };
          
          status = {
            type = "app";
            program = toString (pkgs.writeShellScript "status" ''
              echo "VM Status:"
              echo "=========="
              for vm in webserver database dev-env; do
                echo -n "$vm: "
                if ${pkgs.procps}/bin/pgrep -f "microvm.*$vm" > /dev/null; then
                  echo "Running"
                else
                  echo "Stopped"
                fi
              done
            '');
          };
        };
        
        # Development shell with VM management tools
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            nixpkgs-fmt
            nix-tree
            socat
            tmux
          ];
          
          shellHook = ''
            echo "Blixard VM Development Environment"
            echo "================================="
            echo ""
            echo "Available VMs:"
            echo "  - webserver (10.0.0.1)"
            echo "  - database  (10.0.0.2)"
            echo "  - dev-env   (10.0.0.3)"
            echo ""
            echo "Commands:"
            echo "  nix run .#webserver    - Start webserver VM"
            echo "  nix run .#database     - Start database VM"
            echo "  nix run .#dev-env      - Start development VM"
            echo "  nix run .#start-all    - Start all VMs"
            echo "  nix run .#status       - Check VM status"
            echo ""
            echo "SSH Access:"
            echo "  ssh root@10.0.0.X"
            echo ""
          '';
        };
      };
    };
}