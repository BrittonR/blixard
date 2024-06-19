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
    // Define patterns and corresponding inserts for modifying flake.nix
    // The first pattern matches the 'in {...};' section and inserts the MicroVM configuration import
    // The second pattern matches the 'let ... in' section and inserts the MicroVM NixOS configuration
    let patterns_and_inserts = [
        (
            r"let\s*([\s\S]*?)\n\s*in",
            format!(
                r#"{microvm_name}Config = import ./nixos/microvm/{microvm_name}.nix {{ inherit inputs; }};"#
            ).to_string(),
        ),
        (
            r"in\s*\{([\s\S]*?)\n\s*\};",
            format!(
                r#"{microvm_name}-mvm = {microvm_name}Config.nixosConfigurations.{microvm_name}-mvm;"#
            ).to_string(),
        ),
    ];

    // Apply the regex replacements to flake.nix using the defined patterns and inserts
    apply_regex_patterns("flake.nix", "flake.nix", &patterns_and_inserts);

    // Generate Tailscale configuration if requested
    let tailscale_config = generate_tailscale_config(add_tailscale, true);
    write_and_format_file("modules/tailscale.nix", &tailscale_config);

    // Check for existing SSH key or generate a new one if needed
    let ssh_public_key = check_and_generate_ssh_key_if_needed();

    // Generate the base_microvm.nix file with the given parameters
    let base_config = generate_base_config(true, hypervisor, root_password);
    write_and_format_file("nixos/base_microvm.nix", &base_config);

    // Create the module content with the provided parameters
    // The module content includes the system architecture, inputs, and configurations for the MicroVM
    let module_content = format!(
        r#"{{
  inputs }}:

  let
    system = "x86_64-linux";
    inherit (inputs) nixpkgs sops-nix home-manager microvm deploy-rs;

    self = rec {{
      packages = {{
        {microvm_name}-mvm = self.nixosConfigurations.{microvm_name}-mvm.config.microvm.declaredRunner;
      }};

      nixosConfigurations = {{
        {microvm_name}-mvm = nixpkgs.lib.nixosSystem {{
        specialArgs = {{ inherit inputs system; }};  # Ensure inputs and system are passed here
        inherit system;
        system = "${{system}}";

          modules = [
            ../base_microvm.nix  # Import base MicroVM configuration
            microvm.nixosModules.microvm  # Import MicroVM module
            home-manager.nixosModules.home-manager  # Import Home Manager module
            sops-nix.nixosModules.sops  # Import SOPS module for secret management
            ../../modules/tailscale.nix
            {{
              networking.hostName = "{microvm_name}";  # Set the hostname for the microvm
            }}
          ];
        }};
      }};
    }};
  in self
"#
    );

    // Write and format the module.nix file
    write_and_format_file(
        format!("nixos/microvm/{microvm_name}.nix").as_str(),
        &module_content,
    );
    log_success(format!("nixos/microvm/{microvm_name}.nix").as_str());

    // Determine the deployment IP based on whether Tailscale is added
    // Use the MicroVM name as the hostname if Tailscale is added, otherwise use a local IP
    let deploy_ip = if add_tailscale {
        microvm_name // Use the Tailscale hostname
    } else {
        "10.0.0.7" // Use local IP
    };

    // Generate and append the deployment configuration with the determined IP
    generate_and_append_deploy_config(microvm_name, &deploy_ip, true);
}

