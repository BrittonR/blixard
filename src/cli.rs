use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "MicroVM Tool")]
#[command(version = "1.0")]
#[command(author = "Author Name <email@example.com>")]
#[command(about = "Tool for generating Nix flakes and SSH keys for MicroVMs")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
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
    GenerateSshKeys {
        project_name: Option<String>,
    },
}
