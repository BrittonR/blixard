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
    in
    {
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

              interfaces = [{
                type = "tap";
                id = "vm{{ vm_index }}";
                mac = "{{ vm_mac }}";
              }];

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
                "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILYzh3yIsSTOYXkJMFHBKzkakoDfonm3/RED5rqMqhIO britton@framework"
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

            # Routed network configuration
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
                ExecStart = "${pkgs.coreutils}/bin/echo 'Blixard VM {{ vm_name }} started successfully! SSH: 10.0.0.{{ vm_index }}:22'";
                RemainAfterExit = true;
              };
            };

            system.stateVersion = "23.11";
          }
        ];
      };
    };
}
