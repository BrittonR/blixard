use crate::utils::{
    apply_regex_patterns, check_and_generate_ssh_key_if_needed, generate_ssh_key_pair_and_save_sops,
};
use regex::Regex;
use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::process::{exit, Command};

// Function to generate a Nix module file for a MicroVM
pub fn generate_microvm(
    microvm_name: &str,
    root_password: &str,
    hypervisor: &str,
    add_tailscale: bool,
) {
    // Generate Tailscale configuration if requested
    let tailscale_config = generate_tailscale_config(add_tailscale, true);
    write_and_format_file("modules/tailscale.nix", &tailscale_config);

    // Check for existing SSH key or generate a new one if needed
    let ssh_public_key = check_and_generate_ssh_key_if_needed();

    // Generate the base_microvm.nix file with the given parameters
    let base_config = generate_base_config(true, hypervisor, root_password);
    write_and_format_file("microvms/base-microvm.nix", &base_config);

    // Create the module content with the provided parameters
    let module_content = format!(
        r#"
{{ mkMicrovmConfig }}:

        mkMicrovmConfig  {{
            name = "{microvm_name}";
            hostname = "{microvm_name}";
            volumes = [ {{
                mountPoint = "/var";
                image = "var.img";
                size = 256;
            }} ];
            shares = [ {{
                proto = "9p";
                tag = "ro-store";
                source = "/nix/store";
                mountPoint = "/nix/.ro-store";
            }} ];
            hypervisor = "{hypervisor}";
            socket = "control.socket";
            extraModules = [../modules/tailscale.nix];
        }}

        "#
    );

    // Write and format the module.nix file
    write_and_format_file(
        format!("microvms/{}.nix", microvm_name).as_str(),
        &module_content,
    );
    log_success(format!("microvms/{}.nix", microvm_name).as_str());

    // Insert the new microVM configuration into microvmConfigs.nix
    insert_microvm_into_configs(microvm_name);

    // Determine the deployment IP based on whether Tailscale is added
    let deploy_ip = if add_tailscale {
        microvm_name
    } else {
        "10.0.0.7"
    };

    // Generate and append the deployment configuration with the determined IP
    generate_and_append_deploy_config(microvm_name, &deploy_ip, true);
}

// Function to generate base configuration template
fn generate_base_config(add_defaults: bool, hypervisor: &str, root_password: &str) -> String {
    if add_defaults {
        format!(
            r#"
{{ system, inputs}}:

# Define a function to create microVM configurations
{{
  mkMicrovmConfig = {{ name, hostname, volumes, shares, hypervisor, socket, extraModules ? [] }}:
    inputs.nixpkgs.lib.nixosSystem {{
      inherit system;
      modules = [
        inputs.microvm.nixosModules.microvm
        {{
          networking.hostName = hostname;
          users.users.root.password = "";
          services.openssh.enable = true;
          microvm = {{
            inherit hypervisor socket;
            inherit volumes shares;
          }};
          # Add more shared configurations here
        }}
      ] ++ extraModules;
   }};
}}
           
"#
        )
    } else {
        String::new()
    }
}

// Function to generate Tailscale configuration template
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

/// Function to write and format the Nix file
fn write_and_format_file(file_path: &str, content: &str) {
    if let Some(parent) = Path::new(file_path).parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            eprintln!("Failed to create directories for {}: {}", file_path, err);
            return;
        }
    }

    match File::create(file_path) {
        Ok(mut file) => match file.write_all(content.as_bytes()) {
            Ok(_) => format_nix_file(file_path),
            Err(err) => eprintln!("Failed to write to {}: {}", file_path, err),
        },
        Err(err) => eprintln!("Failed to create file {}: {}", file_path, err),
    }
}

/// Function to format the Nix file using nixfmt
fn format_nix_file(file_path: &str) {
    let status = Command::new("nixfmt")
        .arg(file_path)
        .status()
        .expect("Failed to run nixfmt");

    if !status.success() {
        eprintln!("Failed to format the {} file with nixfmt", file_path);
    }
}

// Function to log success message
fn log_success(file_path: &str) {
    println!(
        "{} file has been created and formatted successfully.",
        file_path
    );
}

