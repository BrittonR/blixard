use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use std::sync::Arc;
use std::time::Duration;
use std::net::SocketAddr;

use crate::error::{BlixardError, BlixardResult};
use crate::types::{NodeConfig, VmCommand};
use crate::vm_backend::{VmManager, VmBackendRegistry};
use crate::raft_manager::{RaftManager, RaftProposal, ProposalData};
use crate::node_shared::SharedNodeState;
use crate::transport::iroh_peer_connector::IrohPeerConnector;
use crate::vm_health_monitor::VmHealthMonitor;

use redb::Database;

/// A Blixard cluster node
pub struct Node {
    /// Shared state that is Send + Sync
    shared: Arc<SharedNodeState>,
    /// Non-sync runtime handles
    handle: Option<JoinHandle<BlixardResult<()>>>,
    raft_handle: Option<JoinHandle<BlixardResult<()>>>,
    /// VM health monitor
    health_monitor: Option<VmHealthMonitor>,
    /// Raft transport adapter
    raft_transport: Option<Arc<crate::transport::raft_transport_adapter::RaftTransport>>,
}

impl Node {
    /// Create a new node
    pub fn new(config: NodeConfig) -> Self {
        Self {
            shared: Arc::new(SharedNodeState::new(config)),
            handle: None,
            raft_handle: None,
            health_monitor: None,
            raft_transport: None,
        }
    }
    
    /// Get a shared reference to the node state
    /// This is what should be passed to the gRPC server
    pub fn shared(&self) -> Arc<SharedNodeState> {
        Arc::clone(&self.shared)
    }

    /// Initialize the node with database and channels
    pub async fn initialize(&mut self) -> BlixardResult<()> {
        // Call internal initialization with default registry
        self.initialize_internal(None).await
    }
    
    /// Internal initialization that accepts an optional VM backend registry
    async fn initialize_internal(&mut self, registry_opt: Option<VmBackendRegistry>) -> BlixardResult<()> {
        // Initialize database
        let data_dir = self.shared.config.data_dir.clone();
        
        // Ensure the data directory exists
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| BlixardError::Storage {
                operation: "create data directory".to_string(),
                source: Box::new(e),
            })?;
        
        let db_path = format!("{}/blixard.db", data_dir);
        
        // Try to open existing database first, create if it doesn't exist
        let database = match std::fs::metadata(&db_path) {
            Ok(_) => {
                // Database exists, open it
                Database::open(&db_path)
                    .map_err(|e| BlixardError::Storage {
                        operation: "open database".to_string(),
                        source: Box::new(e),
                    })?
            }
            Err(_) => {
                // Database doesn't exist, create it
                Database::create(&db_path)
                    .map_err(|e| BlixardError::Storage {
                        operation: "create database".to_string(),
                        source: Box::new(e),
                    })?
            }
        };
        let db_arc = Arc::new(database);
        
        // Initialize all database tables
        crate::storage::init_database_tables(&db_arc)?;
        
        self.shared.set_database(db_arc.clone()).await;

        // Initialize VM manager with factory pattern
        // Use provided registry or default to mock backend
        let registry = registry_opt.unwrap_or_else(VmBackendRegistry::default);
        let vm_backend = self.create_vm_backend(&registry, db_arc.clone())?;
        let vm_manager = Arc::new(VmManager::new(db_arc.clone(), vm_backend, self.shared.clone()));
        
        self.shared.set_vm_manager(vm_manager.clone()).await;
        
        // Initialize VM health monitor
        let health_check_interval = std::time::Duration::from_secs(30); // Check every 30 seconds
        self.health_monitor = Some(VmHealthMonitor::new(
            self.shared.clone(),
            vm_manager,
            health_check_interval,
        ));

        // Initialize quota manager
        let storage = Arc::new(crate::storage::RedbRaftStorage { database: db_arc.clone() });
        let quota_manager = Arc::new(crate::quota_manager::QuotaManager::new(storage).await?);
        self.shared.set_quota_manager(quota_manager).await;

        // Initialize security manager
        let config = crate::config_global::get();
        let security_manager = Arc::new(crate::security::SecurityManager::new(config.security.clone()).await?);
        self.shared.set_security_manager(security_manager).await;
        
        // Initialize observability (metrics and tracing)
        if config.observability.metrics.enabled || config.observability.tracing.enabled {
            tracing::info!("Initializing observability (metrics: {}, tracing: {})",
                         config.observability.metrics.enabled,
                         config.observability.tracing.enabled);
            let observability_manager = Arc::new(crate::observability::ObservabilityManager::new(config.observability.clone()).await?);
            self.shared.set_observability_manager(observability_manager).await;
        }

        // Initialize P2P manager if enabled OR if using Iroh transport
        // With the new transport config, we consider Iroh transport enabled if transport_config is present
        let using_iroh_transport = self.shared.config.transport_config.is_some();
        
        if config.p2p.enabled || using_iroh_transport {
            tracing::info!("Initializing P2P manager (P2P enabled: {}, Iroh transport: {})", 
                         config.p2p.enabled, using_iroh_transport);
            let p2p_config = crate::p2p_manager::P2pConfig::default();
            let p2p_manager = Arc::new(
                crate::p2p_manager::P2pManager::new(
                    self.shared.get_id(),
                    std::path::Path::new(&self.shared.config.data_dir),
                    p2p_config,
                ).await?
            );
            self.shared.set_p2p_manager(p2p_manager.clone()).await;
            
            // Get and store our P2P node address
            if let Ok(node_addr) = p2p_manager.get_node_addr().await {
                self.shared.set_p2p_node_addr(node_addr).await;
                tracing::info!("P2P node initialized with address");
            }
            
            // Start P2P services in background
            let p2p_handle = p2p_manager.clone();
            tokio::spawn(async move {
                if let Err(e) = p2p_handle.start().await {
                    tracing::error!("P2P manager failed to start: {}", e);
                }
            });
        }

