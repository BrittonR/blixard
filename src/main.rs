use clap::Parser;
use std::net::SocketAddr;
use blixard::{
    error::BlixardResult,
    node::Node,
    types::NodeConfig,
};

#[cfg(not(madsim))]
use blixard::grpc_server::start_grpc_server;

#[derive(Parser)]
#[command(name = "blixard")]
#[command(about = "Distributed microVM orchestration platform", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Node operations
    Node {
        /// Node ID (must be unique in cluster)
        #[arg(long)]
        id: u64,
        
        /// Bind address for gRPC server (e.g., 127.0.0.1:7001)
        #[arg(long)]
        bind: String,
        
        /// Data directory for node storage
        #[arg(long, default_value = "./data")]
        data_dir: String,
        
        /// Cluster peers to join (comma-separated addresses)
        #[arg(long)]
        peers: Option<String>,
    },
    /// VM operations
    Vm {
        #[command(subcommand)]
        command: VmCommands,
    },
}

#[derive(clap::Subcommand)]
enum VmCommands {
    /// Create a new VM
    Create {
        #[arg(long)]
        name: String,
    },
    /// List all VMs
    List,
}

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("blixard=info".parse().unwrap())
        )
        .init();

    let cli = Cli::parse();
    
    match cli.command {
        Commands::Node { id, bind, data_dir, peers } => {
            // Parse bind address
            let bind_address: SocketAddr = bind.parse()
                .map_err(|e| blixard::error::BlixardError::ConfigError(
                    format!("Invalid bind address '{}': {}", bind, e)
                ))?;

            // Parse join address from peers (use the first peer)
            let join_addr = if let Some(peers_str) = peers {
                let peer_addrs: Vec<&str> = peers_str.split(',').collect();
                if !peer_addrs.is_empty() {
                    // Validate that it's a valid socket address
                    let _: SocketAddr = peer_addrs[0].parse()
                        .map_err(|e| blixard::error::BlixardError::ConfigError(
                            format!("Invalid peer address '{}': {}", peer_addrs[0], e)
                        ))?;
                    Some(peer_addrs[0].to_string())
                } else {
                    None
                }
            } else {
                None
            };

            // Create node configuration
            let config = NodeConfig {
                id,
                bind_addr: bind_address,
                data_dir,
                join_addr,
                use_tailscale: false,
            };

            // Create and initialize the node
            let mut node = Node::new(config);
            node.initialize().await?;
            
            // Start the node (it spawns its own background tasks)
            node.start().await?;
            
            // Get shared state for gRPC server
            let shared_state = node.shared();
            #[cfg(madsim)]
            let node_id = node.get_id();

            #[cfg(not(madsim))]
            {
                // Keep node alive while running gRPC server
                let _node = node;
                
                // Start gRPC server
                match start_grpc_server(shared_state, bind_address).await {
                    Ok(()) => tracing::info!("gRPC server shut down gracefully"),
                    Err(e) => tracing::error!("gRPC server error: {}", e),
                }
            }
            
            #[cfg(madsim)]
            {
                // In simulation mode, just keep the node running
                let _node = node;
                tracing::info!("Node {} running (gRPC disabled in simulation)", node_id);
                tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
            }
        }
        Commands::Vm { command } => match command {
            VmCommands::Create { name } => {
                eprintln!("VM creation not yet implemented (name: {})", name);
                eprintln!("Please use the gRPC API or wait for client implementation");
                std::process::exit(1);
            }
            VmCommands::List => {
                eprintln!("VM listing not yet implemented");
                eprintln!("Please use the gRPC API or wait for client implementation");
                std::process::exit(1);
            }
        },
    }

    Ok(())
}