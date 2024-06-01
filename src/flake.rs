use std::fs::File;
use std::io::Write;
use std::process::{exit, Command};

pub fn generate_flake(
    description: &str,
    hostname: &str,
    root_password: &str,
    image: &str,
    image_size: &str,
    share_source: &str,
    hypervisor: &str,
    socket: &str,
    add_defaults: bool,
    add_tailscale: bool,
) {
    let base_config = if add_defaults {
        r#"
  environment.noXlibs = false;
  microvm.hypervisor = "cloud-hypervisor";
  users.users.root.password = "test";
  services.openssh = {
    enable = true;
    # settings.PasswordAuthentication = false;
    settings.PermitRootLogin = "yes";
  };
  users.users.britton = {
    isNormalUser = true;
    group = "wheel";
    password = "test";
    createHome = true;
  };

  networking.useNetworkd = false;
  # systemd.network.enable = true;

  networking.interfaces.eth0.useDHCP = false;

  networking.defaultGateway = "10.0.0.1";
  networking.nameservers = ["1.1.1.1" "8.8.8.8"];

  microvm.writableStoreOverlay = "/nix/.rwstore";

  microvm.volumes = [ {
    image = "nix-store-overlay.img";
    mountPoint = config.microvm.writableStoreOverlay;
    size = 2048;
  } ];
  # microvm.mem = 4096;
"#
    } else {
        ""
    };

    let tailscale_config = if add_tailscale {
        r#"
      ../modules/tailscale.nix
      {
        sops.secrets."tailscale/authKey" = {
          sopsFile = ../sops/tailscale.enc.yaml;
        };
        sops.secrets."tailscale/apiKey" = {
          sopsFile = ../sops/tailscale.enc.yaml;
        };
        sops.secrets."tailscale/tailnet" = {
          sopsFile = ../sops/tailscale.enc.yaml;
        };
        networking.interfaces.eth0.ipv4.addresses = [{
          address = "10.0.0.7";
          prefixLength = 28;
        }];
        microvm.shares = [
          {
            tag = "tailscale";
            source = "/run/secrets/tailscale";
            mountPoint = "/dev/tailscale";
            proto = "virtiofs";
          }
        ];
      }
"#
    } else {
        ""
    };

    let flake_content = format!(
        r#"{{
  description = "{}";

  inputs.microvm.url = "github:astro/microvm.nix";
  inputs.microvm.inputs.nixpkgs.follows = "nixpkgs";

  outputs = {{ self, nixpkgs, microvm }}:
    let
      system = "x86_64-linux";
    in {{
      packages = {{
        default = self.packages.${{system}}.my-microvm;
        my-microvm = self.nixosConfigurations.my-microvm.config.microvm.declaredRunner;
      }};

      nixosConfigurations = {{
        my-microvm = nixpkgs.lib.nixosSystem {{
          inherit system;
          modules = [
            microvm.nixosModules.microvm
            {}
            {}
            {{
              networking.hostName = "{}";
              users.users.root.password = "{}";
              microvm = {{
                volumes = [ {{
                  mountPoint = "/var";
                  image = "{}";
                  size = {};
                }} ];
                shares = [ {{
                  proto = "9p";
                  tag = "ro-store";
                  source = "{}";
                  mountPoint = "/nix/.ro-store";
                }} ];

                hypervisor = "{}";
                socket = "{}";
              }};
              {}
            }}
          ];
        }};
      }};
    }};
}}"#,
        description,
        base_config,
        tailscale_config,
        hostname,
        root_password,
        image,
        image_size,
        share_source,
        hypervisor,
        socket,
        ""
    );

    let file_path = "new/flake.nix";
    let mut file = File::create(file_path).expect("Unable to create file");
    file.write_all(flake_content.as_bytes())
        .expect("Unable to write data");

    format_flake(file_path);

    println!("flake.nix file has been created and formatted successfully.");
}

fn format_flake(file_path: &str) {
    let status = Command::new("nixfmt")
        .arg(file_path)
        .status()
        .expect("Failed to run nixfmt");

    if !status.success() {
        eprintln!("Failed to format the flake.nix file with nixfmt");
        exit(1);
    }
}
