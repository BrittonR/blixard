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
    #[command(name = "ssh-keys")]
    SshKeys { project_name: Option<String> },

    #[command(name = "microvm")]
    Microvm {
        description: Option<String>,
        root_password: Option<String>,
        hypervisor: Option<String>,
        add_tailscale: Option<bool>,
    },

    #[command(name = "flake")]
    Flake,

    #[command(name = "host")]
    Host,

    #[command(name = "sops")]
    Sops {
        #[command(subcommand)]
        command: SopsCommands,
    },

    #[command(name = "action")]
    Action {
        name: String,
        working_directory: String,
    },

    #[command(name = "nginx")]
    Nginx {
        name: String,
        port: u16,
        ip: String,
        #[arg(short, long, action = ArgAction::SetTrue)]
        enabled: bool,
    },
}

#[derive(Subcommand)]
enum SopsCommands {
    #[command(name = "tailscale")]
    Tailscale {
        auth_key: Option<String>,
        api_key: Option<String>,
        tailnet: Option<String>,
        exit_node: Option<String>,
    },

    #[command(name = "aws")]
    Aws {
        access_key_id: Option<String>,
        secret_access_key: Option<String>,
        region: Option<String>,
    },

    #[command(name = "cloudflare")]
    Cloudflare { dns_api_token: Option<String> },

    #[command(name = "wikijs")]
    Wikijs {
        db_user: Option<String>,
        db_password: Option<String>,
    },
}

fn main() {
    // Parse CLI arguments
    let cli = Cli::parse();

    match &cli.command {
        Commands::SshKeys { project_name } => {
            let project_name = get_input(project_name, "Project Name");
            generate_ssh_keys(&project_name)
        }
        Commands::Microvm {
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
            generate_microvm(&microvm_name, &root_password, &hypervisor, add_tailscale)
        }
        Commands::Flake => {
            generate_flake_nix();
        }
        Commands::Host => {
            let external_interface = select_network_interface();
            generate_host_configuration(&external_interface);
        }
        Commands::Sops { command } => match command {
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
            SopsCommands::Aws {
                access_key_id,
                secret_access_key,
                region,
            } => aws(
                access_key_id.as_deref(),
                secret_access_key.as_deref(),
                region.as_deref(),
            ),
            SopsCommands::Cloudflare { dns_api_token } => cloudflare(dns_api_token.as_deref()),
            SopsCommands::Wikijs {
                db_user,
                db_password,
            } => wikijs(db_user.as_deref(), db_password.as_deref()),
        },
        Commands::Action {
            name,
            working_directory,
        } => match action::generate_github_action(name, working_directory) {
            Ok(_) => println!("GitHub Action workflow generated successfully."),
            Err(e) => eprintln!("Failed to generate GitHub Action workflow: {}", e),
        },
        Commands::Nginx {
            name,
            port,
            ip,
            enabled,
        } => generate_nginx_config(name, port, &ip, enabled),
    }
}
