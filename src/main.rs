use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

mod cluster;
mod config;
mod error;
mod node;
mod service;
mod storage;

use crate::config::Config;
use crate::service::ServiceManager;

#[derive(Parser, Debug)]
#[command(name = "blixard")]
#[command(author, version, about = "Distributed service management system", long_about = None)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Config file location
    #[arg(short, long, value_name = "FILE", global = true)]
    config: Option<PathBuf>,

    /// Manage user-level services instead of system services
    #[arg(long, global = true)]
    user: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start a service
    Start {
        /// Name of the service to start
        service: String,
    },
    /// Stop a service
    Stop {
        /// Name of the service to stop
        service: String,
    },
    /// List all managed services
    List,
    /// Show status of a service
    Status {
        /// Name of the service
        service: String,
    },
    /// Remove a service from management
    Remove {
        /// Name of the service to remove
        service: String,
    },
    /// Initialize as primary cluster node
    #[command(name = "init-primary")]
    InitPrimary {
        /// Node ID for the primary node
        #[arg(long)]
        node_id: Option<String>,
    },
    /// Initialize as secondary cluster node
    #[command(name = "init-secondary")]
    InitSecondary {
        /// Address of primary node to join
        #[arg(long)]
        primary_addr: String,
    },
    /// Join an existing cluster
    #[command(name = "join-cluster")]
    JoinCluster {
        /// Address of a cluster node
        #[arg(long)]
        cluster_addr: String,
    },
    /// Show cluster status
    #[command(name = "cluster-status")]
    ClusterStatus,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let filter = if cli.verbose {
        EnvFilter::from_default_env()
            .add_directive("blixard=debug".parse()?)
            .add_directive("tikv_raft=info".parse()?)
    } else {
        EnvFilter::from_default_env()
            .add_directive("blixard=info".parse()?)
            .add_directive("tikv_raft=warn".parse()?)
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .init();

    info!("Starting Blixard service manager");

    // Load configuration
    let config = Config::load(cli.config)?;

    // Execute command
    match cli.command {
        Commands::Start { service } => {
            info!("Starting service: {}", service);
            let manager = ServiceManager::new(config, cli.user).await?;
            manager.start_service(&service).await?;
            println!("Service '{}' started successfully", service);
        }
        Commands::Stop { service } => {
            info!("Stopping service: {}", service);
            let manager = ServiceManager::new(config, cli.user).await?;
            manager.stop_service(&service).await?;
            println!("Service '{}' stopped successfully", service);
        }
        Commands::List => {
            let manager = ServiceManager::new(config, cli.user).await?;
            let services = manager.list_services().await?;
            
            if services.is_empty() {
                println!("No services managed by Blixard");
            } else {
                println!("Services managed by Blixard:");
                for (name, info) in services {
                    println!("  {} - {}", name, info.state);
                }
            }
        }
        Commands::Status { service } => {
            let manager = ServiceManager::new(config, cli.user).await?;
            let info = manager.get_service_status(&service).await?;
            println!("Service: {}", service);
            println!("State: {}", info.state);
            println!("Node: {}", info.node);
            println!("Last updated: {}", info.timestamp);
        }
        Commands::Remove { service } => {
            info!("Removing service: {}", service);
            let manager = ServiceManager::new(config, cli.user).await?;
            manager.remove_service(&service).await?;
            println!("Service '{}' removed from management", service);
        }
        Commands::InitPrimary { node_id } => {
            info!("Initializing as primary cluster node");
            let node_id = node_id.unwrap_or_else(|| {
                let id = uuid::Uuid::new_v4().to_string();
                info!("Generated node ID: {}", id);
                id
            });
            
            let node = crate::node::Node::init_primary(config, node_id).await?;
            println!("Primary node initialized successfully");
            
            // Keep the node running
            tokio::signal::ctrl_c().await?;
            info!("Shutting down primary node");
        }
        Commands::InitSecondary { primary_addr } => {
            info!("Initializing as secondary cluster node");
            let node_id = uuid::Uuid::new_v4().to_string();
            info!("Generated node ID: {}", node_id);
            
            let node = crate::node::Node::init_secondary(config, node_id, primary_addr).await?;
            println!("Secondary node initialized successfully");
            
            // Keep the node running
            tokio::signal::ctrl_c().await?;
            info!("Shutting down secondary node");
        }
        Commands::JoinCluster { cluster_addr } => {
            info!("Joining cluster at: {}", cluster_addr);
            let node_id = uuid::Uuid::new_v4().to_string();
            
            let node = crate::node::Node::join_cluster(config, node_id, cluster_addr).await?;
            println!("Joined cluster successfully");
            
            // Keep the node running
            tokio::signal::ctrl_c().await?;
            info!("Shutting down node");
        }
        Commands::ClusterStatus => {
            let manager = ServiceManager::new(config, cli.user).await?;
            let status = manager.get_cluster_status().await?;
            
            println!("Cluster Status:");
            println!("Leader: {}", status.leader.unwrap_or_else(|| "None".to_string()));
            println!("Nodes:");
            for node in status.nodes {
                println!("  {} - {}", node.id, node.state);
            }
        }
    }

    Ok(())
}