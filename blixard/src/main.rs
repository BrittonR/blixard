use clap::Parser;
use std::net::SocketAddr;

use blixard::orchestrator::OrchestratorConfig;
use blixard::{BlixardError, BlixardOrchestrator, BlixardResult, NodeConfig};
use blixard_core::config::{Config, ConfigBuilder};
use daemonize::Daemonize;

mod client;
mod node_discovery;
mod tui;

#[cfg(not(madsim))]
use blixard_core::transport::iroh_service_runner::start_iroh_services;

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

        /// HTTP bootstrap address to join cluster (e.g., http://127.0.0.1:8081)
        #[arg(long)]
        join_address: Option<String>,

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
    /// Security operations (certificates, tokens, etc.)
    Security {
        #[command(subcommand)]
        command: SecurityCommands,
    },
    /// IP pool management operations
    IpPool {
        #[command(subcommand)]
        command: IpPoolCommands,
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
    Tui {},
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
    /// VM health monitoring operations
    Health {
        #[command(subcommand)]
        command: VmHealthCommands,
    },
}

#[derive(clap::Subcommand)]
enum VmHealthCommands {
    /// Get current health status of a VM
    Status {
        #[arg(long)]
        name: String,
    },
    /// Add a health check to a VM
    Add {
        #[arg(long)]
        vm_name: String,
        #[arg(long)]
        check_name: String,
        #[arg(long)]
        check_type: String, // http, tcp, script, console, process, guest-agent
        #[arg(long)]
        weight: Option<f32>,
        #[arg(long)]
        critical: bool,
        /// Type-specific configuration in JSON format
        #[arg(long)]
        config: String,
    },
    /// List health checks for a VM
    List {
        #[arg(long)]
        name: String,
    },
    /// Remove a health check from a VM
    Remove {
        #[arg(long)]
        vm_name: String,
        #[arg(long)]
        check_name: String,
    },
    /// Enable/disable health monitoring for a VM
    Toggle {
        #[arg(long)]
        name: String,
        #[arg(long)]
        enable: bool,
    },
    /// Configure recovery policy for a VM
    Recovery {
        #[arg(long)]
        name: String,
        #[arg(long)]
        enable: Option<bool>,
        #[arg(long)]
        max_retries: Option<u32>,
        #[arg(long)]
        retry_delay_secs: Option<u64>,
        #[arg(long)]
        failure_threshold: Option<u32>,
    },
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
    /// Export cluster state
    Export {
        /// Output file path
        #[arg(short, long)]
        output: std::path::PathBuf,

        /// Cluster name
        #[arg(short, long)]
        cluster_name: String,

        /// Include VM images in export
        #[arg(long)]
        include_images: bool,

        /// Include telemetry data (logs, metrics)
        #[arg(long)]
        include_telemetry: bool,

        /// Skip compression
        #[arg(long)]
        no_compress: bool,

        /// Share via P2P network
        #[arg(long)]
        p2p_share: bool,

        /// Node ID
        #[arg(long, default_value = "1")]
        node_id: u64,

        /// Data directory
        #[arg(long, default_value = "./data")]
        data_dir: String,
    },
    /// Import cluster state
    Import {
        /// Input file path or P2P ticket
        #[arg(short, long)]
        input: String,

        /// Merge with existing state instead of replacing
        #[arg(short, long)]
        merge: bool,

        /// Import from P2P ticket
        #[arg(long)]
        p2p: bool,

        /// Node ID
        #[arg(long, default_value = "1")]
        node_id: u64,

        /// Data directory
        #[arg(long, default_value = "./data")]
        data_dir: String,
    },
    /// IP pool management
    IpPool {
        #[command(subcommand)]
        command: IpPoolCommands,
    },
}

#[derive(clap::Subcommand)]
enum SecurityCommands {
    /// Generate certificates for cluster TLS/mTLS
    GenCerts {
        /// Cluster name
        #[arg(short, long)]
        cluster_name: String,

        /// Node names (comma-separated)
        #[arg(short, long)]
        nodes: String,

        /// Client names for authentication (comma-separated)
        #[arg(long, default_value = "admin,operator")]
        clients: String,

        /// Output directory for certificates
        #[arg(short, long, default_value = "./certs")]
        output_dir: String,

        /// Certificate validity in days
        #[arg(long, default_value = "365")]
        validity_days: i64,

        /// Key algorithm (rsa2048, rsa4096, ecdsa-p256, ecdsa-p384)
        #[arg(long, default_value = "ecdsa-p256")]
        key_algorithm: String,
    },
    /// Generate an API token for authentication
    GenToken {
        /// User/service name
        #[arg(short, long)]
        user: String,

        /// Permissions (comma-separated: cluster-read,cluster-write,vm-read,vm-write,task-read,task-write,metrics-read,admin)
        #[arg(
            short,
            long,
            default_value = "cluster-read,vm-read,task-read,metrics-read"
        )]
        permissions: String,

        /// Token validity in days (0 for no expiration)
        #[arg(long, default_value = "30")]
        validity_days: u64,

        /// Node address to connect to
        #[arg(long, default_value = "127.0.0.1:7001")]
        addr: String,
    },
}