// Function to insert a new MicroVM configuration into configs/microvmConfigs.nix
fn insert_microvm_into_configs(microvm_name: &str) {
    let new_config_entry = format!(
        r#"
        {{
          name = "{microvm_name}";
          config = import ../microvms/{microvm_name}.nix {{ inherit (baseConfig) mkMicrovmConfig; }};
        }}
        "#,
        microvm_name = microvm_name
    );

    let file_path = "configs/microvmConfigs.nix";

    // Read the current content of microvmConfigs.nix
    let mut content = String::new();
    if let Ok(mut file) = File::open(file_path) {
        file.read_to_string(&mut content)
            .expect("Failed to read microvmConfigs.nix");
    }

    if !content.contains("[") {
        content = format!(
            r#"{{
    baseConfig
}}:

[
    {new_config_entry}
]"#
        );
    } else {
        // Insert the new MicroVM configuration into the file content
        let start_pattern = r"\[";
        let end_pattern = r"\]";
        if let Err(err) =
            insert_variable(&mut content, &new_config_entry, start_pattern, end_pattern)
        {
            eprintln!("Failed to insert new MicroVM configuration: {}", err);
            return;
        }
    }

    // Write the updated content back to microvmConfigs.nix
    write_and_format_file(file_path, &content);
}
// Function to generate a host configuration file for the MicroVM
pub fn generate_host_configuration(external_interface: &str) {
    let host_config_content = format!(
        r#"{{
  lib,
  pkgs,
  config,
  inputs,
  ...
}}: {{

  systemd.network = {{
    enable = true;
    netdevs.virbr0.netdevConfig = {{
      Kind = "bridge";
      Name = "virbr0";
    }};
    networks.virbr0 = {{
      matchConfig.Name = "virbr0";
      networkConfig = {{
        DHCPServer = false;
        IPv6SendRA = true;
      }};
      addresses = [
        {{
          addressConfig.Address = "10.0.0.1/28";
        }}
      ];
    }};
    networks.microvm-eth0 = {{
      matchConfig.Name = "vm-*";
      networkConfig.Bridge = "virbr0";
    }};
  }};

  networking.useNetworkd = true;
  networking.nameservers = ["1.1.1.1", "8.8.8.8"];
  networking.nat = {{
    enable = true;
    externalInterface = "{external_interface}";
    enableIPv6 = true;
    internalInterfaces = ["virbr0"];
  }};
}}
"#,
        external_interface = external_interface,
    );

    write_and_format_file("nixos/host-config.nix", &host_config_content);
    log_success("host-config.nix");
}