        // Initialize Raft manager
        // If join_addr is provided, don't bootstrap as single node
        let peers = if let Some(_join_addr) = &self.shared.config.join_addr {
            // We're joining an existing cluster, add a dummy peer to prevent bootstrap
            // The actual peer will be discovered during join
            vec![] // Empty peers - will be populated during join
        } else {
            vec![] // Start with no peers, bootstrap as single node
        };
        let node_id = self.shared.get_id();
        
        // Initialize storage based on node type
        let storage = crate::storage::RedbRaftStorage { database: db_arc.clone() };
        if self.shared.config.join_addr.is_none() {
            // Bootstrap as single node
            storage.initialize_single_node(node_id)?;
            
            // Register this node as a worker in bootstrap mode
            let address = self.shared.get_bind_addr().to_string();
            // Get system resources - use reasonable defaults
            // In production, these could be configured or detected more accurately
            let total_memory_mb = Self::estimate_available_memory();
            let disk_gb = Self::estimate_available_disk(&self.shared.config.data_dir);
            
            let capabilities = crate::raft_manager::WorkerCapabilities {
                cpu_cores: num_cpus::get() as u32,
                memory_mb: total_memory_mb,
                disk_gb,
                features: vec!["microvm".to_string()],
            };
            
            // Bootstrap Exception: Direct database writes are allowed during initial bootstrap
            // of a single-node cluster. After bootstrap completes, all state changes must
            // go through Raft consensus to ensure consistency across the cluster.
            let write_txn = db_arc.begin_write()?;
            {
                let mut worker_table = write_txn.open_table(crate::storage::WORKER_TABLE)?;
                let mut status_table = write_txn.open_table(crate::storage::WORKER_STATUS_TABLE)?;
                
                let worker_data = bincode::serialize(&(address.clone(), capabilities))?;
                worker_table.insert(node_id.to_le_bytes().as_slice(), worker_data.as_slice())?;
                status_table.insert(node_id.to_le_bytes().as_slice(), [crate::raft_manager::WorkerStatus::Online as u8].as_slice())?;
            }
            write_txn.commit()?;
            
            tracing::info!("Node {} registered as worker in bootstrap mode", node_id);
        } else {
            // Initialize as joining node
            storage.initialize_joining_node()?;
        }
        
        let (raft_manager, proposal_tx, message_tx, conf_change_tx, outgoing_rx) = RaftManager::new(
            node_id,
            db_arc.clone(),
            peers,
            Arc::downgrade(&self.shared),
        )?;
        
        self.shared.set_raft_proposal_tx(proposal_tx).await;
        self.shared.set_raft_message_tx(message_tx).await;
        self.shared.set_raft_conf_change_tx(conf_change_tx).await;
        
        // Get transport config
        let default_config = crate::transport::config::TransportConfig::default();
        let transport_config = self.shared.config.transport_config
            .as_ref()
            .unwrap_or(&default_config);
        
        // Create Raft transport adapter (handles both gRPC and Iroh)
        let (raft_msg_tx, raft_msg_rx) = tokio::sync::mpsc::unbounded_channel();
        let raft_transport = Arc::new(crate::transport::raft_transport_adapter::RaftTransport::new(
            self.shared.clone(),
            raft_msg_tx,
            transport_config
        ).await?);
        
        // Store transport in node
        self.raft_transport = Some(raft_transport.clone());
        
        // Create Iroh peer connector with NoOp monitor for now
        let p2p_monitor: Arc<dyn crate::p2p_monitor::P2pMonitor> = Arc::new(crate::p2p_monitor::NoOpMonitor);
        let peer_connector = Arc::new(IrohPeerConnector::new(
            raft_transport.endpoint().as_ref().clone(),
            self.shared.clone(),
            p2p_monitor,
        ));
        self.shared.set_peer_connector(peer_connector.clone()).await;
        
        // Start the peer connector background tasks
        peer_connector.start().await?;
        tracing::info!("Started Iroh peer connector");
        
        // Initialize discovery manager if we have discovery configured
        // This is done after peer connector is set so auto-connect can work
        if let Ok(discovery_config) = Self::create_discovery_config(&self.shared.config) {
            // Check if we have any discovery methods enabled or static nodes configured
            if discovery_config.enable_static || discovery_config.enable_dns || discovery_config.enable_mdns || !discovery_config.static_nodes.is_empty() {
                tracing::info!("Initializing discovery manager (static={}, dns={}, mdns={}, static_nodes={})", 
                    discovery_config.enable_static, discovery_config.enable_dns, discovery_config.enable_mdns, discovery_config.static_nodes.len());
                let mut discovery_manager = crate::discovery::DiscoveryManager::new(discovery_config);
                
                // Start discovery process
                if let Err(e) = discovery_manager.start().await {
                    tracing::warn!("Failed to start discovery manager: {}", e);
                }
                
                let discovery_arc = Arc::new(discovery_manager);
                self.shared.set_discovery_manager(discovery_arc).await;
            }
        }
        
        // Start transport maintenance
        raft_transport.start_maintenance().await;
        