#[derive(clap::Subcommand)]
enum IpPoolCommands {
    /// Create a new IP pool
    Create {
        /// Pool name
        #[arg(short, long)]
        name: String,
        
        /// Subnet (e.g., 10.0.0.0/24)
        #[arg(short, long)]
        subnet: String,
        
        /// Gateway IP address
        #[arg(short, long)]
        gateway: String,
        
        /// DNS servers (comma-separated)
        #[arg(long, default_value = "8.8.8.8,8.8.4.4")]
        dns: String,
        
        /// Start of allocation range
        #[arg(long)]
        start_ip: String,
        
        /// End of allocation range
        #[arg(long)]
        end_ip: String,
        
        /// VLAN ID (optional)
        #[arg(long)]
        vlan: Option<u16>,
        
        /// Topology hint (e.g., datacenter/zone)
        #[arg(long)]
        topology: Option<String>,
        
        /// Node address to connect to
        #[arg(long, default_value = "127.0.0.1:7001")]
        addr: String,
    },
    /// List IP pools
    List {
        /// Node address to connect to
        #[arg(long, default_value = "127.0.0.1:7001")]
        addr: String,
    },
    /// Show IP pool details
    Show {
        /// Pool ID
        #[arg(short, long)]
        id: u64,
        
        /// Node address to connect to
        #[arg(long, default_value = "127.0.0.1:7001")]
        addr: String,
    },
    /// Delete an IP pool
    Delete {
        /// Pool ID
        #[arg(short, long)]
        id: u64,
        
        /// Node address to connect to
        #[arg(long, default_value = "127.0.0.1:7001")]
        addr: String,
    },
    /// Enable or disable a pool
    Enable {
        /// Pool ID
        #[arg(short, long)]
        id: u64,
        
        /// Enable (true) or disable (false)
        #[arg(long)]
        enable: bool,
        
        /// Node address to connect to
        #[arg(long, default_value = "127.0.0.1:7001")]
        addr: String,
    },
    /// Show pool statistics
    Stats {
        /// Pool ID (optional, show all if not specified)
        #[arg(short, long)]
        id: Option<u64>,
        
        /// Node address to connect to
        #[arg(long, default_value = "127.0.0.1:7001")]
        addr: String,
    },
}

