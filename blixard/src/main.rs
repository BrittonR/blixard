use clap::Parser;
use std::net::SocketAddr;

use blixard::{
    BlixardOrchestrator,
    BlixardError, BlixardResult, NodeConfig,
};
use blixard::orchestrator::OrchestratorConfig;

#[cfg(not(madsim))]
use blixard_core::grpc_server::start_grpc_server;

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
        
        /// VM configuration directory
        #[arg(long, default_value = "./vm-configs")]
        vm_config_dir: String,
        
        /// VM data directory 
        #[arg(long, default_value = "./vm-data")]
        vm_data_dir: String,
        
        /// Use mock VM backend for testing
        #[arg(long)]
        mock_vm: bool,
        
        /// VM backend type to use ("mock", "microvm", "docker", etc.)
        #[arg(long, default_value = "microvm")]
        vm_backend: String,
        
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
    /// Reset all data and VMs (clean slate)
    Reset {
        /// Data directory to clean
        #[arg(long, default_value = "./data")]
        data_dir: String,
        
        /// VM configuration directory to clean
        #[arg(long, default_value = "./vm-configs")]
        vm_config_dir: String,
        
        /// VM data directory to clean
        #[arg(long, default_value = "./vm-data")]
        vm_data_dir: String,
        
        /// Force reset without confirmation
        #[arg(long)]
        force: bool,
    },
}

#[derive(clap::Subcommand)]
enum VmCommands {
    /// Create a new VM
    Create {
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "2")]
        vcpus: u32,
        #[arg(long, default_value = "1024")]
        memory: u32,
        #[arg(long, default_value = "")]
        config_path: String,
    },
    /// Start a VM
    Start {
        #[arg(long)]
        name: String,
    },
    /// Stop a VM
    Stop {
        #[arg(long)]
        name: String,
    },
    /// Get VM status
    Status {
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
                .map_err(|e| BlixardError::ConfigError(
                    format!("Invalid log directive: {}", e)
                ))?
        );
    
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .init();

    let cli = Cli::parse();
    
    match cli.command {
        Commands::Node { id, bind, data_dir, vm_config_dir: _, vm_data_dir: _, mock_vm, vm_backend, peers } => {
            // Parse bind address
            let bind_address: SocketAddr = bind.parse()
                .map_err(|e| BlixardError::ConfigError(
                    format!("Invalid bind address '{}': {}", bind, e)
                ))?;

            // Parse join address from peers (use the first peer)
            let join_addr = if let Some(peers_str) = peers {
                let peer_addrs: Vec<&str> = peers_str.split(',').collect();
                if !peer_addrs.is_empty() {
                    // Validate that it's a valid socket address
                    let _: SocketAddr = peer_addrs[0].parse()
                        .map_err(|e| BlixardError::ConfigError(
                            format!("Invalid peer address '{}': {}", peer_addrs[0], e)
                        ))?;
                    Some(peer_addrs[0].to_string())
                } else {
                    None
                }
            } else {
                None
            };

            // Determine VM backend type (mock_vm flag overrides explicit backend selection)
            let vm_backend_type = if mock_vm {
                "mock".to_string()
            } else {
                vm_backend
            };
            
            // Create node configuration
            let node_config = NodeConfig {
                id,
                bind_addr: bind_address,
                data_dir,
                join_addr,
                use_tailscale: false,
                vm_backend: vm_backend_type.clone(),
            };
            
            // Create orchestrator configuration
            let orchestrator_config = OrchestratorConfig {
                node_config,
                vm_backend_type,
            };

            // Create and initialize the orchestrator
            let mut orchestrator = BlixardOrchestrator::new(orchestrator_config).await?;
            orchestrator.initialize().await?;
            orchestrator.start().await?;
            
            // Get shared state for gRPC server
            let shared_state = orchestrator.node().shared();
            let actual_bind_address = orchestrator.bind_address();

            #[cfg(not(madsim))]
            {
                // Keep orchestrator alive while running gRPC server
                let _orchestrator = orchestrator;
                
                // Start gRPC server
                match start_grpc_server(shared_state, actual_bind_address).await {
                    Ok(()) => tracing::info!("gRPC server shut down gracefully"),
                    Err(e) => tracing::error!("gRPC server error: {}", e),
                }
            }
            
            #[cfg(madsim)]
            {
                // In simulation mode, just keep the orchestrator running
                let _orchestrator = orchestrator;
                tracing::info!("Node {} running (gRPC disabled in simulation)", id);
                tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
            }
        }
        Commands::Vm { command } => {
            handle_vm_command(command).await?;
        },
        Commands::Cluster { command } => {
            handle_cluster_command(command).await?;
        }
        Commands::Reset { data_dir, vm_config_dir, vm_data_dir, force } => {
            handle_reset_command(&data_dir, &vm_config_dir, &vm_data_dir, force).await?;
        }
    }

    Ok(())
}

