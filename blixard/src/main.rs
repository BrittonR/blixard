use clap::Parser;
use std::net::SocketAddr;

use blixard::{
    BlixardOrchestrator,
    BlixardError, BlixardResult, NodeConfig,
};
use blixard::orchestrator::OrchestratorConfig;
use blixard_core::config_v2::{Config, ConfigBuilder};

mod tui;

#[cfg(not(madsim))]
use blixard_core::grpc_server::start_grpc_server;

#[derive(Parser)]
#[command(name = "blixard")]
#[command(about = "Distributed microVM orchestration platform", long_about = None)]
struct Cli {
    /// Path to configuration file (TOML format)
    #[arg(short, long, global = true)]
    config: Option<String>,
    
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
        
        /// Run node in background as daemon
        #[arg(long)]
        daemon: bool,
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
    /// Launch TUI (Terminal User Interface) for VM management
    Tui,
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
    /// Delete a VM
    Delete {
        #[arg(long)]
        name: String,
    },
    /// Get VM status
    Status {
        #[arg(long)]
        name: String,
    },
    /// View VM logs
    Logs {
        #[arg(long)]
        name: String,
        /// Follow log output (default: true, use --no-follow to disable)
        #[arg(long, default_value = "true")]
        follow: bool,
        /// Don't follow log output (show last N lines and exit)
        #[arg(long, conflicts_with = "follow")]
        no_follow: bool,
        /// Number of lines to show initially
        #[arg(long, short = 'n', default_value = "50")]
        lines: u32,
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
        Commands::Node { id, bind, data_dir, vm_config_dir: _, vm_data_dir: _, mock_vm, vm_backend, peers, daemon } => {
            // Load configuration from file or create from CLI args
            let mut config = if let Some(config_path) = cli.config {
                // Load from TOML file
                let path = std::path::Path::new(&config_path);
                Config::from_file(path)?
            } else {
                // Create from CLI arguments
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
                
                let mut builder = ConfigBuilder::new()
                    .node_id(id)
                    .bind_address(bind)
                    .data_dir(data_dir)
                    .vm_backend(vm_backend_type);
                
                if let Some(addr) = join_addr {
                    builder = builder.join_address(addr);
                }
                
                builder.build()
                    .map_err(|e| BlixardError::ConfigError(format!("Failed to build config: {}", e)))?
            };
            
            // Apply environment variable overrides
            config.apply_env_overrides();
            
            // Validate configuration
            config.validate()?;
            
            // Convert to old NodeConfig for compatibility
            let bind_address: SocketAddr = config.node.bind_address.parse()
                .map_err(|e| BlixardError::ConfigError(
                    format!("Invalid bind address '{}': {}", config.node.bind_address, e)
                ))?;
            
            let node_config = NodeConfig {
                id: config.node.id.unwrap_or(id),
                bind_addr: bind_address,
                data_dir: config.node.data_dir.to_string_lossy().to_string(),
                join_addr: config.cluster.join_address.clone(),
                use_tailscale: false,
                vm_backend: config.node.vm_backend.clone(),
            };
            
            // Create orchestrator configuration
            let orchestrator_config = OrchestratorConfig {
                node_config,
                vm_backend_type: config.node.vm_backend.clone(),
                config: config.clone(),
            };

            #[cfg(not(madsim))]
            {
                if daemon {
                    // Run in daemon mode (clone values for printing since they were moved)
                    println!("🚀 Starting node {} in background (daemon mode)", id);
                    println!("📍 Bind address: {}", config.node.bind_address);
                    println!("📂 Data directory: {}", config.node.data_dir.display());
                    println!("🔧 VM backend: {}", config.node.vm_backend);
                    
                    // Fork process to run in background
                    match unsafe { libc::fork() } {
                        -1 => {
                            return Err(BlixardError::Internal {
                                message: "Failed to fork process".to_string(),
                            });
                        }
                        0 => {
                            // Child process - detach from terminal and run server
                            if unsafe { libc::setsid() } == -1 {
                                eprintln!("Warning: Failed to create new session");
                            }
                            
                            // Create and initialize the orchestrator
                            let mut orchestrator = BlixardOrchestrator::new(orchestrator_config).await?;
                            orchestrator.initialize(config).await?;
                            orchestrator.start().await?;
                            
                            // Get shared state for gRPC server
                            let shared_state = orchestrator.node().shared();
                            let actual_bind_address = orchestrator.bind_address();
                            
                            // Keep orchestrator alive while running gRPC server
                            let _orchestrator = orchestrator;
                            
                            // Start gRPC server (runs indefinitely)
                            match start_grpc_server(shared_state, actual_bind_address).await {
                                Ok(()) => tracing::info!("gRPC server shut down gracefully"),
                                Err(e) => tracing::error!("gRPC server error: {}", e),
                            }
                        }
                        pid => {
                            // Parent process - print info and exit
                            println!("✅ Node {} started in background with PID {}", id, pid);
                            println!("🔍 To check status: cargo run -- cluster status --addr {}", config.node.bind_address);
                            println!("🛑 To stop: kill {}", pid);
                            return Ok(());
                        }
                    }
                } else {
                    // Run in foreground (normal mode)
                    // Create and initialize the orchestrator
                    let mut orchestrator = BlixardOrchestrator::new(orchestrator_config).await?;
                    orchestrator.initialize(config).await?;
                    orchestrator.start().await?;
                    
                    // Get shared state for gRPC server
                    let shared_state = orchestrator.node().shared();
                    let actual_bind_address = orchestrator.bind_address();
                    
                    // Keep orchestrator alive while running gRPC server
                    let _orchestrator = orchestrator;
                    
                    // Start gRPC server
                    match start_grpc_server(shared_state, actual_bind_address).await {
                        Ok(()) => tracing::info!("gRPC server shut down gracefully"),
                        Err(e) => tracing::error!("gRPC server error: {}", e),
                    }
                }
            }
            
            #[cfg(madsim)]
            {
                // In simulation mode, ignore daemon flag and run normally
                // Create and initialize the orchestrator
                let mut orchestrator = BlixardOrchestrator::new(orchestrator_config).await?;
                orchestrator.initialize(config).await?;
                orchestrator.start().await?;
                
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
        Commands::Tui => {
            handle_tui_command().await?;
        }
    }

    Ok(())
}

#[cfg(not(madsim))]
async fn handle_vm_logs(vm_name: &str, follow: bool, lines: u32) -> BlixardResult<()> {
    use tokio::process::Command;
    use tokio::io::{AsyncBufReadExt, BufReader};
    
    let service_name = format!("blixard-vm-{}", vm_name);
    
    if follow {
        println!("Following logs for VM '{}' (Press Ctrl+C to exit)...", vm_name);
        println!("Service: {}", service_name);
        println!("---");
        
        // Use journalctl --follow for live log streaming
        let mut child = Command::new("journalctl")
            .args(&[
                "--user",
                "-u", &service_name,
                "-n", &lines.to_string(),
                "--follow",
                "--no-pager"
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to start journalctl: {}", e),
            })?;
        
        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            
            while let Some(line) = lines.next_line().await.map_err(|e| BlixardError::Internal {
                message: format!("Failed to read log line: {}", e),
            })? {
                println!("{}", line);
            }
        }
        
        // Wait for the child process to finish (it won't unless user interrupts)
        let _ = child.wait().await;
    } else {
        // Show last N lines without following
        let output = Command::new("journalctl")
            .args(&[
                "--user",
                "-u", &service_name,
                "-n", &lines.to_string(),
                "--no-pager"
            ])
            .output()
            .await
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to run journalctl: {}", e),
            })?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("journalctl failed: {}", stderr);
            std::process::exit(1);
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        print!("{}", stdout);
    }
    
    Ok(())
}

