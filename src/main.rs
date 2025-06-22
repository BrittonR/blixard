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
    /// Cluster management operations
    Cluster {
        #[command(subcommand)]
        command: ClusterCommands,
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

#[derive(clap::Subcommand)]
enum ClusterCommands {
    /// Join a cluster by connecting to a peer
    Join {
        /// Address of a cluster peer to join through (e.g., 127.0.0.1:7001)
        #[arg(long)]
        peer: String,
        
        /// Local node configuration file or address
        #[arg(long, default_value = "127.0.0.1:7001")]
        local_addr: String,
    },
    /// Leave the current cluster
    Leave {
        /// Node ID to remove from cluster
        #[arg(long)]
        node_id: u64,
        
        /// Local node address
        #[arg(long, default_value = "127.0.0.1:7001")]
        local_addr: String,
    },
    /// Get cluster status
    Status {
        /// Node address to query
        #[arg(long, default_value = "127.0.0.1:7001")]
        addr: String,
    },
}

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    let filter = tracing_subscriber::EnvFilter::from_default_env()
        .add_directive(
            "blixard=info".parse()
                .map_err(|e| blixard::error::BlixardError::ConfigError(
                    format!("Invalid log directive: {}", e)
                ))?
        );
    
    tracing_subscriber::fmt()
        .with_env_filter(filter)
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
        Commands::Cluster { command } => {
            handle_cluster_command(command).await?;
        }
    }

    Ok(())
}

#[cfg(not(madsim))]
async fn handle_cluster_command(command: ClusterCommands) -> BlixardResult<()> {
    use blixard::proto::{
        cluster_service_client::ClusterServiceClient,
        JoinRequest, LeaveRequest, ClusterStatusRequest,
    };
    
    match command {
        ClusterCommands::Join { peer, local_addr } => {
            // Connect to the local node
            let mut client = ClusterServiceClient::connect(format!("http://{}", local_addr))
                .await
                .map_err(|e| blixard::error::BlixardError::Internal { 
                    message: format!("Failed to connect to local node: {}", e)
                })?;
            
            // Parse peer address to get node ID (assuming format nodeID@address)
            let (node_id, bind_address) = if peer.contains('@') {
                let parts: Vec<&str> = peer.split('@').collect();
                if parts.len() != 2 {
                    eprintln!("Invalid peer format. Expected 'nodeID@address' but got '{}'", peer);
                    std::process::exit(1);
                }
                
                let id = match parts[0].parse::<u64>() {
                    Ok(id) if id > 0 => id,
                    Ok(0) => {
                        eprintln!("Invalid node ID: must be greater than 0");
                        std::process::exit(1);
                    }
                    Err(e) => {
                        eprintln!("Failed to parse node ID '{}': {}", parts[0], e);
                        std::process::exit(1);
                    }
                };
                (id, parts[1].to_string())
            } else {
                // When no node ID is specified, we'll let the server assign one
                eprintln!("Note: No node ID specified in peer address. Using server assignment.");
                (0, peer.clone())
            };
            
            // Send join request
            let request = tonic::Request::new(JoinRequest {
                node_id,
                bind_address,
            });
            
            match client.join_cluster(request).await {
                Ok(response) => {
                    let resp = response.into_inner();
                    if resp.success {
                        println!("Successfully joined cluster through peer: {}", peer);
                        println!("Message: {}", resp.message);
                    } else {
                        eprintln!("Failed to join cluster: {}", resp.message);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Error joining cluster: {}", e);
                    std::process::exit(1);
                }
            }
        }
        ClusterCommands::Leave { node_id, local_addr } => {
            // Connect to the local node
            let mut client = ClusterServiceClient::connect(format!("http://{}", local_addr))
                .await
                .map_err(|e| blixard::error::BlixardError::Internal { 
                    message: format!("Failed to connect to local node: {}", e)
                })?;
            
            // Send leave request with the provided node ID
            let request = tonic::Request::new(LeaveRequest {
                node_id,
            });
            
            match client.leave_cluster(request).await {
                Ok(response) => {
                    let resp = response.into_inner();
                    if resp.success {
                        println!("Successfully left the cluster");
                        println!("Message: {}", resp.message);
                    } else {
                        eprintln!("Failed to leave cluster: {}", resp.message);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Error leaving cluster: {}", e);
                    std::process::exit(1);
                }
            }
        }
        ClusterCommands::Status { addr } => {
            // Connect to the node
            let mut client = ClusterServiceClient::connect(format!("http://{}", addr))
                .await
                .map_err(|e| blixard::error::BlixardError::Internal { 
                    message: format!("Failed to connect to node: {}", e)
                })?;
            
            // Get cluster status
            let request = tonic::Request::new(ClusterStatusRequest {});
            
            match client.get_cluster_status(request).await {
                Ok(response) => {
                    let status = response.into_inner();
                    println!("Cluster Status:");
                    println!("  Leader ID: {}", status.leader_id);
                    println!("  Term: {}", status.term);
                    println!("  Nodes in cluster:");
                    for node in &status.nodes {
                        println!("    - Node {}: {} ({})", node.id, node.address, 
                            match node.state {
                                0 => "Unknown",
                                1 => "Follower",
                                2 => "Candidate",
                                3 => "Leader",
                                _ => "Invalid",
                            }
                        );
                    }
                }
                Err(e) => {
                    eprintln!("Error getting cluster status: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
    
    Ok(())
}

#[cfg(madsim)]
async fn handle_cluster_command(_command: ClusterCommands) -> BlixardResult<()> {
    eprintln!("Cluster commands are not available in simulation mode");
    std::process::exit(1);
}