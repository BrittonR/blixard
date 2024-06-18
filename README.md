# Blixard

**Blixard** is a CLI tool for generating and managing NixOS infrastructure, including configurations for MicroVMs, SSH keys, and encrypted configuration files using `sops`. This tool simplifies the process of setting up and managing NixOS environments and supports integration with GitHub Actions for continuous deployment.

## Features

- Generate SSH keys for projects

- Generate Nix modules for MicroVMs

- Generate `flake.nix` configurations

- Generate host configurations for MicroVMs

- Generate and encrypt configuration files using `sops` for various services (Tailscale, AWS, Cloudflare, Wiki.js)

- Generate GitHub Actions workflows for automated CI/CD

- Generate Nginx configurations and add them to your service list

## Installation

Clone the repository and build the CLI tool using Cargo:

```sh

git clone https://github.com/brittonr/blixard.git

cd blixard

cargo build --release

After building, you can find the executable in the target/release directory.

Usage

Command Line Interface

The CLI provides several commands for different tasks. Below is an overview of the available commands and their usage.

Generate SSH Keys

Generate SSH keys for a project.

sh

blixard generate-ssh-keys [OPTIONS]

project_name (optional): The name of the project.

Generate Nix Module

Generate a Nix module for a MicroVM.

sh

blixard generate-module [OPTIONS]

description (optional): Description of the MicroVM.

root_password (optional): Root password for the MicroVM.

hypervisor (optional): Hypervisor to use (e.g., cloud-hypervisor, qemu, firecracker).

add_tailscale (optional): Whether to add Tailscale configuration (true/false).

Generate Flake Nix

Generate the flake.nix configuration.

sh

blixard generate-flake-nix

Generate Host Config

Generate a host configuration for the MicroVM.

sh

blixard generate-host-config

Generate Sops Configurations

Generate and encrypt configuration files using sops.

Tailscale Configuration

sh

blixard sops tailscale [OPTIONS]

auth_key (optional): Tailscale auth key.

api_key (optional): Tailscale API key.

tailnet (optional): Tailscale tailnet.

exit_node (optional): Tailscale exit node.

AWS Configuration

sh

blixard sops aws [OPTIONS]

access_key_id (optional): AWS access key ID.

secret_access_key (optional): AWS secret access key.

region (optional): AWS region.

Cloudflare Configuration

sh

blixard sops cloudflare [OPTIONS]

dns_api_token (optional): Cloudflare DNS API token.

Wiki.js Configuration

sh

blixard sops wikijs [OPTIONS]

db_user (optional): Wiki.js database user.

db_password (optional): Wiki.js database password.

Generate GitHub Action

Generate a GitHub Action workflow.

sh

blixard generate-action --name <NAME> --working-directory <WORKING_DIRECTORY>

name: The name of the GitHub Action.

working_directory: The working directory for the action script.

Generate Nginx Config

Generate an Nginx configuration and add it to your service list.

sh

blixard generate-nginx-config --name <NAME> --port <PORT> --ip <IP> --enabled <ENABLED>

name: The name of the service.

port: The port number the service listens on.

ip: The IP address of the service.

enabled: A boolean indicating if the service is enabled.

Examples

Generate SSH Keys

sh

blixard generate-ssh-keys --project-name "my_project"

Generate Nix Module

sh

blixard generate-module --description "My MicroVM" --root-password "password123" --hypervisor "qemu" --add-tailscale true

Generate Flake Nix

sh

blixard generate-flake-nix

Generate Host Config

sh

blixard generate-host-config

Generate and Encrypt Tailscale Configuration

sh

blixard sops tailscale --auth_key "auth_key_value" --api_key "api_key_value" --tailnet "my_tailnet" --exit_node "exit_node_value"

Generate GitHub Action

sh

blixard generate-action --name "deploy" --working-directory "./scripts"

Generate Nginx Config

sh

blixard generate-nginx-config --name "my_service" --port 8080 --ip "192.168.1.1" --enabled true