#[cfg(not(madsim))]
async fn handle_vm_command(command: VmCommands) -> BlixardResult<()> {
    use blixard_core::proto::{
        cluster_service_client::ClusterServiceClient,
        CreateVmRequest, StartVmRequest, StopVmRequest, DeleteVmRequest, GetVmStatusRequest, ListVmsRequest,
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
        VmCommands::Delete { name } => {
            let request = tonic::Request::new(DeleteVmRequest {
                name: name.clone(),
            });
            
            match client.delete_vm(request).await {
                Ok(response) => {
                    let resp = response.into_inner();
                    if resp.success {
                        println!("Successfully deleted VM '{}'", name);
                        println!("Message: {}", resp.message);
                    } else {
                        eprintln!("Failed to delete VM '{}': {}", name, resp.message);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Error deleting VM '{}': {}", name, e);
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
                        
                        let ip_display = if vm.ip_address.is_empty() {
                            "not assigned".to_string()
                        } else {
                            vm.ip_address.clone()
                        };
                        println!("  IP Address: {}", ip_display);
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
                            let ip_display = if vm.ip_address.is_empty() {
                                "no IP".to_string()
                            } else {
                                vm.ip_address.clone()
                            };
                            
                            println!("  - {}: {:?} ({}vcpu, {}MB) IP: {} on node {}",
                                vm.name,
                                vm.state,
                                vm.vcpus,
                                vm.memory_mb,
                                ip_display,
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
        VmCommands::Logs { name, follow, no_follow, lines } => {
            let should_follow = follow && !no_follow;
            handle_vm_logs(&name, should_follow, lines).await?;
        }
    }
    
    Ok(())
}

#[cfg(madsim)]
async fn handle_vm_logs(_vm_name: &str, _follow: bool, _lines: u32) -> BlixardResult<()> {
    eprintln!("VM logs are not available in simulation mode");
    std::process::exit(1);
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

#[cfg(not(madsim))]
async fn start_server_background(
    node_id: u64,
    bind_addr: String,
    data_dir: String,
    vm_backend: String,
) -> BlixardResult<()> {
    // Parse bind address
    let bind_address = bind_addr.parse::<SocketAddr>()
        .map_err(|e| BlixardError::ConfigError(format!("Invalid bind address: {}", e)))?;
    
    // Create node configuration (same structure as the main node command)
    let node_config = NodeConfig {
        id: node_id,
        bind_addr: bind_address,
        data_dir: data_dir.clone(),
        join_addr: None, // No peers for auto-started server
        use_tailscale: false,
        vm_backend: vm_backend.clone(),
    };
    
    // Create default config
    let config = ConfigBuilder::new()
        .node_id(node_id)
        .bind_address(bind_addr.clone())
        .data_dir(data_dir)
        .vm_backend(vm_backend.clone())
        .build()
        .map_err(|e| BlixardError::ConfigError(format!("Failed to build config: {}", e)))?;
    
    // Create orchestrator configuration
    let orchestrator_config = OrchestratorConfig {
        node_config,
        vm_backend_type: vm_backend,
        config: config.clone(),
    };

    // Create and initialize the orchestrator (same as main node command)
    let mut orchestrator = BlixardOrchestrator::new(orchestrator_config).await?;
    orchestrator.initialize(config).await?;
    orchestrator.start().await?;
    
    // Get shared state for gRPC server
    let shared_state = orchestrator.node().shared();
    let actual_bind_address = orchestrator.bind_address();

    // Keep orchestrator alive while running gRPC server
    let _orchestrator = orchestrator;
    
    // Start gRPC server (this will run indefinitely)
    start_grpc_server(shared_state, actual_bind_address).await
}

#[cfg(not(madsim))]
async fn handle_tui_command() -> BlixardResult<()> {
    use crossterm::{
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::{
        backend::CrosstermBackend,
        Terminal,
    };
    use std::io;

    // Setup terminal
    enable_raw_mode().map_err(|e| BlixardError::Internal {
        message: format!("Failed to enable raw mode: {}", e),
    })?;
    
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|e| {
        BlixardError::Internal {
            message: format!("Failed to setup terminal: {}", e),
        }
    })?;
    
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| BlixardError::Internal {
        message: format!("Failed to create terminal: {}", e),
    })?;

    // Create app and event handler
    let mut app = tui::app::App::new().await?;
    let mut event_handler = tui::events::EventHandler::new(250); // 250ms tick rate

    // Only refresh VM list if we have a server connection
    if app.vm_client.is_some() {
        app.refresh_vm_list().await?;
    }

    // Main loop
    let result = loop {
        // Draw UI
        terminal.draw(|f| tui::ui::render(f, &app)).map_err(|e| {
            BlixardError::Internal {
                message: format!("Failed to draw terminal: {}", e),
            }
        })?;

        // Handle events
        match event_handler.next().await {
            Ok(event) => {
                if let Err(e) = app.handle_event(event).await {
                    eprintln!("Error handling event: {}", e);
                }
                
                if app.should_quit {
                    break Ok(());
                }
            }
            Err(e) => {
                break Err(e);
            }
        }
    };

    // Restore terminal
    disable_raw_mode().map_err(|e| BlixardError::Internal {
        message: format!("Failed to disable raw mode: {}", e),
    })?;
    
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen
    ).map_err(|e| BlixardError::Internal {
        message: format!("Failed to restore terminal: {}", e),
    })?;
    
    terminal.show_cursor().map_err(|e| BlixardError::Internal {
        message: format!("Failed to show cursor: {}", e),
    })?;

    result
}

#[cfg(madsim)]
async fn handle_tui_command() -> BlixardResult<()> {
    eprintln!("TUI is not available in simulation mode");
    std::process::exit(1);
}