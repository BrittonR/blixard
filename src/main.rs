mod cli;
mod flake;
mod ssh_keys;
mod utils;

use clap::Parser;
use cli::{Cli, Commands};
use flake::generate_flake;
use ssh_keys::generate_ssh_keys;

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
            let description = utils::get_input(description, "Description");
            let hostname = utils::get_input(hostname, "Hostname");
            let root_password = utils::get_input(root_password, "Root Password");
            let image = utils::get_input(image, "Image");
            let image_size = utils::get_input(image_size, "Image Size");
            let share_source = utils::get_input(share_source, "Share Source");
            let hypervisor = utils::get_input(hypervisor, "Hypervisor");
            let socket = utils::get_input(socket, "Socket");
            let add_defaults = utils::get_input_bool("Add base config defaults?");
            let add_tailscale = utils::get_input_bool("Add Tailscale configuration?");

            generate_flake(
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
            let project_name = utils::get_input(project_name, "Project Name");
            generate_ssh_keys(&project_name)
        }
    }
}
