{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    microvm = {
      url = "github:astro/microvm.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    blixard-modules = {
      url = "path:{{ blixard_modules_path }}";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-parts.follows = "flake-parts";
    };
  };

  outputs = inputs@{ self, flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        # Import microvm flake module if it exists
        # inputs.microvm.flakeModule
      ];
      
      systems = [ "{{ system }}" ];
      
      perSystem = { config, self', inputs', pkgs, system, ... }: {
        # Define the VM as a NixOS configuration
        nixosConfigurations."{{ vm_name }}" = inputs.nixpkgs.lib.nixosSystem {
          inherit system;
          specialArgs = { inherit inputs; };
          modules = [
            # Base microvm module
            inputs.microvm.nixosModules.microvm
            
            # Blixard modules
            {%- for module in flake_modules %}
            inputs.blixard-modules.nixosModules.{{ module }}
            {%- endfor %}
            
            # File-based modules
            {%- for module in file_modules %}
            {{ module }}
            {%- endfor %}
            
            # Main VM configuration
            {
              networking.hostName = "{{ vm_name }}";
              
              microvm = {
                hypervisor = "{{ hypervisor }}";
                vcpu = {{ vcpus }};
                mem = {{ memory }};
                
                interfaces = [ {
                  type = "tap";
                  id = "vm{{ vm_index }}";
                  mac = "{{ vm_mac }}";
                } ];
                
                volumes = [
                  {%- for volume in volumes %}
                  {%- if volume.type == "rootDisk" %}
                  {
                    image = "rootdisk.img";
                    mountPoint = "/";
                    size = {{ volume.size }};
                  }
                  {%- elif volume.type == "dataDisk" %}
                  {
                    image = "{{ volume.path }}";
                    mountPoint = "{{ volume.mountPoint | default('/data') }}";
                    size = {{ volume.size }};
                    {%- if volume.readOnly %}
                    readOnly = true;
                    {%- endif %}
                  }
                  {%- elif volume.type == "virtiofs" %}
                  {
                    tag = "{{ volume.tag }}";
                    socket = "{{ volume.path }}.virtiofs.sock";
                    mountPoint = "{{ volume.mountPoint }}";
                  }
                  {%- endif %}
                  {%- endfor %}
                ];
                
                # Console socket for debugging
                socket = "/tmp/{{ vm_name }}-console.sock";
                
                {%- if kernel_cmdline %}
                kernelParams = [ {{ kernel_cmdline }} ];
                {%- endif %}
              };
              
              # Basic NixOS configuration
              services.getty.autologinUser = "root";
              users.users.root = {
                password = "";
                openssh.authorizedKeys.keys = [
                  {%- for key in ssh_keys %}
                  "{{ key }}"
                  {%- endfor %}
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
              
              # Network configuration
              networking.useNetworkd = true;
              networking.firewall.enable = false;
              
              systemd.network.networks."10-eth" = {
                matchConfig.MACAddress = "{{ vm_mac }}";
                address = [
                  "10.0.0.{{ vm_index }}/32"
                  "fec0::{{ vm_index_hex }}/128"
                ];
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
                  {
                    Destination = "::/0";
                    Gateway = "fec0::";
                    GatewayOnLink = true;
                  }
                ];
                networkConfig = {
                  DNS = [
                    "9.9.9.9"
                    "149.112.112.112"
                    "2620:fe::fe"
                    "2620:fe::9"
                  ];
                };
              };
              
              # Serial console kernel params
              boot.kernelParams = [ 
                "console=ttyS0,115200"
                "console=tty0"
              ];
              
              {%- if init_command %}
              # Custom initialization
              systemd.services.blixard-init = {
                description = "Blixard VM initialization";
                wantedBy = [ "multi-user.target" ];
                after = [ "network.target" ];
                serviceConfig = {
                  Type = "oneshot";
                  ExecStart = {{ init_command }};
                  RemainAfterExit = true;
                };
              };
              {%- endif %}
              
              # Inline modules
              {%- for module in inline_modules %}
              {{ module }}
              {%- endfor %}
              
              system.stateVersion = "23.11";
            }
          ];
        };
      };
      
      # Export flake outputs for the VM
      flake = {
        # Re-export the nixosConfiguration at the top level for compatibility
        nixosConfigurations."{{ vm_name }}" = 
          config.perSystem."{{ system }}".nixosConfigurations."{{ vm_name }}";
          
        # Export VM runner for convenience
        packages."{{ system }}"."{{ vm_name }}-runner" = 
          config.perSystem."{{ system }}".nixosConfigurations."{{ vm_name }}"
            .config.microvm.runner.{{ hypervisor }};
            
        # Export useful VM management apps
        apps."{{ system }}" = {
          # Run the VM
          run = {
            type = "app";
            program = "${self.packages."{{ system }}"."{{ vm_name }}-runner"}/bin/microvm-run";
          };
          
          # Console access
          console = {
            type = "app"; 
            program = toString (pkgs.writeShellScript "console" ''
              ${pkgs.socat}/bin/socat -,rawer,escape=0x1d UNIX-CONNECT:/tmp/{{ vm_name }}-console.sock
            '');
          };
        };
        
        # Development shell with VM management tools
        devShells."{{ system }}".default = pkgs.mkShell {
          buildInputs = with pkgs; [
            microvm
            socat
            qemu
            cloud-hypervisor
          ];
          
          shellHook = ''
            echo "Blixard VM: {{ vm_name }}"
            echo "Commands:"
            echo "  nix run .#run      - Start the VM"
            echo "  nix run .#console  - Connect to console"
            echo "  ssh root@10.0.0.{{ vm_index }} - SSH to VM"
          '';
        };
      };
    };
}