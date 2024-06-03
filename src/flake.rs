use std::fs::File;
use std::io::Write;
use std::process::{exit, Command};

pub fn generate_flake(
    microvm_name: &str,
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
    let base_config = generate_base_config(add_defaults);
    let tailscale_config = generate_tailscale_config(add_tailscale, false);

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
        default = self.packages.${{system}}.{}-mvm;
        {}-mvm = self.nixosConfigurations.{}-mvm.config.microvm.declaredRunner;
      }};

      nixosConfigurations = {{
        {}-mvm = nixpkgs.lib.nixosSystem {{
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
        microvm_name,
        microvm_name,
        microvm_name,
        microvm_name,
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

    write_and_format_file("new/flake.nix", &flake_content);
    log_success("flake.nix");
}

pub fn generate_module(
    microvm_name: &str,
    description: &str,
    hostname: &str,
    root_password: &str,
    hypervisor: &str,
    add_tailscale: bool,
) {
    let base_config = generate_base_config(true);
    let tailscale_config = generate_tailscale_config(add_tailscale, true);

    let module_content = format!(
        r#"{{
  inputs}}:

  let
    system = "x86_64-linux";
    inherit (inputs) nixpkgs sops-nix home-manager microvm deploy-rs;

    self = rec {{
      packages = {{
        {name}-mvm = self.nixosConfigurations.{name}-mvm.config.microvm.declaredRunner;
      }};

      nixosConfigurations = {{
        {name}-mvm = nixpkgs.lib.nixosSystem {{
          inherit (inputs);
          modules = [
            microvm.nixosModules.microvm
            home-manager.nixosModules.home-manager
            sops-nixosModules.sops
            {base_config}
            {tailscale_config}
            {{
              networking.hostName = "{hostname}";
              microvm = {{
                hypervisor = "{hypervisor}";
              }};
            }}
          ];
        }};
      }};
    }};
  in self
"#,
        name = microvm_name,
        base_config = base_config,
        tailscale_config = tailscale_config,
        hostname = hostname,
        hypervisor = hypervisor,
    );

    write_and_format_file("new/module.nix", &module_content);
    log_success("module.nix");
}

fn generate_base_config(add_defaults: bool) -> String {
    if add_defaults {
        BASE_CONFIG_TEMPLATE.to_string()
    } else {
        String::new()
    }
}

fn generate_tailscale_config(add_tailscale: bool, is_module: bool) -> String {
    if add_tailscale {
        if is_module {
            TAILSCALE_MODULE_TEMPLATE.to_string()
        } else {
            TAILSCALE_FLAKE_TEMPLATE.to_string()
        }
    } else {
        String::new()
    }
}

fn write_and_format_file(file_path: &str, content: &str) {
    match File::create(file_path) {
        Ok(mut file) => match file.write_all(content.as_bytes()) {
            Ok(_) => format_nix_file(file_path),
            Err(err) => eprintln!("Failed to write to {}: {}", file_path, err),
        },
        Err(err) => eprintln!("Failed to create {}: {}", file_path, err),
    }
}

fn format_nix_file(file_path: &str) {
    let status = Command::new("nixfmt")
        .arg(file_path)
        .status()
        .expect("Failed to run nixfmt");

    if !status.success() {
        eprintln!("Failed to format the Nix file with nixfmt");
        exit(1);
    }
}

fn log_success(file_path: &str) {
    println!(
        "{} file has been created and formatted successfully.",
        file_path
    );
}

const BASE_CONFIG_TEMPLATE: &str = r#"
{
  nixpkgs.hostPlatform = "${system}";
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
}
"#;

const TAILSCALE_MODULE_TEMPLATE: &str = r#"
"${inputs.self}/modules/tailscale.nix"
{
  sops.secrets."tailscale/authKey" = {
    sopsFile = "${inputs.self}/sops/tailscale.enc.yaml";
  };
  sops.secrets."tailscale/apiKey" = {
    sopsFile = "${inputs.self}/sops/tailscale.enc.yaml";
  };
  sops.secrets."tailscale/tailnet" = {
    sopsFile = "${inputs.self}/sops/tailscale.enc.yaml";
  };
  networking.interfaces.eth0.ipv4.addresses = [{
    address = "10.0.0.7";
    prefixLength = 28;
  }];
  microvm.shares = [{
    tag = "tailscale";
    source = "/run/secrets/tailscale";
    mountPoint = "/dev/tailscale";
    proto = "virtiofs";
  }];
}
"#;

const TAILSCALE_FLAKE_TEMPLATE: &str = r#"
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
"#;