        // Start task to handle outgoing Raft messages with recovery
        let transport_clone = raft_transport.clone();
        let shared_weak = Arc::downgrade(&self.shared);
        let _outgoing_handle = tokio::spawn(async move {
            let mut outgoing_rx = outgoing_rx;
            let mut consecutive_errors = 0;
            const MAX_CONSECUTIVE_ERRORS: u32 = 100;
            
            loop {
                match outgoing_rx.recv().await {
                    Some((to, msg)) => {
                        if let Err(e) = transport_clone.send_message(to, msg).await {
                            tracing::warn!("Failed to send Raft message to {}: {}", to, e);
                            consecutive_errors += 1;
                            
                            // If we have too many consecutive errors, something is wrong
                            if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                                tracing::error!("Too many consecutive errors ({}) sending Raft messages, checking if node is still alive", consecutive_errors);
                                
                                // Check if the node is still alive
                                if shared_weak.upgrade().is_none() {
                                    tracing::error!("Node has been dropped, exiting outgoing message handler");
                                    break;
                                }
                                
                                // Reset counter and continue
                                consecutive_errors = 0;
                            }
                        } else {
                            // Reset error counter on successful send
                            consecutive_errors = 0;
                        }
                    }
                    None => {
                        // Channel closed - this should only happen during shutdown
                        tracing::info!("Outgoing Raft message channel closed, exiting handler");
                        break;
                    }
                }
            }
            Ok::<(), BlixardError>(())
        });
        
        // Start Raft manager with automatic recovery
        let shared_weak = Arc::downgrade(&self.shared);
        let raft_handle = tokio::spawn(async move {
            let mut restart_count = 0;
            let config = crate::config_global::get();
            let max_restarts = config.cluster.raft.max_restarts;
            let restart_delay_ms = config.cluster.raft.restart_delay.as_millis() as u64;
            
            // Run the initial Raft manager
            match raft_manager.run().await {
                Ok(_) => {
                    tracing::info!("Raft manager exited normally");
                    return Ok(());
                }
                Err(e) => {
                    tracing::error!("Raft manager crashed: {}", e);
                    restart_count = 1;
                }
            }
            
            // Recovery loop
            while restart_count < max_restarts {
                tracing::info!("Restarting Raft manager (attempt #{})", restart_count + 1);
                
                // Get strong reference to shared state
                let shared = match shared_weak.upgrade() {
                    Some(s) => s,
                    None => {
                        tracing::error!("Shared state dropped, cannot restart Raft manager");
                        break;
                    }
                };
                
                // Wait before restarting with exponential backoff
                tokio::time::sleep(tokio::time::Duration::from_millis(restart_delay_ms * restart_count as u64)).await;
                
                // Get database and peers
                let db = match shared.get_database().await {
                    Some(db) => db,
                    None => {
                        tracing::error!("Database not available, cannot restart Raft manager");
                        break;
                    }
                };
                
                let peer_infos = shared.get_peers().await;
                let peers: Vec<(u64, String)> = peer_infos.into_iter()
                    .map(|p| (p.id, p.address))
                    .collect();
                let node_id = shared.get_id();
                
                // Create new Raft manager
                match RaftManager::new(
                    node_id,
                    db.clone(),
                    peers,
                    Arc::downgrade(&shared),
                ) {
                    Ok((new_raft_manager, proposal_tx, message_tx, conf_change_tx, outgoing_rx)) => {
                        // Update shared state with new channels
                        shared.set_raft_proposal_tx(proposal_tx).await;
                        shared.set_raft_message_tx(message_tx).await;
                        shared.set_raft_conf_change_tx(conf_change_tx).await;
                        
                        // Get transport config
                        let default_config = crate::transport::config::TransportConfig::default();
                        let transport_config = shared.config.transport_config
                            .as_ref()
                            .unwrap_or(&default_config);
                        
                        // Create new Raft transport adapter
                        let (raft_msg_tx, _raft_msg_rx) = tokio::sync::mpsc::unbounded_channel();
                        match crate::transport::raft_transport_adapter::RaftTransport::new(
                            shared.clone(),
                            raft_msg_tx,
                            transport_config
                        ).await {
                            Ok(new_transport) => {
                                // Start handling outgoing messages with new transport (with recovery)
                                let transport_clone = new_transport.clone();
                                let shared_weak_inner = Arc::downgrade(&shared);
                                tokio::spawn(async move {
                                    let mut outgoing_rx = outgoing_rx;
                                    let mut consecutive_errors = 0;
                                    const MAX_CONSECUTIVE_ERRORS: u32 = 100;
                                    
                                    loop {
                                        match outgoing_rx.recv().await {
                                            Some((to, msg)) => {
                                                if let Err(e) = transport_clone.send_message(to, msg).await {
                                                    tracing::warn!("Failed to send Raft message to {}: {}", to, e);
                                                    consecutive_errors += 1;
                                                    
                                                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                                                        tracing::error!("Too many consecutive errors in restarted handler");
                                                        if shared_weak_inner.upgrade().is_none() {
                                                            break;
                                                        }
                                                        consecutive_errors = 0;
                                                    }
                                                } else {
                                                    consecutive_errors = 0;
                                                }
                                            }
                                            None => {
                                                tracing::info!("Outgoing channel closed in restarted handler");
                                                break;
                                            }
                                        }
                                    }
                                });
                                
                                // Start transport maintenance
                                new_transport.start_maintenance().await;
                            }
                            Err(e) => {
                                tracing::error!("Failed to recreate Raft transport: {}", e);
                                // Without RaftTransport, we can't send Raft messages
                                tracing::error!("No transport available for Raft messages - node cannot participate in consensus");
                            }
                        }
                        
                        // Run the new Raft manager
                        match new_raft_manager.run().await {
                            Ok(_) => {
                                tracing::info!("Restarted Raft manager exited normally");
                                return Ok(());
                            }
                            Err(e) => {
                                tracing::error!("Restarted Raft manager crashed: {}", e);
                                restart_count += 1;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to recreate Raft manager: {}", e);
                        return Err(e);
                    }
                }
            }
            
            Err(BlixardError::Internal {
                message: format!("Raft manager failed after {} restart attempts", max_restarts),
            })
        });
        self.raft_handle = Some(raft_handle);
        
        // If we have a join address, send join request after initialization
        if let Some(join_addr) = self.shared.config.join_addr.clone() {
            // Give the Raft manager a moment to start
            let join_delay = 100u64; // Fixed 100ms delay for join requests
            tokio::time::sleep(tokio::time::Duration::from_millis(join_delay)).await;
            
            // Pre-connect to the join address to ensure bidirectional connectivity
            if let Some(peer) = self.shared.get_peer(1).await {
                // For gRPC mode, use PeerConnector if available
                if let Some(peer_connector) = self.shared.get_peer_connector().await {
                    let _ = peer_connector.connect_to_peer_by_id(peer.id).await;
                    tracing::info!("Pre-connected to leader at {} before sending join request", join_addr);
                }
                // For Iroh mode, connection will be established when sending messages
            }
            
            if let Err(e) = self.send_join_request().await {
                tracing::error!("Failed to join cluster: {}", e);
                return Err(e);
            }
            
            // After successfully joining the cluster, wait a bit for leader election
            // and configuration to stabilize, then register as a worker
            tracing::info!("Successfully joined cluster, waiting for configuration to stabilize");
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            
            // Now register as a worker through Raft
            if let Err(e) = self.register_as_worker().await {
                tracing::error!("Failed to register as worker after joining cluster: {}", e);
                // Don't fail initialization if worker registration fails - the node
                // is still part of the cluster and can participate in consensus
                tracing::warn!("Continuing without worker registration - node can still participate in cluster");
            }
        } else {
            // Bootstrap mode: register as worker immediately
            if let Err(e) = self.register_as_worker().await {
                tracing::error!("Failed to register as worker in bootstrap mode: {}", e);
                // Don't fail initialization if worker registration fails
                tracing::warn!("Continuing without worker registration");
            }
        }
        
        // Mark node as initialized
        self.shared.set_initialized(true).await;
        
        Ok(())
    }


    /// Send join request to a cluster
    pub async fn send_join_request(&self) -> BlixardResult<()> {
        if let Some(join_addr) = &self.shared.config.join_addr {
            tracing::info!("Sending join request to {}", join_addr);
            
            // Get our P2P info if available
            let (p2p_node_id, p2p_addresses, p2p_relay_url) = if let Some(node_addr) = self.shared.get_p2p_node_addr().await {
                let p2p_id = node_addr.node_id.to_string();
                let p2p_addrs: Vec<String> = node_addr.direct_addresses().map(|a| a.to_string()).collect();
                let relay_url = node_addr.relay_url().map(|u| u.to_string());
                tracing::info!("Including our P2P info in join request: node_id={}, addresses={:?}", p2p_id, p2p_addrs);
                (Some(p2p_id), p2p_addrs, relay_url)
            } else {
                tracing::warn!("No P2P node address available for join request");
                (None, Vec::new(), None)
            };
            
            // Create Iroh client to contact the join address
            // Assume the join address is for node 1 (the bootstrap node)
            if let Some(raft_transport) = &self.raft_transport {
                let endpoint = raft_transport.endpoint();
                
                // Parse the join address and create a NodeAddr for the target
                // For now, we'll create a simple connection to the leader
                // In a real implementation, we'd need to discover the leader's Iroh NodeId
                let leader_node_id = 1u64; // Assume node 1 is the initial leader
                
                // For bootstrapping, we need to make an initial connection to get the leader's P2P info
                // We'll use the discovery mechanism if available, or create a temporary client
                
                if let Some(discovery_manager) = self.shared.get_discovery_manager::<crate::discovery::DiscoveryManager>().await {
                    // Try to find the leader's info via discovery
                    let discovered_nodes = discovery_manager.get_nodes().await;
                    
                    // Look for a node that might be the leader (node ID 1 or at the join address)
                    for node_info in discovered_nodes {
                        if node_info.addresses.iter().any(|addr| addr.to_string() == *join_addr) {
                            tracing::info!("Found leader's P2P info via discovery: {}", node_info.node_id);
                            
                            // Create NodeAddr from discovered info
                            let mut node_addr = iroh::NodeAddr::new(node_info.node_id);
                            for addr in &node_info.addresses {
                                node_addr = node_addr.with_direct_addresses([*addr]);
                            }
                            
                            // Store the leader's P2P info
                            self.shared.add_peer_with_p2p(
                                leader_node_id,
                                join_addr.clone(),
                                Some(node_info.node_id.to_string()),
                                node_info.addresses.iter().map(|a| a.to_string()).collect(),
                                None, // TODO: Get relay URL from discovery
                            ).await?;
                            
                            // Now we can make the P2P connection
                            if let Some(p2p_manager) = self.shared.get_p2p_manager().await {
                                tracing::info!("Connecting to leader via P2P at {}", node_info.node_id);
                                p2p_manager.connect_p2p_peer(leader_node_id, &node_addr).await?;
                                
                                // Create P2P client and send join request
                                let (endpoint, _our_node_id) = self.shared.get_iroh_endpoint().await?;
                                let transport_client = crate::transport::iroh_client::IrohClusterServiceClient::new(
                                    Arc::new(endpoint),
                                    node_addr.clone(),
                                );
                                
                                let resp = transport_client.join_cluster(
                                    self.shared.get_id(),
                                    self.shared.get_bind_addr().to_string(),
                                    p2p_node_id.clone(),
                                    p2p_addresses.clone(),
                                    p2p_relay_url.clone(),
                                ).await?;
                                
                                if resp.0 {
                                    tracing::info!("Successfully joined cluster via P2P: {}", resp.1);
                                    
                                    // Store all peer P2P info from the response and establish connections
                                    for peer_info in &resp.2 {
                                        if peer_info.id != self.shared.get_id() && !peer_info.p2p_node_id.is_empty() {
                                            // Store peer info
                                            self.shared.add_peer_with_p2p(
                                                peer_info.id,
                                                peer_info.address.clone(),
                                                Some(peer_info.p2p_node_id.clone()),
                                                peer_info.p2p_addresses.clone(),
                                                if peer_info.p2p_relay_url.is_empty() { None } else { Some(peer_info.p2p_relay_url.clone()) },
                                            ).await?;
                                            
                                            // Establish P2P connection to this peer
                                            if let Ok(node_id_parsed) = peer_info.p2p_node_id.parse::<iroh::NodeId>() {
                                                let mut node_addr = iroh::NodeAddr::new(node_id_parsed);
                                                
                                                // Add relay URL if available
                                                if !peer_info.p2p_relay_url.is_empty() {
                                                    if let Ok(relay_url) = peer_info.p2p_relay_url.parse() {
                                                        node_addr = node_addr.with_relay_url(relay_url);
                                                    }
                                                }
                                                
                                                // Add direct addresses
                                                let addrs: Vec<SocketAddr> = peer_info.p2p_addresses
                                                    .iter()
                                                    .filter_map(|addr_str| addr_str.parse().ok())
                                                    .collect();
                                                if !addrs.is_empty() {
                                                    node_addr = node_addr.with_direct_addresses(addrs);
                                                }
                                                
                                                // Try to connect
                                                tracing::info!("Establishing P2P connection to peer {} after joining cluster", peer_info.id);
                                                if let Some(p2p_manager) = self.shared.get_p2p_manager().await {
                                                    match p2p_manager.connect_p2p_peer(peer_info.id, &node_addr).await {
                                                        Ok(_) => {
                                                            tracing::info!("✅ Successfully connected to P2P peer {}", peer_info.id);
                                                        }
                                                        Err(e) => {
                                                            tracing::warn!("⚠️ Failed to establish P2P connection to peer {}: {}", peer_info.id, e);
                                                        }
                                                    }
                                                } else {
                                                    tracing::warn!("P2P manager not available for establishing connections");
                                                }
                                            }
                                        }
                                    }
                                    
                                    return Ok(());
                                } else {
                                    return Err(BlixardError::ClusterJoin {
                                        reason: resp.1,
                                    });
                                }
                            }
                        }
                    }
                }
                
                // If discovery didn't work, use HTTP bootstrap mechanism
                tracing::info!("Attempting HTTP bootstrap from {}", join_addr);
                
                // Determine if join_addr is already an HTTP URL or a socket address
                let bootstrap_url = if join_addr.starts_with("http://") || join_addr.starts_with("https://") {
                    // Already an HTTP URL, append /bootstrap if not present
                    if join_addr.ends_with("/bootstrap") {
                        join_addr.clone()
                    } else {
                        format!("{}/bootstrap", join_addr.trim_end_matches('/'))
                    }
                } else {
                    // Parse as socket address and construct HTTP URL
                    let addr: SocketAddr = join_addr.parse()
                        .map_err(|e| BlixardError::ConfigError(format!("Invalid join address '{}': {}", join_addr, e)))?;
                    
                    // Get metrics port offset from config
                    let config = crate::config_global::get();
                    let bootstrap_port = addr.port() + config.network.metrics.port_offset;
                    format!("http://{}:{}/bootstrap", addr.ip(), bootstrap_port)
                };
                
                // Fetch bootstrap info
                tracing::debug!("Fetching bootstrap info from {}", bootstrap_url);
                let client = reqwest::Client::new();
                let response = match client.get(&bootstrap_url)
                    .timeout(Duration::from_secs(5))
                    .send()
                    .await {
                    Ok(resp) => resp,
                    Err(e) => {
                        return Err(BlixardError::NetworkError(format!(
                            "Failed to connect to bootstrap endpoint {}: {}", 
                            bootstrap_url, e
                        )));
                    }
                };
                
                if !response.status().is_success() {
                    return Err(BlixardError::ClusterJoin {
                        reason: format!("Bootstrap endpoint returned {}: {}", 
                            response.status(), 
                            response.text().await.unwrap_or_default()),
                    });
                }
                
                let bootstrap_info: crate::iroh_types::BootstrapInfo = response.json().await
                    .map_err(|e| BlixardError::NetworkError(format!("Invalid bootstrap response: {}", e)))?;
                
                tracing::info!("Got bootstrap info: node_id={}, p2p_node_id={}, addresses={:?}", 
                    bootstrap_info.node_id, bootstrap_info.p2p_node_id, bootstrap_info.p2p_addresses);
                
                // Parse the P2P node ID
                let p2p_node_id = bootstrap_info.p2p_node_id.parse::<iroh::NodeId>()
                    .map_err(|e| BlixardError::ConfigError(format!("Invalid P2P node ID: {}", e)))?;
                
                // Create NodeAddr with the bootstrap info
                let mut node_addr = iroh::NodeAddr::new(p2p_node_id);
                // Collect all valid addresses and add them at once
                let addrs: Vec<SocketAddr> = bootstrap_info.p2p_addresses
                    .iter()
                    .filter_map(|addr_str| addr_str.parse().ok())
                    .collect();
                if !addrs.is_empty() {
                    node_addr = node_addr.with_direct_addresses(addrs);
                }
                
                // Add relay URL if available
                if let Some(relay_url) = &bootstrap_info.p2p_relay_url {
                    if let Ok(url) = relay_url.parse() {
                        node_addr = node_addr.with_relay_url(url);
                    }
                }
                
                // Store the leader's P2P info
                self.shared.add_peer_with_p2p(
                    bootstrap_info.node_id,
                    join_addr.clone(),
                    Some(bootstrap_info.p2p_node_id.clone()),
                    bootstrap_info.p2p_addresses.clone(),
                    bootstrap_info.p2p_relay_url.clone(),
                ).await?;
                
                // Now connect via P2P
                if let Some(p2p_manager) = self.shared.get_p2p_manager().await {
                    tracing::info!("Connecting to leader via P2P at {}", node_addr.node_id);
                    p2p_manager.connect_p2p_peer(bootstrap_info.node_id, &node_addr).await?;
                    
                    // Create P2P client and send join request
                    let (endpoint, _our_node_id) = self.shared.get_iroh_endpoint().await?;
                    let transport_client = crate::transport::iroh_client::IrohClusterServiceClient::new(
                        Arc::new(endpoint), 
                        node_addr
                    );
                    
                    // Now proceed with the join request via P2P
                    let our_node_id = self.shared.get_id();
                    let our_bind_address = self.shared.get_bind_addr().to_string();
                    let (our_p2p_node_id, our_p2p_addresses, our_p2p_relay_url) = 
                        if let Some(node_addr) = self.shared.get_p2p_node_addr().await {
                            let p2p_id = node_addr.node_id.to_string();
                            let p2p_addrs: Vec<String> = node_addr.direct_addresses().map(|a| a.to_string()).collect();
                            let relay_url = node_addr.relay_url().map(|u| u.to_string());
                            (Some(p2p_id), p2p_addrs, relay_url)
                        } else {
                            (None, Vec::new(), None)
                        };
                    
                    match transport_client.join_cluster(
                        our_node_id,
                        our_bind_address,
                        our_p2p_node_id,
                        our_p2p_addresses,
                        our_p2p_relay_url
                    ).await {
                        Ok((success, message, peers, voters)) => {
                            if success {
                                tracing::info!("Successfully joined cluster via P2P bootstrap");
                                tracing::info!("Join response contains {} peers and {} voters", 
                                    peers.len(), voters.len());
                                
                                // Update local configuration state with current cluster voters
                                if !voters.is_empty() {
                                    tracing::info!("Updating local configuration with voters: {:?}", voters);
                                    if let Some(db) = self.shared.get_database().await {
                                        let storage = crate::storage::RedbRaftStorage { database: db };
                                        let mut conf_state = raft::prelude::ConfState::default();
                                        conf_state.voters = voters.clone();
                                        if let Err(e) = storage.save_conf_state(&conf_state) {
                                            tracing::warn!("Failed to save initial configuration state: {}", e);
                                        } else {
                                            tracing::info!("Successfully saved initial configuration state with voters: {:?}", voters);
                                        }
                                    } else {
                                        tracing::warn!("No database available to save configuration state");
                                    }
                                } else {
                                    tracing::warn!("Join response contained empty voters list!");
                                }
                                
                                // Store peer information (skip if already exists)
                                for peer in peers {
                                    // Check if peer already exists (e.g., the bootstrap node)
                                    let existing_peers = self.shared.get_peers().await;
                                    if !existing_peers.iter().any(|p| p.id == peer.id) {
                                        self.shared.add_peer_with_p2p(
                                            peer.id,
                                            peer.address.clone(),
                                            Some(peer.p2p_node_id),
                                            peer.p2p_addresses,
                                            Some(peer.p2p_relay_url),
                                        ).await?;
                                    } else {
                                        tracing::debug!("Peer {} already exists, skipping", peer.id);
                                    }
                                }
                                
                                return Ok(());
                            } else {
                                return Err(BlixardError::ClusterJoin {
                                    reason: format!("Join request failed: {}", message),
                                });
                            }
                        }
                        Err(e) => {
                            return Err(BlixardError::ClusterJoin {
                                reason: format!("Failed to send join request via P2P: {}", e),
                            });
                        }
                    }
                } else {
                    return Err(BlixardError::Internal {
                        message: "P2P manager not available".to_string(),
                    });
                }
            }
            
            /* Original commented code for reference
            let join_request = crate::iroh_types::JoinRequest {
                node_id: self.shared.get_id(),
                bind_address: self.shared.get_bind_addr().to_string(),
            };
            
            match client.join_cluster(join_request).await {
                Ok(resp) => {
                    if resp.success {
                                        tracing::info!("Successfully joined cluster at {}", join_addr);
                                        tracing::info!("Join response contains {} peers and {} voters", resp.peers.len(), resp.voters.len());
                                        
                                        // Update local configuration state with current cluster voters
                                        if !resp.voters.is_empty() {
                                            tracing::info!("Updating local configuration with voters: {:?}", resp.voters);
                                            if let Some(db) = self.shared.get_database().await {
                                                let storage = crate::storage::RedbRaftStorage { database: db };
                                                let mut conf_state = raft::prelude::ConfState::default();
                                                conf_state.voters = resp.voters.clone();
                                                if let Err(e) = storage.save_conf_state(&conf_state) {
                                                    tracing::warn!("Failed to save initial configuration state: {}", e);
                                                } else {
                                                    tracing::info!("Successfully saved initial configuration state with voters: {:?}", resp.voters);
                                                }
                                            } else {
                                                tracing::warn!("No database available to save configuration state");
                                            }
                                        } else {
                                            tracing::warn!("Join response contained empty voters list!");
                                        }
                                        
                                        // Add peers from response
                                        for peer in resp.peers {
                                            if peer.id != self.shared.get_id() {
                                                tracing::info!("Adding peer {} at {} from join response", peer.id, peer.address);
                                                let _ = self.shared.add_peer(peer.id, peer.address).await;
                                            } else {
                                                tracing::info!("Skipping self (node {}) in peer list", peer.id);
                                            }
                                        }
                    } else {
                        return Err(BlixardError::ClusterJoin {
                            reason: resp.message,
                        });
                    }
                }
                Err(e) => {
                    return Err(BlixardError::ClusterJoin {
                        reason: format!("Failed to send join request: {}", e),
                    });
                }
            }
            */
        }
        Ok(())
    }

    /// Send a VM command for processing
    pub async fn send_vm_command(&self, command: VmCommand) -> BlixardResult<()> {
        self.shared.send_vm_command(command).await
    }

    /// Get the node ID
    pub fn get_id(&self) -> u64 {
        self.shared.get_id()
    }

    /// Get the bind address
    pub fn get_bind_addr(&self) -> &std::net::SocketAddr {
        self.shared.get_bind_addr()
    }

    /// List all VMs and their status
    pub async fn list_vms(&self) -> BlixardResult<Vec<(crate::types::VmConfig, crate::types::VmStatus)>> {
        self.shared.list_vms().await
    }

    /// Get status of a specific VM
    pub async fn get_vm_status(&self, name: &str) -> BlixardResult<Option<(crate::types::VmConfig, crate::types::VmStatus)>> {
        self.shared.get_vm_status(name).await
    }
    
    /// Get the IP address of a VM
    pub async fn get_vm_ip(&self, name: &str) -> BlixardResult<Option<String>> {
        self.shared.get_vm_ip(name).await
    }

    /// Join a cluster and optionally register as a worker
    pub async fn join_cluster(&mut self, peer_addr: Option<std::net::SocketAddr>) -> BlixardResult<()> {
        // Check if initialized
        if !self.shared.is_initialized().await {
            return Err(BlixardError::Internal {
                message: "Node not initialized".to_string(),
            });
        }
        
        if peer_addr.is_none() {
            // Bootstrap mode: register as worker immediately
            self.register_as_worker().await?;
        } else {
            // Join existing cluster - worker registration will happen later
            // after the node has successfully joined and can identify the leader
            tracing::info!("Joining existing cluster at {:?} - worker registration deferred", peer_addr);
        }
        
        Ok(())
    }
    
    /// Register this node as a worker in the cluster
    pub async fn register_as_worker(&self) -> BlixardResult<()> {
        let node_id = self.shared.get_id();
        let address = self.shared.get_bind_addr().to_string();
        // Get system resources - use reasonable defaults
        let total_memory_mb = Self::estimate_available_memory();
        let disk_gb = Self::estimate_available_disk(&self.shared.config.data_dir);
        
        let capabilities = crate::raft_manager::WorkerCapabilities {
            cpu_cores: num_cpus::get() as u32,
            memory_mb: total_memory_mb,
            disk_gb,
            features: vec!["microvm".to_string()],
        };
        
        // Get topology from node config
        let topology = self.shared.config.topology.clone();
        
        // Check if we're the bootstrap node (no join_addr in config)
        if self.shared.config.join_addr.is_none() {
            // Bootstrap mode: When starting as a single-node cluster, we can write
            // directly to the database. This is the ONLY exception to the rule that
            // all state must go through Raft consensus.
            if let Some(db) = self.shared.get_database().await {
                let read_txn = db.begin_read()?;
                let worker_table = read_txn.open_table(crate::storage::WORKER_TABLE)?;
                
                // Check if already registered
                if worker_table.get(node_id.to_le_bytes().as_slice())?.is_none() {
                    drop(read_txn);
                    
                    // Not registered yet, register now
                    let write_txn = db.begin_write()?;
                    {
                        let mut worker_table = write_txn.open_table(crate::storage::WORKER_TABLE)?;
                        let mut status_table = write_txn.open_table(crate::storage::WORKER_STATUS_TABLE)?;
                        
                        let worker_data = bincode::serialize(&(address.clone(), &capabilities))?;
                        worker_table.insert(node_id.to_le_bytes().as_slice(), worker_data.as_slice())?;
                        status_table.insert(node_id.to_le_bytes().as_slice(), [crate::raft_manager::WorkerStatus::Online as u8].as_slice())?;
                    }
                    write_txn.commit()?;
                    
                    tracing::info!("Node {} registered as worker in bootstrap mode", node_id);
                } else {
                    tracing::info!("Node {} already registered as worker", node_id);
                }
            }
        } else {
            // Join existing cluster via Raft proposal - use the new method
            self.shared.register_worker_through_raft(node_id, address, capabilities, topology).await?;
        }
        
        Ok(())
    }

    /// Leave the cluster
    pub async fn leave_cluster(&mut self) -> BlixardResult<()> {
        // Check if initialized
        if !self.shared.is_initialized().await {
            return Err(BlixardError::Internal {
                message: "Node not initialized".to_string(),
            });
        }
        
        {
            let proposal_data = ProposalData::RemoveWorker {
                node_id: self.shared.get_id(),
            };
            
            let (response_tx, response_rx) = oneshot::channel();
            let proposal = RaftProposal {
                id: uuid::Uuid::new_v4().as_bytes().to_vec(),
                data: proposal_data,
                response_tx: Some(response_tx),
            };
            
            self.shared.send_raft_proposal(proposal).await?;
            
            // Wait for response
            response_rx.await.map_err(|_| BlixardError::Internal {
                message: "Leave proposal response channel closed".to_string(),
            })??;
        }
        
        Ok(())
    }

    /// Get cluster status
    pub async fn get_cluster_status(&self) -> BlixardResult<(u64, Vec<u64>, u64)> {
        self.shared.get_cluster_status().await
    }

    /// Start the node
    pub async fn start(&mut self) -> BlixardResult<()> {
        let node_id = self.shared.get_id();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        self.shared.set_shutdown_tx(shutdown_tx).await;

        // The node doesn't need its own TCP listener since all communication
        // is handled via the gRPC server. We just need a task to manage the
        // node's lifecycle and respond to shutdown signals.
        let handle = tokio::spawn(async move {
            tracing::info!("Node {} started", node_id);

            // Wait for shutdown signal
            let _ = shutdown_rx.await;
            tracing::info!("Node {} shutting down", node_id);

            Ok(())
        });

        self.handle = Some(handle);
        self.shared.set_running(true).await;
        
        // Start VM health monitor
        if let Some(ref mut monitor) = self.health_monitor {
            monitor.start();
            tracing::info!("VM health monitor started");
        }
        
        Ok(())
    }

    /// Stop the node
    pub async fn stop(&mut self) -> BlixardResult<()> {
        if let Some(tx) = self.shared.take_shutdown_tx().await {
            let _ = tx.send(());
        }
        
        // Stop main node handle
        if let Some(handle) = self.handle.take() {
            match handle.await {
                Ok(result) => result?,
                Err(_) => {}, // Task was cancelled
            }
        }
        
        // Stop Raft handle
        if let Some(handle) = self.raft_handle.take() {
            handle.abort(); // Abort the Raft task
        }
        
        // Stop VM health monitor
        if let Some(ref mut monitor) = self.health_monitor {
            monitor.stop();
            tracing::info!("VM health monitor stopped");
        }
        
        // Shutdown Raft transport
        if let Some(transport) = self.raft_transport.take() {
            transport.shutdown().await;
            tracing::info!("Raft transport shut down");
        }
        
        self.shared.set_running(false).await;
        self.shared.set_initialized(false).await;
        
        // Shutdown all components to release database references
        self.shared.shutdown_components().await;
        
        // Add a small delay to ensure file locks are released
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        Ok(())
    }

    /// Check if the node is running
    pub async fn is_running(&self) -> bool {
        self.shared.is_running().await
    }
    
    /// Check if this node is the Raft leader
    pub async fn is_leader(&self) -> bool {
        self.shared.is_leader().await
    }
    
    /// Send a Raft message to the Raft manager
    pub async fn send_raft_message(&self, from: u64, msg: raft::prelude::Message) -> BlixardResult<()> {
        self.shared.send_raft_message(from, msg).await
    }
    
    /// Submit a task to the cluster
    pub async fn submit_task(&self, task_id: &str, task: crate::raft_manager::TaskSpec) -> BlixardResult<u64> {
        self.shared.submit_task(task_id, task).await
    }
    
    /// Get task status
    pub async fn get_task_status(&self, task_id: &str) -> BlixardResult<Option<(String, Option<crate::raft_manager::TaskResult>)>> {
        self.shared.get_task_status(task_id).await
    }
    
    /// Submit VM migration
    pub async fn submit_vm_migration(&self, task: crate::types::VmMigrationTask) -> BlixardResult<()> {
        use crate::raft_manager::ProposalData;
        
        let proposal_data = ProposalData::MigrateVm {
            vm_name: task.vm_name.clone(),
            from_node: task.source_node_id,
            to_node: task.target_node_id,
        };
        
        let (response_tx, response_rx) = oneshot::channel();
        let proposal = RaftProposal {
            id: uuid::Uuid::new_v4().as_bytes().to_vec(),
            data: proposal_data,
            response_tx: Some(response_tx),
        };
        
        self.shared.send_raft_proposal(proposal).await?;
        
        // Wait for response
        response_rx.await.map_err(|_| BlixardError::Internal {
            message: "Migration proposal response channel closed".to_string(),
        })??;
        
        Ok(())
    }
    
    /// Estimate available memory in MB
    /// Returns a conservative default if detection fails
    fn estimate_available_memory() -> u64 {
        // Try to read from /proc/meminfo on Linux
        #[cfg(target_os = "linux")]
        {
            if let Ok(contents) = std::fs::read_to_string("/proc/meminfo") {
                for line in contents.lines() {
                    if line.starts_with("MemTotal:") {
                        if let Some(kb_str) = line.split_whitespace().nth(1) {
                            if let Ok(kb) = kb_str.parse::<u64>() {
                                return kb / 1024; // Convert KB to MB
                            }
                        }
                    }
                }
            }
        }
        
        // Default from configuration if detection fails
        crate::config_global::get().cluster.worker.default_memory_mb
    }
    
    /// Estimate available disk space in GB
    /// Returns a conservative default if detection fails
    fn estimate_available_disk(data_dir: &str) -> u64 {
        // Try to get filesystem stats
        if let Ok(_metadata) = std::fs::metadata(data_dir) {
            // On Unix systems, we could use statvfs, but for now use a default
            // In production, use a proper system info crate
            return 100; // Default to 100GB
        }
        
        // Create directory if it doesn't exist and return default
        let _ = std::fs::create_dir_all(data_dir);
        crate::config_global::get().cluster.worker.default_disk_gb
    }
    
    /// Create discovery configuration from node config
    fn create_discovery_config(config: &NodeConfig) -> BlixardResult<crate::discovery::DiscoveryConfig> {
        let mut discovery_config = crate::discovery::DiscoveryConfig::default();
        
        // If we have a join address, add it as a static node
        if let Some(join_addr) = &config.join_addr {
            // Try to parse the join address to get the node ID
            // For now, we'll use "bootstrap" as the key
            discovery_config.static_nodes.insert(
                "bootstrap".to_string(),
                vec![join_addr.clone()],
            );
        }
        
        // Check environment for additional static nodes
        if let Ok(static_nodes) = std::env::var("BLIXARD_STATIC_NODES") {
            // Format: node1=addr1,node2=addr2
            for entry in static_nodes.split(',') {
                if let Some((node_id, addr)) = entry.split_once('=') {
                    discovery_config.static_nodes.insert(
                        node_id.trim().to_string(),
                        vec![addr.trim().to_string()],
                    );
                }
            }
        }
        
        // Enable/disable discovery methods based on environment
        if let Ok(enable_dns) = std::env::var("BLIXARD_ENABLE_DNS_DISCOVERY") {
            discovery_config.enable_dns = enable_dns.to_lowercase() == "true";
        }
        
        if let Ok(enable_mdns) = std::env::var("BLIXARD_ENABLE_MDNS_DISCOVERY") {
            discovery_config.enable_mdns = enable_mdns.to_lowercase() == "true";
        }
        
        Ok(discovery_config)
    }
    
    /// Initialize VM backend with custom registry (for applications to register backends)
    pub async fn initialize_with_vm_registry(&mut self, registry: VmBackendRegistry) -> BlixardResult<()> {
        // Call the internal initialization with the custom registry
        self.initialize_internal(Some(registry)).await
    }
    
    /// Create a VM backend using the factory pattern
    fn create_vm_backend(
        &self, 
        registry: &VmBackendRegistry, 
        database: Arc<Database>
    ) -> BlixardResult<Arc<dyn crate::vm_backend::VmBackend>> {
        use std::path::PathBuf;
        
        let data_dir = &self.shared.config.data_dir;
        let vm_config_dir = std::fs::canonicalize(PathBuf::from(data_dir).join("vm-configs"))
            .unwrap_or_else(|_| {
                std::env::current_dir()
                    .map(|cwd| cwd.join(data_dir).join("vm-configs"))
                    .unwrap_or_else(|_| PathBuf::from(data_dir).join("vm-configs"))
            });
        let vm_data_dir = std::fs::canonicalize(PathBuf::from(data_dir).join("vm-data"))
            .unwrap_or_else(|_| {
                std::env::current_dir()
                    .map(|cwd| cwd.join(data_dir).join("vm-data"))
                    .unwrap_or_else(|_| PathBuf::from(data_dir).join("vm-data"))
            });
        
        // Create directories
        std::fs::create_dir_all(&vm_config_dir)
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to create VM config directory: {}", e),
            })?;
        std::fs::create_dir_all(&vm_data_dir)
            .map_err(|e| BlixardError::Internal {
                message: format!("Failed to create VM data directory: {}", e),
            })?;
        
        let backend_type = &self.shared.config.vm_backend;
        tracing::info!("Creating VM backend of type: {}", backend_type);
        
        registry.create_backend(backend_type, vm_config_dir, vm_data_dir, database)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_node_lifecycle() {
        let config = NodeConfig {
            id: 1,
            data_dir: "/tmp/test".to_string(),
            bind_addr: "127.0.0.1:0".parse().expect("valid test address"),
            join_addr: None,
            use_tailscale: false,
            vm_backend: "mock".to_string(),
            transport_config: None,
            topology: Default::default(),
        };

        let mut node = Node::new(config);
        
        // Start node
        node.start().await.expect("node should start");
        assert!(node.is_running().await);

        // Stop node
        node.stop().await.expect("node should stop");
        assert!(!node.is_running().await);
    }
}

