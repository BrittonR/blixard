use anyhow::Result;
use clap::{Parser, Subcommand};
use color_eyre::eyre;
use tracing::{info, debug};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod node;
mod storage;
mod raft_node;
mod microvm;
mod tailscale;
mod types;

use crate::node::Node;
use crate::types::*;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start a Blixard node
    Node {
        /// Node ID (must be unique in cluster)
        #[arg(long)]
        id: u64,
        
        /// Data directory for storage
        #[arg(long, default_value = "/var/lib/blixard")]
        data_dir: String,
        
        /// Address to bind to
        #[arg(long, default_value = "0.0.0.0:7000")]
        bind: String,
        
        /// Existing node to join (host:port)
        #[arg(long)]
        join: Option<String>,
        
        /// Enable Tailscale discovery
        #[arg(long)]
        tailscale: bool,
    },
    
    /// VM management commands
    Vm {
        #[command(subcommand)]
        command: VmCommands,
    },
}

#[derive(Subcommand, Debug)]
enum VmCommands {
    /// Create a new VM
    Create {
        /// VM name
        name: String,
        
        /// Path to microvm.nix configuration
        #[arg(long)]
        config: String,
        
        /// Number of vCPUs
        #[arg(long, default_value = "2")]
        vcpus: u32,
        
        /// Memory in MB
        #[arg(long, default_value = "512")]
        memory: u32,
    },
    
    /// Start a VM
    Start {
        /// VM name
        name: String,
    },
    
    /// Stop a VM
    Stop {
        /// VM name
        name: String,
    },
    
    /// List all VMs
    List,
    
    /// Show VM status
    Status {
        /// VM name
        name: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize error handling
    color_eyre::install()?;
    
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "blixard=info,raft=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Node { id, data_dir, bind, join, tailscale } => {
            info!("Starting Blixard node {} at {}", id, bind);
            
            let config = NodeConfig {
                id,
                data_dir,
                bind_addr: bind.parse()?,
                join_addr: join.map(|s| s.parse()).transpose()?,
                use_tailscale: tailscale,
            };
            
            let node = Node::new(config).await?;
            node.run().await?;
        }
        
        Commands::Vm { command } => {
            // For now, connect to local node
            // TODO: discover nodes via Tailscale or config
            let client = connect_to_cluster().await?;
            
            match command {
                VmCommands::Create { name, config, vcpus, memory } => {
                    info!("Creating VM: {}", name);
                    let vm_config = VmConfig {
                        name: name.clone(),
                        config_path: config,
                        vcpus,
                        memory,
                    };
                    client.create_vm(vm_config).await?;
                    println!("VM '{}' created successfully", name);
                }
                
                VmCommands::Start { name } => {
                    info!("Starting VM: {}", name);
                    client.start_vm(name.clone()).await?;
                    println!("VM '{}' started", name);
                }
                
                VmCommands::Stop { name } => {
                    info!("Stopping VM: {}", name);
                    client.stop_vm(name.clone()).await?;
                    println!("VM '{}' stopped", name);
                }
                
                VmCommands::List => {
                    let vms = client.list_vms().await?;
                    println!("VMs in cluster:");
                    for (name, status) in vms {
                        println!("  {} - {:?}", name, status);
                    }
                }
                
                VmCommands::Status { name } => {
                    let status = client.get_vm_status(name.clone()).await?;
                    println!("VM '{}' status: {:?}", name, status);
                }
            }
        }
    }
    
    Ok(())
}

async fn connect_to_cluster() -> Result<ClusterClient> {
    // TODO: Implement cluster discovery
    // For now, connect to localhost
    ClusterClient::connect("http://localhost:7000").await
}

// Placeholder for cluster client
struct ClusterClient;

impl ClusterClient {
    async fn connect(_addr: &str) -> Result<Self> {
        todo!("Implement gRPC client")
    }
    
    async fn create_vm(&self, _config: VmConfig) -> Result<()> {
        todo!()
    }
    
    async fn start_vm(&self, _name: String) -> Result<()> {
        todo!()
    }
    
    async fn stop_vm(&self, _name: String) -> Result<()> {
        todo!()
    }
    
    async fn list_vms(&self) -> Result<Vec<(String, VmStatus)>> {
        todo!()
    }
    
    async fn get_vm_status(&self, _name: String) -> Result<VmStatus> {
        todo!()
    }
}