#[cfg(not(madsim))]
async fn handle_vm_command(command: VmCommands) -> BlixardResult<()> {
    use blixard_core::proto::{
        cluster_service_client::ClusterServiceClient,
        CreateVmRequest, StartVmRequest, StopVmRequest, GetVmStatusRequest, ListVmsRequest,
    };
    
    // Default to connecting to local node
    let local_addr = "127.0.0.1:7001";
    
    // Connect to the local node
    let mut client = ClusterServiceClient::connect(format!("http://{}", local_addr))
        .await
        .map_err(|e| BlixardError::Internal { 
            message: format!("Failed to connect to local node at {}: {}", local_addr, e)
        })?;
    
    match command {
        VmCommands::Create { name, vcpus, memory, config_path } => {
            let request = tonic::Request::new(CreateVmRequest {
                name: name.clone(),
                config_path,
                vcpus,
                memory_mb: memory,
            });
            
            match client.create_vm(request).await {
                Ok(response) => {
                    let resp = response.into_inner();
                    if resp.success {
                        println!("Successfully created VM '{}'", name);
                        println!("VM ID: {}", resp.vm_id);
                        println!("Message: {}", resp.message);
                    } else {
                        eprintln!("Failed to create VM '{}': {}", name, resp.message);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Error creating VM '{}': {}", name, e);
                    std::process::exit(1);
                }
            }
        }
        VmCommands::Start { name } => {
            let request = tonic::Request::new(StartVmRequest {
                name: name.clone(),
            });
            
            match client.start_vm(request).await {
                Ok(response) => {
                    let resp = response.into_inner();
                    if resp.success {
                        println!("Successfully started VM '{}'", name);
                        println!("Message: {}", resp.message);
                    } else {
                        eprintln!("Failed to start VM '{}': {}", name, resp.message);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Error starting VM '{}': {}", name, e);
                    std::process::exit(1);
                }
            }
        }
        VmCommands::Stop { name } => {
            let request = tonic::Request::new(StopVmRequest {
                name: name.clone(),
            });
            
            match client.stop_vm(request).await {
                Ok(response) => {
                    let resp = response.into_inner();
                    if resp.success {
                        println!("Successfully stopped VM '{}'", name);
                        println!("Message: {}", resp.message);
                    } else {
                        eprintln!("Failed to stop VM '{}': {}", name, resp.message);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Error stopping VM '{}': {}", name, e);
                    std::process::exit(1);
                }
            }
        }
        VmCommands::Status { name } => {
            let request = tonic::Request::new(GetVmStatusRequest {
                name: name.clone(),
            });
            
            match client.get_vm_status(request).await {
                Ok(response) => {
                    let resp = response.into_inner();
                    if resp.found {
                        let vm = resp.vm_info.unwrap();
                        println!("VM '{}' Status:", name);
                        println!("  State: {:?}", vm.state);
                        println!("  Node ID: {}", vm.node_id);
                        println!("  vCPUs: {}", vm.vcpus);
                        println!("  Memory: {} MB", vm.memory_mb);
                    } else {
                        println!("VM '{}' not found", name);
                    }
                }
                Err(e) => {
                    eprintln!("Error getting VM '{}' status: {}", name, e);
                    std::process::exit(1);
                }
            }
        }
        VmCommands::List => {
            let request = tonic::Request::new(ListVmsRequest {});
            
            match client.list_vms(request).await {
                Ok(response) => {
                    let resp = response.into_inner();
                    if resp.vms.is_empty() {
                        println!("No VMs found");
                    } else {
                        println!("VMs:");
                        for vm in &resp.vms {
                            println!("  - {}: {:?} ({}vcpu, {}MB) on node {}",
                                vm.name,
                                vm.state,
                                vm.vcpus,
                                vm.memory_mb,
                                vm.node_id
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error listing VMs: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
    
    Ok(())
}

#[cfg(madsim)]
async fn handle_vm_command(_command: VmCommands) -> BlixardResult<()> {
    eprintln!("VM commands are not available in simulation mode");
    std::process::exit(1);
}

#[cfg(not(madsim))]
async fn handle_cluster_command(command: ClusterCommands) -> BlixardResult<()> {
    use blixard_core::proto::{
        cluster_service_client::ClusterServiceClient,
        JoinRequest, LeaveRequest, ClusterStatusRequest,
    };
    
    match command {
        ClusterCommands::Join { peer, local_addr } => {
            // Connect to the local node
            let mut client = ClusterServiceClient::connect(format!("http://{}", local_addr))
                .await
                .map_err(|e| BlixardError::Internal { 
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
                    Ok(0) => {
                        eprintln!("Invalid node ID: must be greater than 0");
                        std::process::exit(1);
                    }
                    Ok(id) => id,
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
                .map_err(|e| BlixardError::Internal { 
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
                .map_err(|e| BlixardError::Internal { 
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

async fn handle_reset_command(
    data_dir: &str,
    vm_config_dir: &str, 
    vm_data_dir: &str,
    force: bool
) -> BlixardResult<()> {
    use std::path::Path;
    use std::io::{self, Write};
    
    println!("🚨 RESET WARNING");
    println!("================");
    println!("This will completely remove all blixard data and VMs:");
    println!("  📂 Data directory: {}", data_dir);
    println!("  📂 VM configs: {}", vm_config_dir);  
    println!("  📂 VM data: {}", vm_data_dir);
    println!("  🗃️  Database files");
    println!("  🖥️  All running VMs will be stopped");
    println!();
    
    if !force {
        print!("❓ Are you sure you want to continue? (y/N): ");
        io::stdout().flush().unwrap();
        
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let input = input.trim().to_lowercase();
        
        if input != "y" && input != "yes" {
            println!("❌ Reset cancelled");
            return Ok(());
        }
    }
    
    println!("🔄 Starting reset process...");
    
    // Note: Processes should be stopped manually before running reset
    println!("💡 Note: Please ensure blixard nodes and VMs are stopped before running reset");
    println!("   You can stop them with: pkill -f blixard && pkill -f microvm");
    println!();
    
    // Remove directories
    let dirs_to_remove = vec![
        (data_dir, "Data directory"),
        (vm_config_dir, "VM configurations"),
        (vm_data_dir, "VM data"),
    ];
    
    for (dir_path, description) in dirs_to_remove {
        if Path::new(dir_path).exists() {
            println!("🗑️  Removing {}...", description);
            match tokio::fs::remove_dir_all(dir_path).await {
                Ok(_) => println!("✅ Removed {}", dir_path),
                Err(e) => println!("⚠️  Warning: Failed to remove {}: {}", dir_path, e),
            }
        } else {
            println!("ℹ️  {} does not exist: {}", description, dir_path);
        }
    }
    
    // Remove specific database files in current directory
    let db_patterns = vec!["*.redb", "*.redb-lock", "db.redb*"];
    println!("🗑️  Removing database files...");
    
    for pattern in db_patterns {
        match glob::glob(pattern) {
            Ok(entries) => {
                for entry in entries {
                    match entry {
                        Ok(path) => {
                            if let Err(e) = tokio::fs::remove_file(&path).await {
                                println!("⚠️  Warning: Failed to remove {}: {}", path.display(), e);
                            } else {
                                println!("✅ Removed {}", path.display());
                            }
                        }
                        Err(e) => println!("⚠️  Warning: Failed to process pattern {}: {}", pattern, e),
                    }
                }
            }
            Err(e) => println!("⚠️  Warning: Failed to search for {}: {}", pattern, e),
        }
    }
    
    // Clean up temporary VM artifacts
    println!("🗑️  Cleaning temporary VM artifacts...");
    let temp_patterns = vec!["/tmp/*-console.sock", "/tmp/microvm-*"];
    
    for pattern in temp_patterns {
        match glob::glob(pattern) {
            Ok(entries) => {
                for entry in entries {
                    match entry {
                        Ok(path) => {
                            if let Err(e) = tokio::fs::remove_file(&path).await {
                                println!("⚠️  Warning: Failed to remove {}: {}", path.display(), e);
                            } else {
                                println!("✅ Removed {}", path.display());
                            }
                        }
                        Err(_) => {}, // Ignore glob errors for optional cleanup
                    }
                }
            }
            Err(_) => {}, // Ignore glob errors for optional cleanup
        }
    }
    
    println!();
    println!("✨ Reset complete! Blixard is now in a clean state.");
    println!("💡 You can now start fresh with: cargo run -- node --id 1 --bind 127.0.0.1:7001");
    
    Ok(())
}