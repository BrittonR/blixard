use clap::{ArgAction, Parser, Subcommand};
use flake::{generate_flake_nix, generate_host_configuration, generate_microvm};
use nginx::generate_nginx_config;
use sops::{aws, cloudflare, tailscale, wikijs};
use ssh_keys::generate_ssh_keys;
use utils::{get_hypervisor_input, get_input, get_input_bool, select_network_interface};

mod action;
mod flake;
mod nginx;
mod sops;
mod ssh_keys;
mod utils;

#[derive(Parser)]
#[command(
    name = "Blixard",
    version = "0.1",
    author = "Britton Robitzsch <b@robitzs.ch>",
    about = "Opinionated Tool for generating NixOS Infrastructure using MicroVMs"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Generate SSH keys for a project")]
    GenerateSshKeys { project_name: Option<String> },
    #[command(about = "Generate a Nix module for a MicroVM")]
    GenerateMicrovm {
        description: Option<String>,
        root_password: Option<String>,
        hypervisor: Option<String>,
        add_tailscale: Option<bool>,
    },
    #[command(about = "Generate the flake.nix configuration")]
    GenerateFlakeNix,
    #[command(about = "Generate a host configuration for the MicroVM")]
    GenerateHostConfig,
    #[command(about = "Generate and encrypt configuration files using sops")]
    Sops {
        #[command(subcommand)]
        command: SopsCommands,
    },
    #[command(about = "Generate a GitHub Action")]
    GenerateAction {
        name: String,
        working_directory: String,
    },
    #[command(about = "Generate an Nginx configuration")]
    GenerateNginxConfig {
        name: String,
        port: u16,
        ip: String,
        #[arg(short, long, action = ArgAction::SetTrue)]
        enabled: bool,
    },
}

#[derive(Subcommand)]
enum SopsCommands {
    #[command(about = "Generate a Tailscale configuration file and encrypt it with sops")]
    Tailscale {
        auth_key: Option<String>,
        api_key: Option<String>,
        tailnet: Option<String>,
        exit_node: Option<String>,
    },
    #[command(about = "Generate an AWS configuration file and encrypt it with sops")]
    Aws {
        access_key_id: Option<String>,
        secret_access_key: Option<String>,
        region: Option<String>,
    },
    #[command(about = "Generate a Cloudflare configuration file and encrypt it with sops")]
    Cloudflare { dns_api_token: Option<String> },
    #[command(about = "Generate a Wiki.js configuration file and encrypt it with sops")]
    Wikijs {
        db_user: Option<String>,
        db_password: Option<String>,
    },
}

fn main() {
    // Parse CLI arguments
    let cli = Cli::parse();

    match &cli.command {
        // Handle GenerateSshKeys command
        Commands::GenerateSshKeys { project_name } => {
            let project_name = get_input(project_name, "Project Name");
            generate_ssh_keys(&project_name)
        }
        // Handle GenerateMicrovm command
        Commands::GenerateMicrovm {
            description,
            root_password,
            hypervisor,
            add_tailscale,
        } => {
            let microvm_name = get_input(&None, "MicroVM name");
            let description = get_input(
                &Some(format!("MicroVM description for {}", microvm_name)),
                "Description",
            );
            let root_password = get_input(root_password, "Root Password");
            let hypervisor = get_hypervisor_input(hypervisor.as_deref());
            let add_tailscale = get_input_bool("Add Tailscale configuration?");
            generate_microvm(
                &microvm_name,
                // &description,
                &root_password,
                &hypervisor,
                add_tailscale,
            )
        }
        // Handle GenerateFlakeNix command
        Commands::GenerateFlakeNix => {
            generate_flake_nix();
        }
        // Handle GenerateHostConfig command
        Commands::GenerateHostConfig => {
            let external_interface = select_network_interface();
            generate_host_configuration(&external_interface);
        }
        // Handle Sops command
        Commands::Sops { command } => match command {
            // Handle Tailscale subcommand
            SopsCommands::Tailscale {
                auth_key,
                api_key,
                tailnet,
                exit_node,
            } => tailscale(
                auth_key.as_deref(),
                api_key.as_deref(),
                tailnet.as_deref(),
                exit_node.as_deref(),
            ),
            // Handle Aws subcommand
            SopsCommands::Aws {
                access_key_id,
                secret_access_key,
                region,
            } => aws(
                access_key_id.as_deref(),
                secret_access_key.as_deref(),
                region.as_deref(),
            ),
            // Handle Cloudflare subcommand
            SopsCommands::Cloudflare { dns_api_token } => cloudflare(dns_api_token.as_deref()),
            // Handle Wikijs subcommand
            SopsCommands::Wikijs {
                db_user,
                db_password,
            } => wikijs(db_user.as_deref(), db_password.as_deref()),
        },
        // Handle GenerateAction command
        Commands::GenerateAction {
            name,
            working_directory,
        } => match action::generate_github_action(name, working_directory) {
            Ok(_) => println!("GitHub Action workflow generated successfully."),
            Err(e) => eprintln!("Failed to generate GitHub Action workflow: {}", e),
        },
        // Handle GenerateNginxConfig command
        Commands::GenerateNginxConfig {
            name,
            port,
            ip,
            enabled,
        } => generate_nginx_config(name, port, &ip, enabled),
    }
}