#[tokio::main]
async fn main() -> BlixardResult<()> {
    // Initialize logging
    let filter = tracing_subscriber::EnvFilter::from_default_env().add_directive(
        "blixard=info"
            .parse()
            .map_err(|e| BlixardError::ConfigError(format!("Invalid log directive: {}", e)))?,
    );

    tracing_subscriber::fmt().with_env_filter(filter).init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Node {
            id,
            bind,
            data_dir,
            vm_config_dir: _,
            vm_data_dir: _,
            mock_vm,
            vm_backend,
            peers,
            join_address,
            daemon,
        } => {
            // Load configuration from file or create from CLI args
            let mut config = if let Some(config_path) = cli.config {
                // Load from TOML file
                let path = std::path::Path::new(&config_path);
                Config::from_file(path)?
            } else {
                // Create from CLI arguments
                // Prefer join_address over peers for backward compatibility
                let join_addr = if let Some(addr) = join_address {
                    Some(addr)
                } else if let Some(peers_str) = peers {
                    let peer_addrs: Vec<&str> = peers_str.split(',').collect();
                    if !peer_addrs.is_empty() {
                        // Validate that it's a valid socket address
                        let _: SocketAddr = peer_addrs[0].parse().map_err(|e| {
                            BlixardError::ConfigError(format!(
                                "Invalid peer address '{}': {}",
                                peer_addrs[0], e
                            ))
                        })?;
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

                builder.build().map_err(|e| {
                    BlixardError::ConfigError(format!("Failed to build config: {}", e))
                })?
            };

            // Apply environment variable overrides
            config.apply_env_overrides();

            // Validate configuration
            config.validate()?;

            // Convert to old NodeConfig for compatibility
            let bind_address: SocketAddr = config.node.bind_address.parse().map_err(|e| {
                BlixardError::ConfigError(format!(
                    "Invalid bind address '{}': {}",
                    config.node.bind_address, e
                ))
            })?;

            let node_config = NodeConfig {
                id: config.node.id.unwrap_or(id),
                bind_addr: bind_address,
                data_dir: config.node.data_dir.to_string_lossy().to_string(),
                join_addr: config.cluster.join_address.clone(),
                use_tailscale: false,
                vm_backend: config.node.vm_backend.clone(),
                transport_config: config.transport.clone(),
                topology: Default::default(),
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
                    // Run in daemon mode (safe implementation)
                    println!("🚀 Starting node {} in background (daemon mode)", id);
                    println!("📍 Bind address: {}", config.node.bind_address);
                    println!("📂 Data directory: {}", config.node.data_dir.display());
                    println!("🔧 VM backend: {}", config.node.vm_backend);

                    // Create PID file path in data directory
                    let pid_file = config.node.data_dir.join(format!("blixard-node-{}.pid", id));
                    let log_file = config.node.data_dir.join(format!("blixard-node-{}.log", id));
                    
                    // Create daemon configuration
                    let daemonize = Daemonize::new()
                        .pid_file(&pid_file)
                        .working_directory(&config.node.data_dir)
                        .umask(0o027)
                        .stdout(std::fs::File::create(&log_file).map_err(|e| {
                            BlixardError::Internal {
                                message: format!("Failed to create log file: {}", e),
                            }
                        })?)
                        .stderr(std::fs::File::create(&log_file).map_err(|e| {
                            BlixardError::Internal {
                                message: format!("Failed to create log file: {}", e),
                            }
                        })?);

                    // Fork to daemon safely
                    match daemonize.start() {
                        Ok(_) => {
                            // We're now in the daemon process
                            println!("✅ Node {} started in background", id);
                            println!("📝 PID file: {}", pid_file.display());
                            println!("📝 Log file: {}", log_file.display());
                            println!("🔍 To check status: cargo run -- cluster status --addr {}", config.node.bind_address);
                        }
                        Err(e) => {
                            return Err(BlixardError::Internal {
                                message: format!("Failed to daemonize: {}", e),
                            });
                        }
                    }
                }
                
                // Run the server (both daemon and foreground modes end up here)
                // Create and initialize the orchestrator
                let mut orchestrator = BlixardOrchestrator::new(orchestrator_config).await?;
                orchestrator.initialize(config.clone()).await?;
                orchestrator.start().await?;

                // Get shared state for gRPC server
                let shared_state = orchestrator.node().shared();
                let actual_bind_address = orchestrator.bind_address();

                // Keep orchestrator alive while running server
                let _orchestrator = orchestrator;

                // Start Iroh services
                let handle = start_iroh_services(shared_state, actual_bind_address).await?;

                // Wait for the service to complete
                match handle.await {
                    Ok(()) => tracing::info!("Services shut down gracefully"),
                    Err(e) => tracing::error!("Service error: {}", e),
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
        }
        Commands::Cluster { command } => {
            handle_cluster_command(command).await?;
        }
        Commands::Security { command } => {
            handle_security_command(command).await?;
        }
        Commands::Reset {
            data_dir,
            vm_config_dir,
            vm_data_dir,
            force,
        } => {
            handle_reset_command(&data_dir, &vm_config_dir, &vm_data_dir, force).await?;
        }
        Commands::Tui {} => {
            handle_tui_command().await?;
        }
        Commands::IpPool { command } => {
            handle_ip_pool_command(command).await?;
        }
    }

    Ok(())
}

#[cfg(not(madsim))]
async fn handle_vm_logs(vm_name: &str, follow: bool, lines: u32) -> BlixardResult<()> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::Command;

    let service_name = format!("blixard-vm-{}", vm_name);

    if follow {
        println!(
            "Following logs for VM '{}' (Press Ctrl+C to exit)...",
            vm_name
        );
        println!("Service: {}", service_name);
        println!("---");

        // Use journalctl --follow for live log streaming
        let mut child = Command::new("journalctl")
            .args(&[
                "--user",
                "-u",
                &service_name,
                "-n",
                &lines.to_string(),
                "--follow",
                "--no-pager",
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

            while let Some(line) = lines
                .next_line()
                .await
                .map_err(|e| BlixardError::Internal {
                    message: format!("Failed to read log line: {}", e),
                })?
            {
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
                "-u",
                &service_name,
                "-n",
                &lines.to_string(),
                "--no-pager",
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
    use crate::client::UnifiedClient;
    use blixard_core::iroh_types::{
        CreateVmRequest, DeleteVmRequest, GetVmStatusRequest, ListVmsRequest, StartVmRequest,
        StopVmRequest,
    };

    // Check for BLIXARD_NODE_ADDR environment variable first
    let node_addr =
        std::env::var("BLIXARD_NODE_ADDR").unwrap_or_else(|_| "127.0.0.1:7001".to_string());

    // Connect to the node using unified client (supports both direct addresses and registry files)
    let mut client = UnifiedClient::new(&node_addr)
        .await
        .map_err(|e| BlixardError::Internal {
            message: format!("Failed to connect to node at {}: {}", node_addr, e),
        })?;

    match command {
        VmCommands::Create {
            name,
            vcpus,
            memory,
            config_path,
        } => {
            let request = CreateVmRequest {
                name: name.clone(),
                config_path,
                vcpus,
                memory_mb: memory,
            };

            match client.create_vm(request).await {
                Ok(resp) => {
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
            let request = StartVmRequest { name: name.clone() };

            match client.start_vm(request).await {
                Ok(resp) => {
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
            let request = StopVmRequest { name: name.clone() };

            match client.stop_vm(request).await {
                Ok(resp) => {
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
            let request = DeleteVmRequest { name: name.clone() };

            match client.delete_vm(request).await {
                Ok(resp) => {
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
            let request = GetVmStatusRequest { name: name.clone() };

            match client.get_vm_status(request).await {
                Ok(resp) => {
                    if resp.found {
                        let vm = match resp.vm_info {
                            Some(vm) => vm,
                            None => {
                                eprintln!("Error: VM info not found in response");
                                std::process::exit(1);
                            }
                        };
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
            let request = ListVmsRequest {};

            match client.list_vms(request).await {
                Ok(resp) => {
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

                            println!(
                                "  - {}: {:?} ({}vcpu, {}MB) IP: {} on node {}",
                                vm.name, vm.state, vm.vcpus, vm.memory_mb, ip_display, vm.node_id
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
        VmCommands::Logs {
            name,
            follow,
            no_follow,
            lines,
        } => {
            let should_follow = follow && !no_follow;
            handle_vm_logs(&name, should_follow, lines).await?;
        }
        VmCommands::Health { command } => {
            handle_vm_health_command(command, &mut client).await?;
        }
    }

    Ok(())
}

#[cfg(not(madsim))]
async fn handle_vm_health_command(
    command: VmHealthCommands,
    client: &mut crate::client::UnifiedClient,
) -> BlixardResult<()> {
    use blixard_core::vm_health_types::HealthCheckType;

    match command {
        VmHealthCommands::Status { name } => {
            match client.get_vm_health_status(name.clone()).await {
                Ok(response) => {
                    if let Some(status) = response.health_status {
                        println!("VM '{}' Health Status:", name);
                        println!("  State: {:?}", status.state);
                        println!("  Score: {:.1}%", status.score);
                        if let Some(last_healthy_secs) = status.last_healthy_at_secs {
                            let last_healthy = std::time::UNIX_EPOCH
                                + std::time::Duration::from_secs(last_healthy_secs as u64);
                            println!(
                                "  Last Healthy: {:?}",
                                last_healthy.elapsed().unwrap_or_default()
                            );
                        }
                        if status.consecutive_failures > 0 {
                            println!("  Consecutive Failures: {}", status.consecutive_failures);
                        }

                        if !status.check_results.is_empty() {
                            println!("\nHealth Check Results:");
                            for result in &status.check_results {
                                let status_icon = if result.success { "✓" } else { "✗" };
                                println!(
                                    "  {} {}: {} ({}ms)",
                                    status_icon,
                                    result.check_name,
                                    result.message,
                                    result.duration_ms
                                );
                            }
                        }
                    } else {
                        println!("No health status available for VM '{}'", name);
                    }
                }
                Err(e) => {
                    eprintln!("Error getting health status for VM '{}': {}", name, e);
                    std::process::exit(1);
                }
            }
        }
        VmHealthCommands::Add {
            vm_name,
            check_name,
            check_type,
            weight,
            critical,
            config,
        } => {
            // Parse the health check type and config
            let health_check_type = match parse_health_check_type(&check_type, &config) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Invalid health check configuration: {}", e);
                    std::process::exit(1);
                }
            };

            use blixard_core::vm_health_types::{HealthCheck, HealthCheckPriority};

            let health_check = HealthCheck {
                name: check_name.clone(),
                check_type: health_check_type,
                weight: weight.unwrap_or(1.0),
                critical,
                priority: HealthCheckPriority::Quick,
                recovery_escalation: None,
                failure_threshold: 3,
            };

            match client
                .add_vm_health_check(vm_name.clone(), health_check)
                .await
            {
                Ok(response) => {
                    if response.success {
                        println!(
                            "Successfully added health check '{}' to VM '{}'",
                            check_name, vm_name
                        );
                    } else {
                        eprintln!("Failed to add health check: {}", response.message);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Error adding health check: {}", e);
                    std::process::exit(1);
                }
            }
        }
        VmHealthCommands::List { name } => match client.list_vm_health_checks(name.clone()).await {
            Ok(response) => {
                if response.health_checks.is_empty() {
                    println!("No health checks configured for VM '{}'", name);
                } else {
                    println!("Health checks for VM '{}':", name);
                    for check in &response.health_checks {
                        println!(
                            "  - {} ({})",
                            check.name,
                            match &check.check_type {
                                HealthCheckType::Http { .. } => "HTTP",
                                HealthCheckType::Tcp { .. } => "TCP",
                                HealthCheckType::Script { .. } => "Script",
                                HealthCheckType::Console { .. } => "Console",
                                HealthCheckType::Process { .. } => "Process",
                                HealthCheckType::GuestAgent { .. } => "Guest Agent",
                            }
                        );
                        println!("    Weight: {}, Critical: {}", check.weight, check.critical);
                    }
                }
            }
            Err(e) => {
                eprintln!("Error listing health checks: {}", e);
                std::process::exit(1);
            }
        },
        VmHealthCommands::Remove {
            vm_name,
            check_name,
        } => {
            match client
                .remove_vm_health_check(vm_name.clone(), check_name.clone())
                .await
            {
                Ok(response) => {
                    if response.success {
                        println!(
                            "Successfully removed health check '{}' from VM '{}'",
                            check_name, vm_name
                        );
                    } else {
                        eprintln!("Failed to remove health check: {}", response.message);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Error removing health check: {}", e);
                    std::process::exit(1);
                }
            }
        }
        VmHealthCommands::Toggle { name, enable } => {
            match client
                .toggle_vm_health_monitoring(name.clone(), enable)
                .await
            {
                Ok(response) => {
                    if response.success {
                        let action = if enable { "enabled" } else { "disabled" };
                        println!(
                            "Successfully {} health monitoring for VM '{}'",
                            action, name
                        );
                    } else {
                        eprintln!("Failed to toggle health monitoring: {}", response.message);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Error toggling health monitoring: {}", e);
                    std::process::exit(1);
                }
            }
        }
        VmHealthCommands::Recovery {
            name,
            enable,
            max_retries,
            retry_delay_secs,
            failure_threshold: _,
        } => {
            use blixard_core::vm_auto_recovery::RecoveryPolicy;
            use std::time::Duration;

            let max_restart_attempts = max_retries.unwrap_or(3);
            let restart_delay_secs = retry_delay_secs.unwrap_or(30);
            let enable_migration = enable.unwrap_or(true);
            let backoff_multiplier = 2.0;
            let max_backoff_delay_secs = 300;

            let policy = RecoveryPolicy {
                max_restart_attempts,
                restart_delay: Duration::from_secs(restart_delay_secs),
                enable_migration,
                backoff_multiplier,
                max_backoff_delay: Duration::from_secs(max_backoff_delay_secs),
            };

            match client
                .configure_vm_recovery_policy(name.clone(), policy)
                .await
            {
                Ok(response) => {
                    if response.success {
                        println!("Successfully configured recovery policy for VM '{}'", name);
                        println!("  Max Restart Attempts: {}", max_restart_attempts);
                        println!("  Restart Delay: {}s", restart_delay_secs);
                        println!("  Enable Migration: {}", enable_migration);
                        println!("  Backoff Multiplier: {}", backoff_multiplier);
                        println!("  Max Backoff Delay: {}s", max_backoff_delay_secs);
                    } else {
                        eprintln!("Failed to configure recovery policy: {}", response.message);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Error configuring recovery policy: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}

fn parse_health_check_type(
    check_type: &str,
    config: &str,
) -> Result<blixard_core::vm_health_types::HealthCheckType, String> {
    use blixard_core::vm_health_types::HealthCheckType;

    let config_value: serde_json::Value =
        serde_json::from_str(config).map_err(|e| format!("Invalid JSON config: {}", e))?;

    match check_type {
        "http" => {
            let url = config_value["url"]
                .as_str()
                .ok_or("Missing 'url' in config")?
                .to_string();
            let expected_status = config_value["expected_status"]
                .as_u64()
                .ok_or("Missing 'expected_status' in config")?
                as u16;
            let timeout_secs = config_value["timeout_secs"].as_u64().unwrap_or(10);

            let headers = if let Some(headers_val) = config_value["headers"].as_object() {
                Some(
                    headers_val
                        .iter()
                        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                        .collect(),
                )
            } else {
                None
            };

            Ok(HealthCheckType::Http {
                url,
                expected_status,
                timeout_secs,
                headers,
            })
        }
        "tcp" => {
            let address = config_value["address"]
                .as_str()
                .ok_or("Missing 'address' in config")?
                .to_string();
            let timeout_secs = config_value["timeout_secs"].as_u64().unwrap_or(5);

            Ok(HealthCheckType::Tcp {
                address,
                timeout_secs,
            })
        }
        "script" => {
            let command = config_value["command"]
                .as_str()
                .ok_or("Missing 'command' in config")?
                .to_string();
            let args = if let Some(args_val) = config_value["args"].as_array() {
                args_val
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            } else {
                vec![]
            };
            let expected_exit_code =
                config_value["expected_exit_code"].as_i64().unwrap_or(0) as i32;
            let timeout_secs = config_value["timeout_secs"].as_u64().unwrap_or(10);

            Ok(HealthCheckType::Script {
                command,
                args,
                expected_exit_code,
                timeout_secs,
            })
        }
        "console" => {
            let healthy_pattern = config_value["healthy_pattern"]
                .as_str()
                .ok_or("Missing 'healthy_pattern' in config")?
                .to_string();
            let unhealthy_pattern = config_value["unhealthy_pattern"]
                .as_str()
                .map(|s| s.to_string());
            let timeout_secs = config_value["timeout_secs"].as_u64().unwrap_or(5);

            Ok(HealthCheckType::Console {
                healthy_pattern,
                unhealthy_pattern,
                timeout_secs,
            })
        }
        "process" => {
            let process_name = config_value["process_name"]
                .as_str()
                .ok_or("Missing 'process_name' in config")?
                .to_string();
            let min_instances = config_value["min_instances"].as_u64().unwrap_or(1) as u32;

            Ok(HealthCheckType::Process {
                process_name,
                min_instances,
            })
        }
        "guest-agent" => {
            let timeout_secs = config_value["timeout_secs"].as_u64().unwrap_or(5);

            Ok(HealthCheckType::GuestAgent { timeout_secs })
        }
        _ => Err(format!("Unknown health check type: {}", check_type)),
    }
}

#[cfg(madsim)]
async fn handle_vm_health_command(
    _command: VmHealthCommands,
    _client: &mut crate::client::UnifiedClient,
) -> BlixardResult<()> {
    eprintln!("VM health commands are not available in simulation mode");
    std::process::exit(1);
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
    use crate::client::UnifiedClient;
    use blixard_core::iroh_types::{ClusterStatusRequest, JoinRequest, LeaveRequest};

    match command {
        ClusterCommands::Join { peer, local_addr } => {
            // Connect to the local node
            let mut client =
                UnifiedClient::new(&local_addr)
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("Failed to connect to local node: {}", e),
                    })?;

            // Parse peer address to get node ID (assuming format nodeID@address)
            let (node_id, bind_address) = if peer.contains('@') {
                let parts: Vec<&str> = peer.split('@').collect();
                if parts.len() != 2 {
                    eprintln!(
                        "Invalid peer format. Expected 'nodeID@address' but got '{}'",
                        peer
                    );
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
            let request = JoinRequest {
                node_id,
                bind_address,
                p2p_node_addr: None,
            };

            match client.join_cluster(request).await {
                Ok(resp) => {
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
        ClusterCommands::Leave {
            node_id,
            local_addr,
        } => {
            // Connect to the local node
            let mut client =
                UnifiedClient::new(&local_addr)
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("Failed to connect to local node: {}", e),
                    })?;

            // Send leave request with the provided node ID
            let request = LeaveRequest { node_id };

            match client.leave_cluster(request).await {
                Ok(resp) => {
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
            let mut client =
                UnifiedClient::new(&addr)
                    .await
                    .map_err(|e| BlixardError::Internal {
                        message: format!("Failed to connect to node: {}", e),
                    })?;

            // Get cluster status
            let request = ClusterStatusRequest {};

            match client.get_cluster_status(request).await {
                Ok(status) => {
                    println!("Cluster Status:");
                    println!("  Leader ID: {}", status.leader_id);
                    println!("  Term: {}", status.term);
                    println!("  Nodes in cluster:");
                    for node in &status.nodes {
                        println!(
                            "    - Node {}: {} ({})",
                            node.id,
                            node.address,
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
        ClusterCommands::Export {
            output,
            cluster_name,
            include_images,
            include_telemetry,
            no_compress,
            p2p_share,
            node_id,
            data_dir,
        } => {
            use blixard_core::cluster_state::{ClusterStateManager, ExportOptions};
            use blixard_core::iroh_transport_v2::IrohTransportV2;
            use blixard_core::storage::RedbRaftStorage;
            use blixard_core::raft_storage::Storage;
            use std::sync::Arc;

            // Initialize storage
            let db_path = std::path::Path::new(&data_dir).join("blixard.db");
            let database = Arc::new(redb::Database::create(&db_path)?);
            let storage: Arc<dyn Storage> =
                Arc::new(RedbRaftStorage { database });

            // Initialize P2P transport if needed
            let transport = if p2p_share {
                Some(Arc::new(
                    IrohTransportV2::new(node_id, std::path::Path::new(&data_dir)).await?,
                ))
            } else {
                None
            };

            let manager = ClusterStateManager::new(node_id, storage, transport.clone());

            let options = ExportOptions {
                include_vm_images: include_images,
                include_telemetry,
                compress: !no_compress,
                encryption_key: None,
            };

            if p2p_share {
                let ticket = manager.share_state_p2p(&cluster_name, &options).await?;
                println!("✅ Cluster state shared via P2P");
                println!("📤 Share this ticket with other nodes:");
                println!("{}", ticket);

                // Also save to file
                manager
                    .export_to_file(&cluster_name, &output, &options)
                    .await?;
                println!("💾 Also saved to file: {:?}", output);
            } else {
                manager
                    .export_to_file(&cluster_name, &output, &options)
                    .await?;
                println!("✅ Cluster state exported to: {:?}", output);
            }

            // Cleanup P2P transport
            if let Some(transport) = transport {
                match Arc::try_unwrap(transport) {
                    Ok(t) => {
                        t.shutdown().await?;
                    }
                    Err(_) => {
                        eprintln!("Warning: Failed to unwrap transport Arc - transport may still have references");
                    }
                }
            }
        }
        ClusterCommands::Import {
            input,
            merge,
            p2p,
            node_id,
            data_dir,
        } => {
            use blixard_core::cluster_state::ClusterStateManager;
            use blixard_core::iroh_transport_v2::IrohTransportV2;
            use blixard_core::storage::RedbRaftStorage;
            use blixard_core::raft_storage::Storage;
            use std::sync::Arc;

            // Initialize storage
            let db_path = std::path::Path::new(&data_dir).join("blixard.db");
            let database = Arc::new(redb::Database::create(&db_path)?);
            let storage: Arc<dyn Storage> =
                Arc::new(RedbRaftStorage { database });

            // Initialize P2P transport if needed
            let transport = if p2p {
                Some(Arc::new(
                    IrohTransportV2::new(node_id, std::path::Path::new(&data_dir)).await?,
                ))
            } else {
                None
            };

            let manager = ClusterStateManager::new(node_id, storage, transport.clone());

            if p2p {
                println!("📥 Importing cluster state from P2P ticket...");
                manager.import_state_p2p(&input, merge).await?;
                println!("✅ Cluster state imported successfully from P2P");
            } else {
                let input_path = std::path::PathBuf::from(input);
                println!("📥 Importing cluster state from file: {:?}", input_path);
                manager.import_from_file(&input_path, merge).await?;
                println!("✅ Cluster state imported successfully");
            }

            // Cleanup P2P transport
            if let Some(transport) = transport {
                match Arc::try_unwrap(transport) {
                    Ok(t) => {
                        t.shutdown().await?;
                    }
                    Err(_) => {
                        eprintln!("Warning: Failed to unwrap transport Arc - transport may still have references");
                    }
                }
            }
        }
        ClusterCommands::IpPool { command } => {
            handle_ip_pool_command(command).await?;
        }
    }

    Ok(())
}

#[cfg(madsim)]
async fn handle_cluster_command(_command: ClusterCommands) -> BlixardResult<()> {
    eprintln!("Cluster commands are not available in simulation mode");
    std::process::exit(1);
}

#[cfg(not(madsim))]
async fn handle_ip_pool_command(command: IpPoolCommands) -> BlixardResult<()> {
    use crate::client::UnifiedClient;
    use blixard_core::ip_pool::{IpPoolCommand, IpPoolConfig, IpPoolId};
    use blixard_core::raft_manager::ProposalData;
    use std::collections::{BTreeSet, HashMap};
    use std::net::IpAddr;
    use std::str::FromStr;
    
    // Check for BLIXARD_NODE_ADDR environment variable first
    let node_addr =
        std::env::var("BLIXARD_NODE_ADDR").unwrap_or_else(|_| command_addr(&command));
    
    // Connect to the node
    let _client = UnifiedClient::new(&node_addr)
        .await
        .map_err(|e| BlixardError::Internal {
            message: format!("Failed to connect to node at {}: {}", node_addr, e),
        })?;
    
    match command {
        IpPoolCommands::Create {
            name,
            subnet,
            gateway,
            dns,
            start_ip,
            end_ip,
            vlan,
            topology,
            addr: _,
        } => {
            // Parse subnet
            let subnet_net = ipnet::IpNet::from_str(&subnet)
                .map_err(|e| BlixardError::ConfigError(format!("Invalid subnet: {}", e)))?;
            
            // Parse IPs
            let gateway_ip = IpAddr::from_str(&gateway)
                .map_err(|e| BlixardError::ConfigError(format!("Invalid gateway IP: {}", e)))?;
            let start_addr = IpAddr::from_str(&start_ip)
                .map_err(|e| BlixardError::ConfigError(format!("Invalid start IP: {}", e)))?;
            let end_addr = IpAddr::from_str(&end_ip)
                .map_err(|e| BlixardError::ConfigError(format!("Invalid end IP: {}", e)))?;
            
            // Parse DNS servers
            let dns_servers: Vec<IpAddr> = dns
                .split(',')
                .map(|s| IpAddr::from_str(s.trim()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| BlixardError::ConfigError(format!("Invalid DNS server: {}", e)))?;
            
            // Create pool config
            let pool_id = IpPoolId(rand::random());
            let config = IpPoolConfig {
                id: pool_id,
                name,
                subnet: subnet_net,
                vlan_id: vlan,
                gateway: gateway_ip,
                dns_servers,
                allocation_start: start_addr,
                allocation_end: end_addr,
                topology_hint: topology,
                reserved_ips: BTreeSet::new(),
                enabled: true,
                tags: HashMap::new(),
            };
            
            // Validate the config
            config.validate()?;
            
            // Submit through Raft
            println!("Creating IP pool '{}'...", config.name);
            
            // Create the IP pool command
            let command = IpPoolCommand::CreatePool(config.clone());
            let _proposal = ProposalData::IpPoolCommand(command);
            
            // Submit the proposal through cluster operations
            // Note: This requires a proper cluster operation method to be added
            eprintln!("Note: IP pool creation through CLI requires cluster operation support");
            eprintln!("Pool configuration validated successfully:");
            eprintln!("  ID: {}", pool_id);
            eprintln!("  Subnet: {}", subnet_net);
            eprintln!("  Gateway: {}", gateway_ip);
            eprintln!("  Range: {} - {}", start_addr, end_addr);
            eprintln!("  Total IPs: {}", config.total_capacity());
            
            // TODO: Add cluster operation to submit proposal
            // Example: client.submit_proposal(proposal).await?;
        }
        IpPoolCommands::List { addr: _ } => {
            println!("Listing IP pools...");
            // TODO: Implement a cluster operation to query IP pools from state
            // This would query the IP pool manager state through Raft
            eprintln!("Note: IP pool listing requires cluster state query support");
        }
        IpPoolCommands::Show { id, addr: _ } => {
            println!("Showing IP pool {}...", id);
            eprintln!("Note: IP pool details through CLI is not yet fully implemented");
        }
        IpPoolCommands::Delete { id, addr: _ } => {
            println!("Deleting IP pool {}...", id);
            
            let command = IpPoolCommand::DeletePool(IpPoolId(id));
            let _proposal = ProposalData::IpPoolCommand(command);
            
            // TODO: Submit proposal through cluster operation
            eprintln!("Note: IP pool deletion requires cluster operation support");
        }
        IpPoolCommands::Enable { id, enable, addr: _ } => {
            let action = if enable { "Enabling" } else { "Disabling" };
            println!("{} IP pool {}...", action, id);
            
            let command = IpPoolCommand::SetPoolEnabled { 
                pool_id: IpPoolId(id), 
                enabled: enable 
            };
            let _proposal = ProposalData::IpPoolCommand(command);
            
            // TODO: Submit proposal through cluster operation
            eprintln!("Note: IP pool enable/disable requires cluster operation support");
        }
        IpPoolCommands::Stats { id, addr: _ } => {
            if let Some(pool_id) = id {
                println!("Getting statistics for IP pool {}...", pool_id);
            } else {
                println!("Getting statistics for all IP pools...");
            }
            eprintln!("Note: IP pool statistics through CLI is not yet fully implemented");
        }
    }
    
    Ok(())
}

fn command_addr(command: &IpPoolCommands) -> String {
    match command {
        IpPoolCommands::Create { addr, .. } |
        IpPoolCommands::List { addr } |
        IpPoolCommands::Show { addr, .. } |
        IpPoolCommands::Delete { addr, .. } |
        IpPoolCommands::Enable { addr, .. } |
        IpPoolCommands::Stats { addr, .. } => addr.clone(),
    }
}

#[cfg(madsim)]
async fn handle_ip_pool_command(_command: IpPoolCommands) -> BlixardResult<()> {
    eprintln!("IP pool commands are not available in simulation mode");
    std::process::exit(1);
}

async fn handle_reset_command(
    data_dir: &str,
    vm_config_dir: &str,
    vm_data_dir: &str,
    force: bool,
) -> BlixardResult<()> {
    use std::io::{self, Write};
    use std::path::Path;

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
        if let Err(e) = io::stdout().flush() {
            eprintln!("Error flushing stdout: {}", e);
            return Err(BlixardError::IoError(Box::new(e)));
        }

        let mut input = String::new();
        if let Err(e) = io::stdin().read_line(&mut input) {
            eprintln!("Error reading input: {}", e);
            return Err(BlixardError::IoError(Box::new(e)));
        }
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
                        Err(e) => {
                            println!("⚠️  Warning: Failed to process pattern {}: {}", pattern, e)
                        }
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
                        Err(_) => {} // Ignore glob errors for optional cleanup
                    }
                }
            }
            Err(_) => {} // Ignore glob errors for optional cleanup
        }
    }

    println!();
    println!("✨ Reset complete! Blixard is now in a clean state.");
    println!("💡 You can now start fresh with: cargo run -- node --id 1 --bind 127.0.0.1:7001");

    Ok(())
}

#[cfg(not(madsim))]
async fn handle_tui_command() -> BlixardResult<()> {
    use crossterm::{
        event::{DisableMouseCapture, EnableMouseCapture},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::{backend::CrosstermBackend, Terminal};
    use std::io;

    // Setup terminal
    enable_raw_mode().map_err(|e| BlixardError::Internal {
        message: format!("Failed to enable raw mode: {}", e),
    })?;

    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).map_err(|e| {
        BlixardError::Internal {
            message: format!("Failed to setup terminal: {}", e),
        }
    })?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| BlixardError::Internal {
        message: format!("Failed to create terminal: {}", e),
    })?;

    // Create modern app and event handler
    let mut app = tui::app::App::new();
    let mut event_handler = tui::events::EventHandler::new(250); // 250ms tick rate

    // Setup event sender and initial data refresh
    app.set_event_sender(event_handler.sender());
    app.refresh_all_data().await?;

    // Main loop
    let result = loop {
        // Draw modern UI
        terminal
            .draw(|f| tui::ui::render(f, &app))
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to draw terminal: {}", e),
            })?;

        // Handle events
        match event_handler.next().await {
            Ok(event) => {
                // Handle quit event for immediate exit
                use tui::events::Event;
                if let Event::Key(key_event) = &event {
                    if key_event.code == crossterm::event::KeyCode::Char('q') {
                        app.ui_state.should_quit = true;
                    }
                }

                // Let the app handle the event
                app.handle_event(event).await?;

                if app.ui_state.should_quit {
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
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .map_err(|e| BlixardError::Internal {
        message: format!("Failed to restore terminal: {}", e),
    })?;

    terminal.show_cursor().map_err(|e| BlixardError::Internal {
        message: format!("Failed to show cursor: {}", e),
    })?;

    result
}

#[cfg(not(madsim))]
async fn handle_security_command(command: SecurityCommands) -> BlixardResult<()> {
    use blixard_core::cert_generator::{
        generate_cluster_pki, KeyAlgorithm,
    };
    use std::path::PathBuf;

    match command {
        SecurityCommands::GenCerts {
            cluster_name,
            nodes,
            clients,
            output_dir,
            validity_days: _,
            key_algorithm,
        } => {
            println!("🔐 Generating certificates for cluster: {}", cluster_name);

            // Parse node names
            let node_names: Vec<String> = nodes
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            if node_names.is_empty() {
                eprintln!("Error: No node names provided");
                std::process::exit(1);
            }

            // Parse client names
            let client_names: Vec<String> = clients
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            // Parse key algorithm
            let _key_algo = match key_algorithm.as_str() {
                "rsa2048" => KeyAlgorithm::Rsa2048,
                "rsa4096" => KeyAlgorithm::Rsa4096,
                "ecdsa-p256" => KeyAlgorithm::EcdsaP256,
                "ecdsa-p384" => KeyAlgorithm::EcdsaP384,
                _ => {
                    eprintln!("Error: Invalid key algorithm '{}'. Valid options: rsa2048, rsa4096, ecdsa-p256, ecdsa-p384", key_algorithm);
                    std::process::exit(1);
                }
            };

            // Create output directory
            let output_path = PathBuf::from(&output_dir);

            // Generate certificates
            match generate_cluster_pki(
                &cluster_name,
                node_names.clone(),
                client_names.clone(),
                output_path.clone(),
            )
            .await
            {
                Ok(()) => {
                    println!("✅ Successfully generated certificates!");
                    println!();
                    println!("📁 Certificate files created in: {}", output_dir);
                    println!();
                    println!("🔑 CA Certificate:");
                    println!("   - {}/ca-{}-ca.crt", output_dir, cluster_name);
                    println!("   - {}/ca-{}-ca.key", output_dir, cluster_name);
                    println!();
                    println!("🖥️  Server Certificates:");
                    for node in &node_names {
                        println!("   - {}/server-{}.crt", output_dir, node);
                        println!("   - {}/server-{}.key", output_dir, node);
                    }
                    println!();
                    println!("👤 Client Certificates:");
                    for client in &client_names {
                        println!("   - {}/client-{}.crt", output_dir, client);
                        println!("   - {}/client-{}.key", output_dir, client);
                    }
                    println!();
                    println!("💡 To use these certificates, configure your nodes with:");
                    println!("   --config <config.toml>");
                    println!();
                    println!("Example config.toml:");
                    println!("[security.tls]");
                    println!("enabled = true");
                    println!("cert_file = \"{}/server-<node-name>.crt\"", output_dir);
                    println!("key_file = \"{}/server-<node-name>.key\"", output_dir);
                    println!("ca_file = \"{}/ca-{}-ca.crt\"", output_dir, cluster_name);
                    println!("require_client_cert = true  # For mTLS");
                }
                Err(e) => {
                    eprintln!("❌ Failed to generate certificates: {}", e);
                    std::process::exit(1);
                }
            }
        }
        SecurityCommands::GenToken {
            user,
            permissions,
            validity_days,
            addr: _,
        } => {
            // Parse permissions
            let perm_strings: Vec<&str> = permissions
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            let mut parsed_permissions = Vec::with_capacity(perm_strings.len());
            for perm in perm_strings {
                match perm {
                    "cluster-read" => parsed_permissions.push("ClusterRead"),
                    "cluster-write" => parsed_permissions.push("ClusterWrite"),
                    "vm-read" => parsed_permissions.push("VmRead"),
                    "vm-write" => parsed_permissions.push("VmWrite"),
                    "task-read" => parsed_permissions.push("TaskRead"),
                    "task-write" => parsed_permissions.push("TaskWrite"),
                    "metrics-read" => parsed_permissions.push("MetricsRead"),
                    "admin" => parsed_permissions.push("Admin"),
                    _ => {
                        eprintln!("Error: Invalid permission '{}'. Valid options: cluster-read, cluster-write, vm-read, vm-write, task-read, task-write, metrics-read, admin", perm);
                        std::process::exit(1);
                    }
                }
            }

            println!("🔑 Generating API token for user: {}", user);
            println!("📋 Permissions: {:?}", parsed_permissions);
            println!("⏱️  Validity: {} days", validity_days);

            // TODO: Connect to node and generate token via gRPC
            eprintln!("Note: Token generation via gRPC is not yet implemented");
            eprintln!("For now, tokens must be generated on the node directly");
        }
    }

    Ok(())
}

#[cfg(madsim)]
async fn handle_security_command(_command: SecurityCommands) -> BlixardResult<()> {
    eprintln!("Security commands are not available in simulation mode");
    std::process::exit(1);
}

#[cfg(madsim)]
async fn handle_modern_tui_command() -> BlixardResult<()> {
    eprintln!("Modern TUI is not available in simulation mode");
    std::process::exit(1);
}

#[cfg(madsim)]
async fn handle_tui_command() -> BlixardResult<()> {
    eprintln!("TUI is not available in simulation mode");
    std::process::exit(1);
}
