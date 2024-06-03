use aws::generate_aws_config;
use clap::{Parser, Subcommand};
use cloudflare::generate_cloudflare_config;
use flake::{generate_flake, generate_module};
use ssh_keys::generate_ssh_keys;
use tailscale::generate_tailscale_config;
use utils::{get_hypervisor_input, get_input, get_input_bool};

mod aws;
mod cloudflare;
mod flake;
mod ssh_keys;
mod tailscale;
mod utils;

#[derive(Parser)]
#[command(
    name = "MicroVM Tool",
    version = "1.0",
    author = "Author Name <email@example.com>",
    about = "Tool for generating Nix flakes and SSH keys for MicroVMs"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Generate a Nix flake for a MicroVM")]
    GenerateFlake {
        description: Option<String>,
        hostname: Option<String>,
        root_password: Option<String>,
        image: Option<String>,
        image_size: Option<String>,
        share_source: Option<String>,
        hypervisor: Option<String>,
        socket: Option<String>,
    },
    #[command(about = "Generate SSH keys for a project")]
    GenerateSshKeys { project_name: Option<String> },
    #[command(about = "Generate a Nix module for a MicroVM")]
    GenerateModule {
        description: Option<String>,
        hostname: Option<String>,
        root_password: Option<String>,
        hypervisor: Option<String>,
        add_tailscale: Option<bool>,
    },
    #[command(about = "Generate a Tailscale configuration file and encrypt it with sops")]
    GenerateTailscaleConfig {
        auth_key: String,
        api_key: String,
        tailnet: String,
        exit_node: String,
    },
    #[command(about = "Generate an AWS configuration file and encrypt it with sops")]
    GenerateAwsConfig {
        access_key_id: String,
        secret_access_key: String,
        region: String,
    },
    #[command(about = "Generate a Cloudflare configuration file and encrypt it with sops")]
    GenerateCloudflareConfig { dns_api_token: String },
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::GenerateFlake {
            description,
            hostname,
            root_password,
            image,
            image_size,
            share_source,
            hypervisor,
            socket,
        } => {
            let microvm_name = get_input(&None, "MicroVM name");
            let description = get_input(
                &Some(format!("MicroVM description for {}", microvm_name)),
                "Description",
            );
            let hostname = get_input(&Some(microvm_name.clone()), "Hostname");
            let root_password = get_input(root_password, "Root Password");
            let image = get_input(image, "Image");
            let image_size = get_input(image_size, "Image Size");
            let share_source = get_input(share_source, "Share Source");
            let hypervisor = get_hypervisor_input(hypervisor);
            let socket = get_input(socket, "Socket");
            let add_defaults = get_input_bool("Add base config defaults?");
            let add_tailscale = get_input_bool("Add Tailscale configuration?");

            generate_flake(
                &microvm_name,
                &description,
                &hostname,
                &root_password,
                &image,
                &image_size,
                &share_source,
                &hypervisor,
                &socket,
                add_defaults,
                add_tailscale,
            )
        }
        Commands::GenerateSshKeys { project_name } => {
            let project_name = get_input(project_name, "Project Name");
            generate_ssh_keys(&project_name)
        }
        Commands::GenerateModule {
            description,
            hostname,
            root_password,
            hypervisor,
            add_tailscale,
        } => {
            let microvm_name = get_input(&None, "MicroVM name");
            let description = get_input(
                &Some(format!("MicroVM description for {}", microvm_name)),
                "Description",
            );
            let hostname = get_input(&Some(microvm_name.clone()), "Hostname");
            let root_password = get_input(root_password, "Root Password");
            let hypervisor = get_hypervisor_input(hypervisor);
            let add_tailscale = get_input_bool("Add Tailscale configuration?");

            generate_module(
                &microvm_name,
                &description,
                &hostname,
                &root_password,
                &hypervisor,
                add_tailscale,
            )
        }
        Commands::GenerateTailscaleConfig {
            auth_key,
            api_key,
            tailnet,
            exit_node,
        } => generate_tailscale_config(auth_key, api_key, tailnet, exit_node),
        Commands::GenerateAwsConfig {
            access_key_id,
            secret_access_key,
            region,
        } => generate_aws_config(access_key_id, secret_access_key, region),
        Commands::GenerateCloudflareConfig { dns_api_token } => {
            generate_cloudflare_config(dns_api_token)
        }
    }
}