// Function to generate base configuration template
fn generate_base_config(add_defaults: bool, hypervisor: &str, root_password: &str) -> String {
    if add_defaults {
        format!(
            r#"
            {{
            pkgs,
            config,
            system,
            ...
            }}: 

            {{
              nixpkgs.hostPlatform = "${{system}}";  # Set the host platform for nixpkgs
              environment.noXlibs = false;  # Disable X libraries to minimize the environment
              microvm.hypervisor = "{hypervisor}";  # Set the hypervisor to be used (cloud-hypervisor)
            
              # Configure root user
              users.users.root.password = "{root_password}";  # Set the root password
            
              sops.secrets."ssh_public_key" = {{
                sopsFile = "${{inputs.self}}/admin_ssh_key.enc.yaml";
              }};
              # Enable and configure OpenSSH service
              services.openssh = {{
                enable = true;  # Enable OpenSSH service
                settings.PermitRootLogin = "yes";  # Allow root login
                # other common options:
                # settings.PasswordAuthentication = false;  # Disable password authentication
              }};
            
              # Configure admin group and user
              users.groups.admin = {{}};  # Create admin group
              users.users.admin = {{
                isNormalUser = true;  # Create a normal user
                group = "admin";  # Add user to admin group
                extraGroups = ["wheel" "admin" "sudo"];  # Add user to additional groups
                password = "test";  # Set user password
                home = "/home/admin";  # Set home directory
                createHome = true;  # Create home directory for user
                openssh.authorizedKeys.keys = ["${{config.sops.secrets."ssh_public_key".value}}"];  # Set SSH authorized keys
              }};

            
              # Configure sudo rules for admin group
              security.sudo.extraRules = [{{
                groups = [ "admin" ];  # Apply rules to admin group
                commands = [{{
                  command = "ALL";  # Allow all commands
                  options = [ "NOPASSWD" ];  # Allow without password
                }}];
              }}];
            
              # Configure networking settings
              networking.useNetworkd = false;  # Use legacy networking
              networking.interfaces.eth0.useDHCP = false;  # Disable DHCP
              networking.defaultGateway = "10.0.0.1";  # Set default gateway
              networking.nameservers = ["1.1.1.1" "8.8.8.8"];  # Set nameservers
            
              # Configure writable store overlay
              microvm.writableStoreOverlay = "/nix/.rwstore";  # Path to writable store overlay
            
              # Configure volumes for the microVM
              microvm.volumes = [ {{
                image = "nix-store-overlay.img";  # Image file for the volume
                mountPoint = config.microvm.writableStoreOverlay;  # Mount point for the volume
                size = 2048;  # Size of the volume in MB
              }} ];
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
///
/// # Arguments
///
/// * `file_path` - A string slice that holds the path of the file
/// * `content` - A string slice that holds the content to be written to the file
///
/// This function ensures that the necessary directories are created before attempting to write the file.
/// It then writes the content to the file and formats it using `nixfmt`.
fn write_and_format_file(file_path: &str, content: &str) {
    // Ensure the directory exists
    if let Some(parent) = Path::new(file_path).parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            eprintln!("Failed to create directories for {}: {}", file_path, err);
            return;
        }
    }

    // Create and write to the file
    match File::create(file_path) {
        Ok(mut file) => match file.write_all(content.as_bytes()) {
            Ok(_) => format_nix_file(file_path),
            Err(err) => eprintln!("Failed to write to {}: {}", file_path, err),
        },
        Err(err) => eprintln!("Failed to create file {}: {}", file_path, err),
    }
}

/// Function to format the Nix file using nixfmt
///
/// # Arguments
///
/// * `file_path` - A string slice that holds the path of the file
///
/// This function runs the `nixfmt` command to format the specified Nix file.
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

// Function to generate a host configuration file for the MicroVM
pub fn generate_host_configuration(external_interface: &str) {
    // Create the content for host-config.nix
    let host_config_content = format!(
        r#"{{
  lib,
  pkgs,
  config,
  inputs,
  ...
}}: {{

  # Enable systemd-networkd for managing network configurations
  systemd.network = {{
    enable = true;

    # Define a bridge interface named virbr0
    netdevs.virbr0.netdevConfig = {{
      Kind = "bridge";
      Name = "virbr0";
    }};

    # Configure the bridge interface virbr0
    networks.virbr0 = {{
      matchConfig.Name = "virbr0";

      # Network configuration for the bridge interface
      networkConfig = {{
        DHCPServer = false;  # Disable DHCP server
        IPv6SendRA = true;   # Enable IPv6 Router Advertisements
      }};

      # Set the IP address for the bridge interface
      addresses = [
        {{
          addressConfig.Address = "10.0.0.1/28";
        }}
      ];
    }};

    # Configure the virtual machine network interface to use the bridge
    networks.microvm-eth0 = {{
      matchConfig.Name = "vm-*";
      networkConfig.Bridge = "virbr0";  # Bridge the VM interface to virbr0
    }};
  }};

  # Use systemd-networkd for network configuration
  networking.useNetworkd = true;

  # Set the DNS nameservers
  networking.nameservers = ["1.1.1.1", "8.8.8.8"];

  # Enable NAT for the external interface
  networking.nat = {{
    enable = true;
    externalInterface = "{external_interface}";  # External interface for NAT
    enableIPv6 = true;  # Enable IPv6 NAT
    internalInterfaces = ["virbr0"];  # Internal interface for NAT
  }};
}}
"#,
        external_interface = external_interface,
    );

    // Write and format the host-config.nix file
    write_and_format_file("nixos/host-config.nix", &host_config_content);
    log_success("host-config.nix");
}

// Function to generate flake.nix configuration
pub fn generate_flake_nix() -> () {
    // Define the flake.nix content
    let flake_nix_content = r#"
{
  description = "NixOS configuration with flakes";

  # Inputs section declares all the dependencies and external sources needed for this configuration.
  inputs = {
    # nixos-generators: Used for generating NixOS images for different platforms.
    nixos-generators = {
      url = "github:nix-community/nixos-generators";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # nixpkgs: The standard Nix Packages collection, tracking the unstable branch.
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nixpkgs-stable.url = "github:NixOS/nixpkgs/nixos-24.05";
    nixpkgs-unstable.url = "github:NixOS/nixpkgs/nixos-unstable";

    # flake-utils: Provides utilities for flake development.
    flake-utils.url = "github:numtide/flake-utils";

    # nix-darwin: Used for managing macOS configurations.
    nix-darwin = {
      url = "github:LnL7/nix-darwin";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # nixpkgs-darwin: Nix packages collection tailored for Darwin.
    nixpkgs-darwin.url = "github:nixos/nixpkgs/nixpkgs-23.11-darwin";

    # sops-nix: Provides support for NixOS and home-manager with encrypted secrets using sops.
    sops-nix.url = "github:Mic92/sops-nix";

    # home-manager: A tool for managing user environments using Nix.
    home-manager.url = "github:nix-community/home-manager";
    home-manager.inputs.nixpkgs.follows = "nixpkgs";

    # microvm: Builds and runs NixOS on various Type-2 Hypervisors, providing isolation and high performance with virtio interfaces.
    microvm.url = "github:astro/microvm.nix";
    microvm.inputs.nixpkgs.follows = "nixpkgs";

    # disko: A declarative disk partitioning tool.
    disko.url = "github:nix-community/disko";
    disko.inputs.nixpkgs.follows = "nixpkgs";

    # terranix: Define Terraform configuration using Nix.
    terranix.url = "github:terranix/terranix";

    # nixos-hardware: Collection of NixOS hardware configurations.
    nixos-hardware.url = "github:NixOS/nixos-hardware/master";

    # deploy-rs: A tool for deploying Nix-based systems.
    deploy-rs.url = "github:serokell/deploy-rs";

  };

  outputs = inputs@{ ... }:
  let

    nixConfiguration = import ./nixos/nixos-configurations.nix {inherit inputs;};

    deploy = import ./nixos/deploy-config.nix {inherit inputs;};

  in {
    nixosConfigurations = {
      inherit (nixConfiguration);
    };

    deploy = deploy;
  };
}
"#;

    // Write the flake.nix content and format the file
    write_and_format_file("flake.nix", &flake_nix_content);
    log_success("flake.nix");
}
/// Helper function to insert a variable into the configuration content
///
/// # Arguments
///
/// * `content` - The mutable string content to be updated
/// * `variable` - The new variable to be inserted
/// * `start_pattern` - The regex pattern marking the start of the insertion point
/// * `end_pattern` - The regex pattern marking the end of the insertion point
fn insert_variable(
    content: &mut String,
    variable: &str,
    start_pattern: &str,
    end_pattern: &str,
) -> Result<(), Box<dyn Error>> {
    // Define the regex patterns
    let start_re = Regex::new(start_pattern)?;
    let end_re = Regex::new(end_pattern)?;

    // Find the insertion point using the start pattern
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

/// Function to generate and append deployment configuration to deploy-config.nix
///
/// # Arguments
///
/// * `server` - A string slice representing the server name
/// * `ip` - A string slice representing the IP address of the server
/// * `fast` - A boolean indicating if the server has a fast connection
///
/// # Returns
///
/// This function returns a `Result` with unit type on success and `Box<dyn Error>` on failure
pub fn generate_and_append_deploy_config(
    server: &str,
    ip: &str,
    fast: bool,
) -> Result<(), Box<dyn Error>> {
    // Define the base configuration template for deploy-config.nix
    let base_config = r#"{inputs}:
    let inherit (inputs) self nixpkgs nixos-generators sops-nix deploy-rs;

    # Function to create a node configuration
    mkNode = server: ip: fast: {
    hostname = "${ip}"; # Set the hostname to the provided IP address
    fastConnection = fast; # Specify if the node has a fast connection (true/false)
    activationTimeout = 600; # Timeout for activation in seconds (Max 600)
    confirmTimeout = 600; # Timeout for confirmation in seconds (Max 600)
    sshOpts = [ "-i" "/home/admin/.ssh/admin" ]; # SSH options, including path to the SSH key
    autoRollback = true; # Automatically rollback if the activation fails

    # Define the system profile path using deploy-rs activation
    profiles.system.path =
      deploy-rs.lib.x86_64-linux.activate.nixos
        self.nixosConfigurations."${server}"; # Add configurations to available builds
};

in {
  user = "admin"; # Default user for SSH connections
  sshUser = "admin"; # User for SSH connections during deployment
  nodes = {
    tower = mkNode "tower" "nixos" true; # Define the node 'tower' with specific parameters
  };
}"#;

    // Generate the new module information to be appended
    let new_module_info = format!(
        r#"{server} = mkNode "{server}" "{ip}" {fast};"#,
        server = server,
        ip = ip,
        fast = fast
    );

    // Path to the deploy-config.nix file
    let file_path = "nixos/deploy-config.nix";

    // Open the deploy-config.nix file, creating it if it doesn't exist
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(file_path)?;

    // Read the content of the file into a string
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    // If the file is empty, initialize it with the base configuration
    if content.trim().is_empty() {
        content = base_config.to_string();
    }

    // Insert the new module information into the configuration content
    insert_variable(&mut content, &new_module_info, r"nodes = \{", r"\};")?;

    // Write the updated content back to the deploy-config.nix file and format it
    write_and_format_file(file_path, &content);

    // Print a success message indicating the new configuration has been added
    println!(
        "Configuration for {} has been added to {}.",
        server, file_path
    );

    Ok(())
}

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