// Function to generate flake.nix configuration
pub fn generate_flake_nix() {
    initialize_configs();
    let flake_nix_content = r#"
{
  description = "NixOS configuration with flakes";

  inputs = {
    nixos-generators = {
      url = "github:nix-community/nixos-generators";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nixpkgs-stable.url = "github:NixOS/nixpkgs/nixos-24.05";
    nixpkgs-unstable.url = "github:NixOS/nixpkgs/nixos-unstable";

    flake-utils.url = "github:numtide/flake-utils";

    nix-darwin = {
      url = "github:LnL7/nix-darwin";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    nixpkgs-darwin.url = "github:nixos/nixpkgs/nixpkgs-23.11-darwin";

    sops-nix.url = "github:Mic92/sops-nix";

    home-manager.url = "github:nix-community/home-manager";
    home-manager.inputs.nixpkgs.follows = "nixpkgs";

    microvm.url = "github:astro/microvm.nix";
    microvm.inputs.nixpkgs.follows = "nixpkgs";

    disko.url = "github:nix-community/disko";
    disko.inputs.nixpkgs.follows = "nixpkgs";

    terranix.url = "github:terranix/terranix";

    nixos-hardware.url = "github:NixOS/nixos-hardware/master";

    deploy-rs.url = "github:serokell/deploy-rs";
  };

  outputs = inputs@{self, ... }:
  let
    system = "x86_64-linux";

    # Import the base configuration
    baseConfig = import ./microvms/base-microvm.nix { inherit system inputs; };

    # Import microVM configurations
    microvmConfigs = import ./configs/microvmConfigs.nix { inherit baseConfig; };

    # Import other NixOS configurations
    otherNixosConfigs = import ./configs/otherNixosConfigs.nix { inherit inputs system; };

    # Combine both lists
    allConfigs = microvmConfigs ++ otherNixosConfigs;

    # Generate nixosConfigurations and packages attributes
    nixosConfigurations = builtins.listToAttrs (map (vm: {
      name = vm.name;
      value = vm.config;
    }) allConfigs);

    packages = builtins.listToAttrs (map (vm: {
      name = vm.name;
      value = vm.config.config.microvm.declaredRunner or null;
    }) microvmConfigs) 
    ;

  in {
    inherit nixosConfigurations packages;
  };
}
"#;

    write_and_format_file("flake.nix", &flake_nix_content);
    log_success("flake.nix");
}

// Function to generate and append deployment configuration to deploy-config.nix
pub fn generate_and_append_deploy_config(
    server: &str,
    ip: &str,
    fast: bool,
) -> Result<(), Box<dyn Error>> {
    let base_config = r#"{inputs}:
    let inherit (inputs) self nixpkgs nixos-generators sops-nix deploy-rs;

    mkNode = server: ip: fast: {
      hostname = "${server}";
      fastConnection = fast;
      activationTimeout = 600;
      confirmTimeout = 600;
      sshOpts = [ "-i" "/home/admin/.ssh/admin" ];
      autoRollback = true;
      profiles.system.path =
        deploy-rs.lib.x86_64-linux.activate.nixos
          self.nixosConfigurations."${server}";
    };

    in {
      user = "admin";
      sshUser = "admin";
      nodes = {
        tower = mkNode "tower" "10.0.0.2" true;
      };
}"#;

    let new_module_info = format!(
        r#"{server} = mkNode "{server}" "{ip}" {fast};"#,
        server = server,
        ip = ip,
        fast = fast
    );

    let file_path = "nixos/deploy-config.nix";

    let mut content = String::new();
    if let Ok(mut file) = File::open(file_path) {
        file.read_to_string(&mut content)
            .expect("Failed to read deploy-config.nix");
    }

    if content.trim().is_empty() {
        content = base_config.to_string();
    }

    insert_variable(&mut content, &new_module_info, r"nodes = \{", r"\};")?;

    write_and_format_file(file_path, &content);

    println!(
        "Configuration for {} has been added to {}.",
        server, file_path
    );

    Ok(())
}

// Function to insert a variable into a string content at a specific location
// fn insert_variable(
//     content: &mut String,
//     variable: &str,
//     start_pattern: &str,
//     end_pattern: &str,
// ) -> Result<(), Box<dyn Error>> {
//     let start_re = Regex::new(start_pattern)?;
//     let end_re = Regex::new(end_pattern)?;

//     if let Some(start_pos) = start_re.find(content) {
//         let start = start_pos.end();
//         if let Some(end_pos) = end_re.find(&content[start..]) {
//             let end = start + end_pos.start();
//             content.insert_str(end, &format!("\n    {}", variable));
//             Ok(())
//         } else {
//             Err("End pattern not found".into())
//         }
//     } else {
//         Err("Start pattern not found".into())
//     }
// }
fn insert_variable(
    content: &mut String,
    variable: &str,
    start_pattern: &str,
    end_pattern: &str,
) -> Result<(), Box<dyn Error>> {
    let start_re = Regex::new(start_pattern)?;
    let end_re = Regex::new(end_pattern)?;

    if let Some(start_pos) = start_re.find(content) {
        let start = start_pos.end();
        if let Some(end_pos) = end_re.find(&content[start..]) {
            let end = start + end_pos.start();
            content.insert_str(end, &format!("\n    {}", variable));
            Ok(())
        } else {
            Err("End pattern not found".into())
        }
    } else {
        Err("Start pattern not found".into())
    }
}

// Function to generate the initial microvmConfigs.nix file

// Function to generate the initial otherNixosConfigs.nix file

const TAILSCALE_MODULE_TEMPLATE: &str = r#"
{
  pkgs,
  config,
  ...
}: {
  sops.secrets."tailscale/authKey" = {
    sopsFile = "${inputs.self}/sops/tailscale.enc.yaml";
  };
  sops.secrets."tailscale/apiKey" = {
    sopsFile = "${inputs.self}/sops/tailscale.enc.yaml";
  };
  sops.secrets."tailscale/tailnet" = {
    sopsFile = "${inputs.self}/sops/tailscale.enc.yaml";
  };
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
  microvm.shares = [{
    tag = "tailscale";
    source = "/run/secrets/tailscale";
    mountPoint = "/dev/tailscale";
    proto = "virtiofs";
  }];
}
"#;
// Function to generate the initial microvmConfigs.nix file
fn generate_initial_microvm_configs() {
    let content = r#"{ baseConfig }:

[
  # Add initial microVM configurations here
]"#;

    write_and_format_file("configs/microvmConfigs.nix", content);
    log_success("configs/microvmConfigs.nix");
}

// Function to generate the initial otherNixosConfigs.nix file
fn generate_initial_other_nixos_configs() {
    let content = r#"{ inputs, system }:

[
  # Add initial other NixOS configurations here
]"#;

    write_and_format_file("configs/otherNixosConfigs.nix", content);
    log_success("configs/otherNixosConfigs.nix");
}

// Initialize the configs/microvmConfigs.nix and configs/otherNixosConfigs.nix files
pub fn initialize_configs() {
    generate_initial_microvm_configs();
    generate_initial_other_nixos_configs();
}
