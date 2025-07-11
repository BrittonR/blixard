use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::error::{BlixardError, BlixardResult};
use crate::node_shared::SharedNodeState;
use crate::p2p_manager::{ConnectionQuality, PeerInfo};
use crate::raft_manager::{ProposalData, RaftConfChange, RaftManager};
use crate::raft::messages::RaftProposal;
use crate::types::{NodeConfig, VmCommand};
use crate::vm_backend::{VmBackendRegistry, VmManager};
use crate::vm_health_monitor::VmHealthMonitor;

use redb::{Database, ReadableTable};

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
    async fn initialize_internal(
        &mut self,
        registry_opt: Option<VmBackendRegistry>,
    ) -> BlixardResult<()> {
        // Phase 1: Database initialization
        let db = self.setup_database().await?;

        // Phase 2: VM infrastructure
        let registry = registry_opt.unwrap_or_else(VmBackendRegistry::default);
        self.setup_vm_infrastructure(registry, db.clone()).await?;

        // Phase 3: Storage managers (quota and IP pool)
        self.setup_storage_managers(db.clone()).await?;

        // Phase 4: Security and observability
        self.setup_security_and_observability().await?;

        // Phase 5: P2P infrastructure
        self.setup_p2p_infrastructure().await?;

        // Phase 6: Raft initialization
        let (raft_manager, _message_rx, _, proposal_tx, _conf_change_tx) = 
            self.initialize_raft(db.clone()).await?;

        // Phase 7: Transport setup
        let (message_tx, _unused_rx) = mpsc::unbounded_channel();
        
        // Set the message tx in shared state
        self.shared.set_raft_message_tx(message_tx.clone());
        
        let (raft_transport, outgoing_rx) = 
            self.setup_transport(message_tx.clone(), conf_change_tx, proposal_tx).await?;

        // Phase 8: Spawn Raft message handler
        self.spawn_raft_message_handler(raft_transport, outgoing_rx);

        // Phase 9: Spawn Raft manager with automatic recovery
        let raft_handle = self.spawn_raft_manager_with_recovery(raft_manager);
        self.raft_handle = Some(raft_handle);

        // Phase 10: Perform cluster join if configured
        self.join_cluster_if_configured().await?;

        // Mark node as initialized
        self.shared.set_initialized(true);

        Ok(())
    }

    /// Set up database infrastructure
    async fn setup_database(&self) -> BlixardResult<Arc<Database>> {
        let data_dir = &self.shared.config.data_dir;

        // Ensure the data directory exists
        std::fs::create_dir_all(data_dir).map_err(|e| BlixardError::Storage {
            operation: "create data directory".to_string(),
            source: Box::new(e),
        })?;

        let db_path = format!("{}/blixard.db", data_dir);

        // Try to open existing database first, create if it doesn't exist
        let database = match std::fs::metadata(&db_path) {
            Ok(_) => {
                // Database exists, open it
                Database::open(&db_path).map_err(|e| BlixardError::Storage {
                    operation: "open database".to_string(),
                    source: Box::new(e),
                })?
            }
            Err(_) => {
                // Database doesn't exist, create it
                Database::create(&db_path).map_err(|e| BlixardError::Storage {
                    operation: "create database".to_string(),
                    source: Box::new(e),
                })?
            }
        };
        
        let db_arc = Arc::new(database);

        // Initialize all database tables
        crate::raft_storage::init_database_tables(&db_arc)?;

        self.shared.set_database(Some(db_arc.clone())).await;
        
        Ok(db_arc)
    }

    /// Set up VM infrastructure (manager and health monitor)
    async fn setup_vm_infrastructure(
        &mut self,
        registry: VmBackendRegistry,
        db: Arc<Database>,
    ) -> BlixardResult<()> {
        // Create VM backend
        let vm_backend = self.create_vm_backend(&registry, db.clone())?;
        
        // Store VM backend in shared state
        self.shared.set_vm_manager(vm_backend.clone());
        
        // Initialize VM manager
        let vm_manager = Arc::new(VmManager::new(
            db.clone(),
            vm_backend,
            self.shared.clone(),
        ));

        // Recover persisted VMs
        if let Err(e) = vm_manager.recover_persisted_vms().await {
            tracing::error!("Failed to recover persisted VMs: {}", e);
            // Continue initialization even if recovery fails
        }

        // Initialize VM health monitor
        let health_monitor = VmHealthMonitor::new(
            self.shared.clone(),
            vm_manager.clone(),
            Duration::from_secs(30),
        );
        self.health_monitor = Some(health_monitor);

        Ok(())
    }

    /// Set up storage infrastructure (quota and IP pool managers)
    async fn setup_storage_managers(&self, db: Arc<Database>) -> BlixardResult<()> {
        // Initialize quota manager
        let storage = Arc::new(crate::raft_storage::RedbRaftStorage {
            database: db.clone(),
        });
        let quota_manager = Arc::new(crate::quota_manager::QuotaManager::new(storage).await?);
        self.shared.set_quota_manager(quota_manager);

        // Initialize IP pool manager
        let ip_pool_manager = Arc::new(crate::ip_pool_manager::IpPoolManager::new());

        // Load existing pools and allocations from database
        let pools = {
            let read_txn = db.begin_read().map_err(|e| BlixardError::Storage {
                operation: "begin read transaction".to_string(),
                source: Box::new(e),
            })?;
            
            let mut pools = Vec::new();
            if let Ok(table) = read_txn.open_table(crate::raft_storage::IP_POOL_TABLE) {
                for entry in table.iter().map_err(|e| BlixardError::Storage {
                    operation: "iterate IP pools".to_string(),
                    source: Box::new(e),
                })? {
                    let (_, value) = entry.map_err(|e| BlixardError::Storage {
                        operation: "read IP pool entry".to_string(),
                        source: Box::new(e),
                    })?;
                    
                    let pool: crate::ip_pool::IpPoolConfig = bincode::deserialize(value.value())
                        .map_err(|e| BlixardError::Serialization {
                            operation: "deserialize IP pool".to_string(),
                            source: Box::new(e),
                        })?;
                    pools.push(pool);
                }
            }
            pools
        };
        
        let allocations = {
            let read_txn = db.begin_read().map_err(|e| BlixardError::Storage {
                operation: "begin read transaction".to_string(),
                source: Box::new(e),
            })?;
            
            let mut allocations = Vec::new();
            if let Ok(table) = read_txn.open_table(crate::raft_storage::IP_ALLOCATION_TABLE) {
                for entry in table.iter().map_err(|e| BlixardError::Storage {
                    operation: "iterate IP allocations".to_string(),
                    source: Box::new(e),
                })? {
                    let (_, value) = entry.map_err(|e| BlixardError::Storage {
                        operation: "read IP allocation entry".to_string(),
                        source: Box::new(e),
                    })?;
                    
                    let allocation: crate::ip_pool::IpAllocation = bincode::deserialize(value.value())
                        .map_err(|e| BlixardError::Serialization {
                            operation: "deserialize IP allocation".to_string(),
                            source: Box::new(e),
                        })?;
                    allocations.push(allocation);
                }
            }
            allocations
        };
        
        ip_pool_manager.load_from_storage(pools, allocations).await?;
        self.shared.set_ip_pool_manager(ip_pool_manager);
        
        tracing::info!("Initialized IP pool manager");
        Ok(())
    }

    /// Set up security and observability infrastructure
    async fn setup_security_and_observability(&self) -> BlixardResult<()> {
        // Phase 4: Security and observability - placeholder for future implementation
        // This could include metrics initialization, tracing setup, etc.
        tracing::info!("Security and observability infrastructure initialized");
        Ok(())
    }

    /// Set up P2P infrastructure if enabled
    async fn setup_p2p_infrastructure(&mut self) -> BlixardResult<()> {
        let _config = crate::config_global::get()?;
        
        // P2P manager initialization - placeholder for future enhancement
        // The Iroh transport handles P2P functionality directly
        tracing::info!("P2P infrastructure initialized via Iroh transport");

        Ok(())
    }

    /// Register bootstrap worker in database
    async fn register_bootstrap_worker(&self, db: Arc<Database>) -> BlixardResult<()> {
        let txn = db.begin_write().map_err(|e| BlixardError::Storage {
            operation: "begin write transaction".to_string(),
            source: Box::new(e),
        })?;

        // Register this node as a worker
        let capabilities = crate::raft_manager::WorkerCapabilities {
            cpu_cores: num_cpus::get() as u32,
            memory_mb: Self::estimate_available_memory(),
            disk_gb: Self::estimate_available_disk(&self.shared.config.data_dir),
            features: match self.shared.config.vm_backend.as_str() {
                "microvm" => vec!["microvm".to_string()],
                "docker" => vec!["container".to_string()],
                "mock" => vec!["mock".to_string()],
                _ => vec![],
            },
        };

        // For bootstrap, we'll store the capabilities directly
        let worker_data = bincode::serialize(&capabilities).map_err(|e| BlixardError::Serialization {
            operation: "serialize worker capabilities".to_string(),
            source: Box::new(e),
        })?;

        {
            let mut worker_table = txn.open_table(crate::raft_storage::WORKER_TABLE)?;
            let key_bytes = self.shared.config.id.to_be_bytes();
            worker_table.insert(key_bytes.as_slice(), worker_data.as_slice())?;
        }

        txn.commit().map_err(|e| BlixardError::Storage {
            operation: "commit worker registration".to_string(),
            source: Box::new(e),
        })?;

        tracing::info!(
            "Registered bootstrap node {} as worker with {} CPUs, {} MB memory, {} GB disk",
            self.shared.config.id,
            capabilities.cpu_cores,
            capabilities.memory_mb,
            capabilities.disk_gb
        );

        Ok(())
    }

    /// Initialize Raft manager and channels
    async fn initialize_raft(
        &mut self,
        db: Arc<Database>,
    ) -> BlixardResult<(RaftManager, mpsc::UnboundedReceiver<(u64, raft::prelude::Message)>, mpsc::UnboundedReceiver<RaftConfChange>, mpsc::UnboundedSender<RaftProposal>, mpsc::UnboundedSender<RaftConfChange>)> {
        let joining_cluster = self.shared.config.join_addr.is_some();
        let mut storage = crate::raft_storage::RedbRaftStorage {
            database: db.clone(),
        };

        // Initialize based on whether we're joining or bootstrapping
        if !joining_cluster {
            // Bootstrap mode - initialize storage with this node
            tracing::info!("Bootstrapping new cluster with node {}", self.shared.config.id);
            storage.initialize_single_node(self.shared.config.id)?;
            
            // Register as worker in bootstrap mode
            self.register_bootstrap_worker(db.clone()).await?;
        } else {
            // Join mode - storage will be initialized during join
            tracing::info!("Preparing to join existing cluster");
        }

        // Create Raft manager
        let (raft_manager, proposal_tx, _message_tx, conf_change_tx, message_rx) = RaftManager::new(
            self.shared.config.id,
            db.clone(),
            vec![], // No initial peers for bootstrap/join
            Arc::downgrade(&self.shared),
        )?;
        
        // Set up batch processing if enabled  
        let final_proposal_tx = self.setup_batch_processing(proposal_tx.clone())?;
        
        // Store the proposal tx in shared state
        self.shared.set_raft_proposal_tx(final_proposal_tx.clone());

        // Create a conf_change_rx for the return value (not used in this refactored version)
        let (_, conf_change_rx) = mpsc::unbounded_channel();

        Ok((raft_manager, message_rx, conf_change_rx, final_proposal_tx, conf_change_tx))
    }

    /// Set up batch processing for Raft proposals if enabled
    fn setup_batch_processing(
        &self,
        proposal_tx: mpsc::UnboundedSender<RaftProposal>,
    ) -> BlixardResult<mpsc::UnboundedSender<RaftProposal>> {
        let config = crate::config_global::get()?;
        let batch_config = crate::raft_batch_processor::BatchConfig {
            enabled: config.cluster.raft.batch_processing.enabled,
            max_batch_size: config.cluster.raft.batch_processing.max_batch_size,
            batch_timeout_ms: config.cluster.raft.batch_processing.batch_timeout_ms,
            max_batch_bytes: config.cluster.raft.batch_processing.max_batch_bytes,
        };

        if batch_config.enabled {
            let (batch_tx, batch_processor) =
                crate::raft_batch_processor::create_batch_processor(batch_config, proposal_tx, self.shared.get_id());
            
            tokio::spawn(async move {
                batch_processor.run().await;
                tracing::debug!("Batch processor completed");
            });

            Ok(batch_tx)
        } else {
            Ok(proposal_tx)
        }
    }

    /// Send join request to a cluster
    pub async fn send_join_request(&self) -> BlixardResult<()> {
        if let Some(join_addr) = &self.shared.config.join_addr {
            tracing::info!("Sending join request to {}", join_addr);

            // Get our P2P info if available
            let (p2p_node_id, p2p_addresses, p2p_relay_url) = self.gather_local_p2p_info().await;

            // Create Iroh client to contact the join address
            // Assume the join address is for node 1 (the bootstrap node)
            if let Some(raft_transport) = &self.raft_transport {
                let _endpoint = raft_transport.endpoint();

                // Parse the join address and create a NodeAddr for the target
                // For now, we'll create a simple connection to the leader
                // In a real implementation, we'd need to discover the leader's Iroh NodeId
                let leader_node_id = 1u64; // Assume node 1 is the initial leader

                // For bootstrapping, we need to make an initial connection to get the leader's P2P info
                // We'll use the discovery mechanism if available, or create a temporary client

                // Try discovery-based join first
                if let Ok(result) = self.attempt_discovery_based_join(join_addr, leader_node_id, &p2p_node_id, &p2p_addresses, &p2p_relay_url).await {
                    return Ok(result);
                }

                // If discovery didn't work, use HTTP bootstrap mechanism
                let bootstrap_url = self.build_bootstrap_url(join_addr).await?;

                // Fetch bootstrap info
                let bootstrap_info = self.fetch_bootstrap_info(&bootstrap_url).await?;

                // Create NodeAddr from bootstrap info
                let node_addr = self.create_p2p_node_addr(&bootstrap_info).await?;

                // Store the leader's P2P info
                let peer_info = PeerInfo {
                    node_id: bootstrap_info.p2p_node_id.clone(),
                    address: join_addr.clone(),
                    last_seen: chrono::Utc::now(),
                    capabilities: vec![],
                    shared_resources: HashMap::new(),
                    connection_quality: ConnectionQuality {
                        latency_ms: 0,
                        bandwidth_mbps: 100.0,
                        packet_loss: 0.0,
                        reliability_score: 1.0,
                    },
                };
                self.shared
                    .add_peer_with_p2p(bootstrap_info.node_id, peer_info);

                // Execute HTTP bootstrap join
                self.execute_http_bootstrap_join(&bootstrap_info, &node_addr, &p2p_node_id, &p2p_addresses, &p2p_relay_url).await?;
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
                                                let storage = crate::raft_storage::RedbRaftStorage { database: db };
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
                                                let peer_info = PeerInfo {
                                                    node_id: peer.id.to_string(),
                                                    address: peer.address.clone(),
                                                    last_seen: chrono::Utc::now(),
                                                    capabilities: vec![],
                                                    shared_resources: HashMap::new(),
                                                    connection_quality: ConnectionQuality {
                        latency_ms: 0,
                        bandwidth_mbps: 100.0,
                        packet_loss: 0.0,
                        reliability_score: 1.0,
                    },
                                                };
                                                self.shared.add_peer(peer.id, peer_info);
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

    /// Gather local P2P info if available
    async fn gather_local_p2p_info(&self) -> (Option<String>, Vec<String>, Option<String>) {
        if let Some(node_addr) = self.shared.get_p2p_node_addr() {
            let p2p_id = node_addr.node_id.to_string();
            let p2p_addrs: Vec<String> = node_addr
                .direct_addresses()
                .map(|a| a.to_string())
                .collect();
            let relay_url = node_addr.relay_url().map(|u| u.to_string());
            (Some(p2p_id), p2p_addrs, relay_url)
        } else {
            (None, Vec::new(), None)
        }
    }

    /// Build bootstrap URL from join address
    async fn build_bootstrap_url(&self, join_addr: &str) -> BlixardResult<String> {
        tracing::info!("Attempting HTTP bootstrap from {}", join_addr);
        
        // Determine if join_addr is already an HTTP URL or a socket address
        let bootstrap_url = if join_addr.starts_with("http://") || join_addr.starts_with("https://") {
            // Already an HTTP URL, append /bootstrap if not present
            if join_addr.ends_with("/bootstrap") {
                join_addr.to_string()
            } else {
                format!("{}/bootstrap", join_addr.trim_end_matches('/'))
            }
        } else {
            // Parse as socket address and construct HTTP URL
            let addr: SocketAddr = join_addr.parse().map_err(|e| {
                BlixardError::ConfigError(format!(
                    "Invalid join address '{}': {}",
                    join_addr, e
                ))
            })?;

            // Get metrics port offset from config
            let config = crate::config_global::get()?;
            let bootstrap_port = addr.port() + config.network.metrics.port_offset;
            format!("http://{}:{}/bootstrap", addr.ip(), bootstrap_port)
        };
        
        Ok(bootstrap_url)
    }

    /// Fetch bootstrap info from HTTP endpoint
    async fn fetch_bootstrap_info(&self, bootstrap_url: &str) -> BlixardResult<crate::iroh_types::BootstrapInfo> {
        tracing::debug!("Fetching bootstrap info from {}", bootstrap_url);
        let client = reqwest::Client::new();
        let response = match client
            .get(bootstrap_url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
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
                reason: format!(
                    "Bootstrap endpoint returned {}: {}",
                    response.status(),
                    response.text().await.unwrap_or_default()
                ),
            });
        }

        let bootstrap_info: crate::iroh_types::BootstrapInfo =
            response.json().await.map_err(|e| {
                BlixardError::NetworkError(format!("Invalid bootstrap response: {}", e))
            })?;

        tracing::info!(
            "Got bootstrap info: node_id={}, p2p_node_id={}, addresses={:?}",
            bootstrap_info.node_id,
            bootstrap_info.p2p_node_id,
            bootstrap_info.p2p_addresses
        );
        
        Ok(bootstrap_info)
    }

    /// Create P2P NodeAddr from bootstrap info
    async fn create_p2p_node_addr(&self, bootstrap_info: &crate::iroh_types::BootstrapInfo) -> BlixardResult<iroh::NodeAddr> {
        // Parse the P2P node ID
        let p2p_node_id = bootstrap_info
            .p2p_node_id
            .parse::<iroh::NodeId>()
            .map_err(|e| {
                BlixardError::ConfigError(format!("Invalid P2P node ID: {}", e))
            })?;

        // Create NodeAddr with the bootstrap info
        let mut node_addr = iroh::NodeAddr::new(p2p_node_id);
        // Collect all valid addresses and add them at once
        let addrs: Vec<SocketAddr> = bootstrap_info
            .p2p_addresses
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
        
        Ok(node_addr)
    }

    /// Attempt discovery-based join to cluster
    async fn attempt_discovery_based_join(
        &self,
        join_addr: &str,
        leader_node_id: u64,
        p2p_node_id: &Option<String>,
        p2p_addresses: &[String],
        p2p_relay_url: &Option<String>,
    ) -> BlixardResult<()> {
        if let Some(discovery_manager) = self
            .shared
            .get_discovery_manager::<crate::discovery::DiscoveryManager>()
        {
            // Try to find the leader's info via discovery
            let discovered_nodes = discovery_manager.get_nodes().await;

            // Look for a node that might be the leader (node ID 1 or at the join address)
            for node_info in discovered_nodes {
                if node_info
                    .addresses
                    .iter()
                    .any(|addr| addr.to_string() == *join_addr)
                {
                    tracing::info!(
                        "Found leader's P2P info via discovery: {}",
                        node_info.node_id
                    );

                    // Create NodeAddr from discovered info
                    let mut node_addr = iroh::NodeAddr::new(node_info.node_id);
                    for addr in &node_info.addresses {
                        node_addr = node_addr.with_direct_addresses([*addr]);
                    }

                    // Store the leader's P2P info
                    let peer_info = PeerInfo {
                        node_id: node_info.node_id.to_string(),
                        address: join_addr.to_string(),
                        last_seen: chrono::Utc::now(),
                        capabilities: vec![],
                        shared_resources: HashMap::new(),
                        connection_quality: ConnectionQuality {
                        latency_ms: 0,
                        bandwidth_mbps: 100.0,
                        packet_loss: 0.0,
                        reliability_score: 1.0,
                    },
                    };
                    self.shared
                        .add_peer_with_p2p(leader_node_id, peer_info);

                    // Now we can make the P2P connection
                    if let Some(p2p_manager) = self.shared.get_p2p_manager() {
                        tracing::info!(
                            "Connecting to leader via P2P at {}",
                            node_info.node_id
                        );
                        p2p_manager
                            .connect_p2p_peer(leader_node_id, &node_addr)
                            .await?;

                        // Execute the P2P join request
                        let (success, message, peers, voters) = self.execute_p2p_join_request(
                            &node_addr,
                            p2p_node_id,
                            p2p_addresses,
                            p2p_relay_url,
                        ).await?;

                        if success {
                            tracing::info!(
                                "Successfully joined cluster via P2P: {}",
                                message
                            );

                            // Update cluster configuration
                            self.update_cluster_configuration(&voters).await?;

                            // Store peer information from response
                            self.store_peer_information(&peers).await?;

                            return Ok(());
                        } else {
                            return Err(BlixardError::ClusterJoin { reason: message });
                        }
                    }
                }
            }
        }
        
        Err(BlixardError::ClusterJoin {
            reason: "Discovery-based join failed".to_string(),
        })
    }

    /// Execute P2P join request
    async fn execute_p2p_join_request(
        &self,
        _node_addr: &iroh::NodeAddr,
        _p2p_node_id: &Option<String>,
        _p2p_addresses: &[String],
        _p2p_relay_url: &Option<String>,
    ) -> BlixardResult<(bool, String, Vec<crate::iroh_types::NodeInfo>, Vec<u64>)> {
        // TODO: Fix to handle proper endpoint retrieval
        Err(BlixardError::NotImplemented { 
            feature: "P2P cluster join".to_string() 
        })
    }

    /// Execute HTTP bootstrap join process  
    async fn execute_http_bootstrap_join(
        &self,
        _bootstrap_info: &crate::iroh_types::BootstrapInfo,
        _node_addr: &iroh::NodeAddr,
        _p2p_node_id: &Option<String>,
        _p2p_addresses: &[String],
        _p2p_relay_url: &Option<String>,
    ) -> BlixardResult<()> {
        // TODO: Fix to handle proper endpoint retrieval
        Err(BlixardError::NotImplemented { 
            feature: "HTTP bootstrap join".to_string() 
        })
    }

    /// Update shared state after joining cluster
    async fn update_shared_state_after_join(
        &self,
        _peers: &[crate::iroh_types::NodeInfo],
        _p2p_node_id: &Option<String>,
        _p2p_addresses: &[String],
        _p2p_relay_url: &Option<String>,
    ) -> BlixardResult<()> {
        // TODO: Implement proper state update after join
        // Temporarily stubbed out due to missing endpoint and node_addr variables
        tracing::warn!("update_shared_state_after_join not fully implemented");
        Ok(())
    }

    /// Store peer information from join response
    async fn store_peer_information(&self, peers: &[crate::iroh_types::NodeInfo]) -> BlixardResult<()> {
        // Store all peer P2P info from the response and establish connections
        for peer_info in peers {
            if peer_info.id != self.shared.get_id() && !peer_info.p2p_node_id.is_empty() {
                // Store peer info
                let p2p_peer_info = PeerInfo {
                    node_id: peer_info.p2p_node_id.clone(),
                    address: peer_info.address.clone(),
                    last_seen: chrono::Utc::now(),
                    capabilities: vec![],
                    shared_resources: HashMap::new(),
                    connection_quality: ConnectionQuality {
                        latency_ms: 0,
                        bandwidth_mbps: 100.0,
                        packet_loss: 0.0,
                        reliability_score: 1.0,
                    },
                };
                self.shared
                    .add_peer_with_p2p(peer_info.id, p2p_peer_info);

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
                    let addrs: Vec<SocketAddr> = peer_info
                        .p2p_addresses
                        .iter()
                        .filter_map(|addr_str| addr_str.parse().ok())
                        .collect();
                    if !addrs.is_empty() {
                        node_addr = node_addr.with_direct_addresses(addrs);
                    }

                    // Try to connect
                    tracing::info!("Establishing P2P connection to peer {} after joining cluster", peer_info.id);
                    if let Some(p2p_manager) = self.shared.get_p2p_manager() {
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
        Ok(())
    }

    /// Execute HTTP bootstrap join process
    async fn execute_http_bootstrap_join(
        &self,
        bootstrap_info: &crate::iroh_types::BootstrapInfo,
        node_addr: &iroh::NodeAddr,
        p2p_node_id: &Option<String>,
        p2p_addresses: &[String],
        p2p_relay_url: &Option<String>,
    ) -> BlixardResult<()> {
        // Now connect via P2P
        if let Some(p2p_manager) = self.shared.get_p2p_manager() {
            tracing::info!("Connecting to leader via P2P at {}", node_addr.node_id);
            p2p_manager
                .connect_p2p_peer(bootstrap_info.node_id, node_addr)
                .await?;

            // Create P2P client and send join request
            let (endpoint, _our_node_id) = p2p_manager.get_endpoint();
            let transport_client =
                crate::transport::iroh_client::IrohClusterServiceClient::new(
                    Arc::new(endpoint),
                    node_addr.clone(),
                );

            // Now proceed with the join request via P2P
            let our_node_id = self.shared.get_id();
            let our_bind_address = self.shared.get_bind_addr().to_string();
            let (our_p2p_node_id, our_p2p_addresses, our_p2p_relay_url) =
                if let Some(node_addr) = self.shared.get_p2p_node_addr() {
                    let p2p_id = node_addr.node_id.to_string();
                    let p2p_addrs: Vec<String> = node_addr
                        .direct_addresses()
                        .map(|a| a.to_string())
                        .collect();
                    let relay_url = node_addr.relay_url().map(|u| u.to_string());
                    (Some(p2p_id), p2p_addrs, relay_url)
                } else {
                    (None, Vec::new(), None)
                };

            match transport_client
                .join_cluster(
                    our_node_id,
                    our_bind_address,
                    our_p2p_node_id,
                    our_p2p_addresses,
                    our_p2p_relay_url,
                )
                .await
            {
                Ok((success, message, peers, voters)) => {
                    if success {
                        tracing::info!("Successfully joined cluster via P2P bootstrap");
                        tracing::info!(
                            "Join response contains {} peers and {} voters",
                            peers.len(),
                            voters.len()
                        );

                        // Update local configuration state with current cluster voters
                        self.update_cluster_configuration(&voters).await?;

                        // Store peer information from response
                        self.store_peer_information(&peers).await?;

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

    /// Update cluster configuration with voters
    async fn update_cluster_configuration(&self, voters: &[u64]) -> BlixardResult<()> {
        if !voters.is_empty() {
            tracing::info!("Updating local configuration with voters: {:?}", voters);
            if let Some(db) = self.shared.get_database().await {
                let storage = crate::raft_storage::RedbRaftStorage { database: db };
                let mut conf_state = raft::prelude::ConfState::default();
                conf_state.voters = voters.to_vec();
                if let Err(e) = storage.save_conf_state(&conf_state) {
                    tracing::warn!(
                        "Failed to save initial configuration state: {}",
                        e
                    );
                } else {
                    tracing::info!("Successfully saved initial configuration state with voters: {:?}", voters);
                }
            } else {
                tracing::warn!("No database available to save configuration state");
            }
        } else {
            tracing::warn!("Join response contained empty voters list!");
        }
        Ok(())
    }

    /// Set up transport infrastructure (Raft transport and peer connector)
    async fn setup_transport(
        &mut self,
        message_tx: mpsc::UnboundedSender<(u64, raft::prelude::Message)>,
        _conf_change_tx: mpsc::UnboundedSender<RaftConfChange>,
        proposal_tx: mpsc::UnboundedSender<RaftProposal>,
    ) -> BlixardResult<(Arc<crate::transport::raft_transport_adapter::RaftTransport>, mpsc::UnboundedReceiver<(u64, raft::prelude::Message)>)> {
        // Get transport configuration - using default TransportConfig (Iroh-only now)
        let transport_config = crate::transport::config::TransportConfig::default();

        // Create transport  
        let raft_transport = Arc::new(
            crate::transport::raft_transport_adapter::RaftTransport::new(
                self.shared.clone(),
                message_tx.clone(),
                &transport_config,
            )
            .await?,
        );

        // Store transport for peer management
        // Note: Transport stored implicitly through peer connector and endpoints

        // Get the endpoint from the transport and create peer connector
        let endpoint = raft_transport.endpoint().as_ref().clone();
        let p2p_monitor = Arc::new(crate::p2p_monitor::NoOpMonitor);
        
        let peer_connector = Arc::new(crate::transport::iroh_peer_connector::IrohPeerConnector::new(
            endpoint,
            self.shared.clone(),
            p2p_monitor,
        ));

        let connector_clone = peer_connector.clone();
        tokio::spawn(async move {
            if let Err(e) = connector_clone.start().await {
                tracing::error!("Peer connector error: {}", e);
            }
        });

        self.shared.set_peer_connector(peer_connector);

        // Create message channel for outgoing messages
        let (_outgoing_tx, outgoing_rx) = mpsc::unbounded_channel();
        // Set individual Raft channels
        self.shared.set_raft_proposal_tx(proposal_tx);
        self.shared.set_raft_message_tx(message_tx.clone());

        // Initialize discovery if configured
        self.setup_discovery_if_configured().await?;

        Ok((raft_transport, outgoing_rx))
    }

    /// Set up discovery manager if configured
    async fn setup_discovery_if_configured(&mut self) -> BlixardResult<()> {
        // Discovery is now handled by Iroh's built-in discovery
        // No need for separate discovery manager
        Ok(())
    }

    /// Spawn Raft message handler for outgoing messages
    fn spawn_raft_message_handler(
        &self,
        transport: Arc<crate::transport::raft_transport_adapter::RaftTransport>,
        mut outgoing_rx: mpsc::UnboundedReceiver<(u64, raft::prelude::Message)>,
    ) {
        let shared_weak = Arc::downgrade(&self.shared);
        
        tokio::spawn(async move {
            while let Some((target, msg)) = outgoing_rx.recv().await {
                if let Some(shared) = shared_weak.upgrade() {
                    let result = transport.send_message(target, msg).await;
                    
                    if let Err(e) = result {
                        match &e {
                            BlixardError::NodeNotFound { .. } => {
                                // Node removed from cluster, this is expected
                                tracing::debug!("Cannot send message to removed node {}: {}", target, e);
                            }
                            _ => {
                                tracing::warn!("Failed to send Raft message to {}: {}", target, e);
                                // Could implement retry logic here
                            }
                        }
                    }
                } else {
                    // Node is shutting down
                    break;
                }
            }
            tracing::info!("Raft message handler stopped");
        });
    }

    /// Spawn Raft manager with automatic recovery
    fn spawn_raft_manager_with_recovery(
        &self,
        raft_manager: RaftManager,
    ) -> tokio::task::JoinHandle<BlixardResult<()>> {
        let shared_weak = Arc::downgrade(&self.shared);
        let node_id = self.shared.get_id();
        let shared_for_db = self.shared.clone();
        
        tokio::spawn(async move {
            let mut restart_count = 0;
            let config = match crate::config_global::get() {
                Ok(cfg) => cfg,
                Err(e) => {
                    tracing::error!("Failed to get config for Raft restart: {}", e);
                    return Err(e);
                }
            };
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
            while restart_count <= max_restarts {
                if let Some(shared) = shared_weak.upgrade() {
                    if !shared.is_running() {
                        tracing::info!("Node is stopping, not restarting Raft manager");
                        return Ok(());
                    }

                    // Exponential backoff: 2^(restart_count - 1) * base delay
                    let delay = restart_delay_ms * (1 << (restart_count - 1).min(5));
                    tracing::warn!(
                        "Restarting Raft manager (attempt {}/{}) after {} ms",
                        restart_count, max_restarts, delay
                    );
                    tokio::time::sleep(Duration::from_millis(delay)).await;

                    // Recreate storage and Raft manager
                    let db = shared_for_db.get_database().await;
                    match Self::recreate_raft_manager(node_id, db.clone(), shared.clone()).await {
                        Ok(new_manager) => {
                            tracing::info!("Successfully recreated Raft manager");
                            match new_manager.run().await {
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
                            restart_count += 1;
                        }
                    }
                } else {
                    // Node has been dropped
                    break;
                }
            }

            let error = BlixardError::Internal {
                message: format!("Raft manager failed after {} restart attempts", max_restarts),
            };
            tracing::error!("{}", error);
            Err(error)
        })
    }

    /// Recreate Raft manager for recovery
    async fn recreate_raft_manager(
        node_id: u64,
        db: Option<Arc<Database>>,
        shared: Arc<SharedNodeState>,
    ) -> BlixardResult<RaftManager> {
        let db = db.ok_or_else(|| BlixardError::Internal {
            message: "Database not available for Raft recovery".to_string(),
        })?;

        // Create new channels
        let (proposal_tx, proposal_rx) = mpsc::unbounded_channel();
        let (message_tx, message_rx) = mpsc::unbounded_channel();
        let (conf_change_tx, conf_change_rx): (mpsc::UnboundedSender<RaftConfChange>, mpsc::UnboundedReceiver<RaftConfChange>) = mpsc::unbounded_channel();
        let (outgoing_tx, outgoing_rx): (mpsc::UnboundedSender<(u64, raft::prelude::Message)>, mpsc::UnboundedReceiver<(u64, raft::prelude::Message)>) = mpsc::unbounded_channel();

        // Update shared state with new channels
        // Set individual Raft channels
        shared.set_raft_proposal_tx(proposal_tx.clone());
        shared.set_raft_message_tx(message_tx.clone());

        // Create new Raft manager with correct signature
        let peers = shared.get_peers();
        let peer_list: Vec<(u64, String)> = peers.into_iter().map(|p| {
            // Parse node_id string to u64 - use 0 as fallback
            let id = p.node_id.parse::<u64>().unwrap_or(0);
            (id, p.address)
        }).collect();
        let shared_weak = Arc::downgrade(&shared);
        let (raft_manager, _proposal_tx, _message_tx, _conf_change_tx, _outgoing_tx) = RaftManager::new(
            node_id,
            db,
            peer_list,
            shared_weak,
        )?;

        // Message handling is managed internally by RaftManager

        Ok(raft_manager)
    }

    /// Perform cluster join if configured
    async fn join_cluster_if_configured(&mut self) -> BlixardResult<()> {
        if let Some(join_addr) = &self.shared.config.join_addr.clone() {
            // Wait for Raft to be ready
            self.wait_for_raft_ready().await?;

            // Pre-connect to the node we're joining
            self.pre_connect_to_leader(&join_addr).await?;

            // Send join request
            self.send_join_request().await?;

            // Wait for leader to be identified
            self.wait_for_leader_identification().await?;

            // Register as worker through Raft
            let capabilities = crate::raft_manager::WorkerCapabilities {
                cpu_cores: num_cpus::get() as u32,
                memory_mb: Self::estimate_available_memory(),
                disk_gb: Self::estimate_available_disk(&self.shared.config.data_dir),
                features: match self.shared.config.vm_backend.as_str() {
                    "microvm" => vec!["microvm".to_string()],
                    "docker" => vec!["container".to_string()],
                    "mock" => vec!["mock".to_string()],
                    _ => vec![],
                },
            };
            let topology = self.shared.config.topology.clone();
            self.shared.register_worker_through_raft(
                self.shared.get_id(),
                self.shared.get_bind_addr().to_string(),
                capabilities,
                topology,
            ).await?;
        } else {
            // Bootstrap mode - already registered as worker directly
            tracing::info!("Node {} bootstrapped successfully", self.shared.get_id());
        }

        Ok(())
    }

    /// Wait for Raft manager to be ready
    async fn wait_for_raft_ready(&self) -> BlixardResult<()> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(30);

        while start.elapsed() < timeout {
            if self.shared.is_initialized() {
                tracing::info!("Raft is ready");
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        Err(BlixardError::Internal {
            message: "Timeout waiting for Raft to initialize".to_string(),
        })
    }

    /// Pre-connect to the leader node
    async fn pre_connect_to_leader(&self, join_addr: &str) -> BlixardResult<()> {
        tracing::info!("Pre-connecting to leader at {}", join_addr);
        
        if let Some(peer_connector) = self.shared.get_peer_connector() {
            let parts: Vec<&str> = join_addr.split(':').collect();
            if parts.len() == 2 {
                if let Ok(port) = parts[1].parse::<u16>() {
                    let addr = format!("{}:{}", parts[0], port);
                    // Note: Pre-connection will happen during join process when leader ID is known
                    tracing::debug!("Will connect to {} during join process", addr);
                }
            }
        }
        Ok(())
    }

    /// Wait for leader identification after joining
    async fn wait_for_leader_identification(&self) -> BlixardResult<()> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(30);

        tracing::info!("Waiting for leader identification...");
        
        while start.elapsed() < timeout {
            let raft_status = self.shared.get_raft_status();
            if let Some(leader_id) = raft_status.leader_id {
                tracing::info!("Leader identified: {}", leader_id);
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        Err(BlixardError::Internal {
            message: "Timeout waiting for leader identification".to_string(),
        })
    }

    /// Send a VM command for processing
    pub async fn send_vm_command(&self, command: VmCommand) -> BlixardResult<()> {
        let vm_name = match &command {
            VmCommand::Create { config, .. } => &config.name,
            VmCommand::Start { name } => name,
            VmCommand::Stop { name } => name,
            VmCommand::Delete { name } => name,
            VmCommand::UpdateStatus { name, .. } => name,
            VmCommand::Migrate { task } => &task.vm_name,
        };
        let command_str = format!("{:?}", command);
        self.shared.send_vm_command(vm_name, command_str).await.map(|_| ())
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
    pub async fn list_vms(
        &self,
    ) -> BlixardResult<Vec<(crate::types::VmConfig, crate::types::VmStatus)>> {
        let vm_states = self.shared.list_vms().await?;
        Ok(vm_states.into_iter().map(|state| (state.config, state.status)).collect())
    }

    /// Get status of a specific VM
    pub async fn get_vm_status(
        &self,
        name: &str,
    ) -> BlixardResult<Option<(crate::types::VmConfig, crate::types::VmStatus)>> {
        match self.shared.get_vm_info(name).await {
            Ok(vm_state) => Ok(Some((vm_state.config, vm_state.status))),
            Err(BlixardError::NotFound { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Get the IP address of a VM
    pub async fn get_vm_ip(&self, name: &str) -> BlixardResult<Option<String>> {
        match self.shared.get_vm_ip(name).await {
            Ok(ip) => Ok(Some(ip)),
            Err(BlixardError::NotFound { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Join a cluster and optionally register as a worker
    pub async fn join_cluster(
        &mut self,
        peer_addr: Option<std::net::SocketAddr>,
    ) -> BlixardResult<()> {
        // Check if initialized
        if !self.shared.is_initialized() {
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
            tracing::info!(
                "Joining existing cluster at {:?} - worker registration deferred",
                peer_addr
            );
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
                let worker_table = read_txn.open_table(crate::raft_storage::WORKER_TABLE)?;

                // Check if already registered
                if worker_table
                    .get(node_id.to_le_bytes().as_slice())?
                    .is_none()
                {
                    drop(read_txn);

                    // Not registered yet, register now
                    let write_txn = db.begin_write()?;
                    {
                        let mut worker_table =
                            write_txn.open_table(crate::raft_storage::WORKER_TABLE)?;
                        let mut status_table =
                            write_txn.open_table(crate::raft_storage::WORKER_STATUS_TABLE)?;

                        let worker_data = bincode::serialize(&(address.clone(), &capabilities))?;
                        worker_table
                            .insert(node_id.to_le_bytes().as_slice(), worker_data.as_slice())?;
                        status_table.insert(
                            node_id.to_le_bytes().as_slice(),
                            [crate::raft_manager::WorkerStatus::Online as u8].as_slice(),
                        )?;
                    }
                    write_txn.commit()?;

                    tracing::info!("Node {} registered as worker in bootstrap mode", node_id);
                } else {
                    tracing::info!("Node {} already registered as worker", node_id);
                }
            }
        } else {
            // Join existing cluster via Raft proposal - use the new method
            self.shared
                .register_worker_through_raft(node_id, address, capabilities, topology)
                .await?;
        }

        Ok(())
    }

    /// Leave the cluster
    pub async fn leave_cluster(&mut self) -> BlixardResult<()> {
        // Check if initialized
        if !self.shared.is_initialized() {
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
        self.shared.set_shutdown_tx(shutdown_tx);

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
        self.shared.set_running(true);

        // Start VM health monitor
        if let Some(ref mut monitor) = self.health_monitor {
            monitor.start();
            tracing::info!("VM health monitor started");
        }

        Ok(())
    }

    /// Stop the node
    pub async fn stop(&mut self) -> BlixardResult<()> {
        if let Some(tx) = self.shared.take_shutdown_tx() {
            let _ = tx.send(());
        }

        // Stop main node handle
        if let Some(handle) = self.handle.take() {
            match handle.await {
                Ok(result) => result?,
                Err(_) => {} // Task was cancelled
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

        self.shared.set_running(false);
        self.shared.set_initialized(false);

        // Shutdown all components to release database references
        self.shared.shutdown_components().await;

        // Add a small delay to ensure file locks are released
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        Ok(())
    }

    /// Check if the node is running
    pub async fn is_running(&self) -> bool {
        self.shared.is_running()
    }

    /// Check if this node is the Raft leader
    pub async fn is_leader(&self) -> bool {
        self.shared.is_leader()
    }

    /// Send a Raft message to the Raft manager
    pub async fn send_raft_message(
        &self,
        from: u64,
        msg: raft::prelude::Message,
    ) -> BlixardResult<()> {
        self.shared.send_raft_message(from, msg).await
    }

    /// Submit a task to the cluster
    pub async fn submit_task(
        &self,
        task_id: &str,
        task: crate::raft_manager::TaskSpec,
    ) -> BlixardResult<u64> {
        self.shared.submit_task(task_id, task).await
    }

    /// Get task status
    pub async fn get_task_status(
        &self,
        task_id: &str,
    ) -> BlixardResult<Option<(String, Option<crate::raft_manager::TaskResult>)>> {
        self.shared.get_task_status(task_id).await
    }

    /// Submit VM migration
    pub async fn submit_vm_migration(
        &self,
        task: crate::types::VmMigrationTask,
    ) -> BlixardResult<()> {
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
        crate::config_global::get()
            .map(|cfg| cfg.cluster.worker.default_memory_mb)
            .unwrap_or(4096) // Reasonable default of 4GB
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
        crate::config_global::get()
            .map(|cfg| cfg.cluster.worker.default_disk_gb)
            .unwrap_or(100) // Reasonable default of 100GB
    }

    /// Create discovery configuration from node config
    fn create_discovery_config(
        config: &NodeConfig,
    ) -> BlixardResult<crate::discovery::DiscoveryConfig> {
        let mut discovery_config = crate::discovery::DiscoveryConfig::default();

        // If we have a join address, add it as a static node
        if let Some(join_addr) = &config.join_addr {
            // Try to parse the join address to get the node ID
            // For now, we'll use "bootstrap" as the key
            discovery_config
                .static_nodes
                .insert("bootstrap".to_string(), vec![join_addr.clone()]);
        }

        // Check environment for additional static nodes
        if let Ok(static_nodes) = std::env::var("BLIXARD_STATIC_NODES") {
            // Format: node1=addr1,node2=addr2
            for entry in static_nodes.split(',') {
                if let Some((node_id, addr)) = entry.split_once('=') {
                    discovery_config
                        .static_nodes
                        .insert(node_id.trim().to_string(), vec![addr.trim().to_string()]);
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
    pub async fn initialize_with_vm_registry(
        &mut self,
        registry: VmBackendRegistry,
    ) -> BlixardResult<()> {
        // Call the internal initialization with the custom registry
        self.initialize_internal(Some(registry)).await
    }

    /// Create a VM backend using the factory pattern
    fn create_vm_backend(
        &self,
        registry: &VmBackendRegistry,
        database: Arc<Database>,
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
        std::fs::create_dir_all(&vm_config_dir).map_err(|e| BlixardError::Internal {
            message: format!("Failed to create VM config directory: {}", e),
        })?;
        std::fs::create_dir_all(&vm_data_dir).map_err(|e| BlixardError::Internal {
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
        assert!(node.is_running());

        // Stop node
        node.stop().await.expect("node should stop");
        assert!(!node.is_running());
    }
}
